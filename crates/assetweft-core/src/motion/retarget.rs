use super::digest;
use crate::assurance::{ValidationLimits, canonical_f64, read_bounded};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    path::Path,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LimbSpec {
    name: String,
    upper_len_m: f64,
    lower_len_m: f64,
    max_extension_ratio: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rig {
    height_m: f64,
    limbs: Vec<LimbSpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Contact {
    foot_marker: String,
    stance_frames: [usize; 2],
    max_slip_m: f64,
    loop_tolerance_m: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LimbTarget {
    limb: String,
    hip_pos: [f64; 3],
    end_effector_pos: [f64; 3],
    foot_contact_pos: Option<[f64; 3]>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetargetFrame {
    root_pos: [f64; 3],
    targets: Vec<LimbTarget>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    source_rig: Rig,
    target_rig: Rig,
    contact: Contact,
    frames: Vec<RetargetFrame>,
}

#[derive(Serialize)]
pub struct Report {
    input_sha256: String,
    scale_ratio: Option<f64>,
    pub failed_stage: Option<&'static str>,
    stages: Vec<Stage>,
    max_reach_utilization: Option<f64>,
    max_contact_slip_m: Option<f64>,
    max_loop_error_m: Option<f64>,
    visual_engine_status: &'static str,
}

#[derive(Serialize)]
struct Stage {
    stage: &'static str,
    status: &'static str,
    detail: &'static str,
    frame: Option<usize>,
    limb: Option<String>,
}

struct Failure {
    detail: &'static str,
    frame: Option<usize>,
    limb: Option<String>,
}

impl From<&'static str> for Failure {
    fn from(detail: &'static str) -> Self {
        Self {
            detail,
            frame: None,
            limb: None,
        }
    }
}

impl Failure {
    fn at(detail: &'static str, frame: usize, limb: Option<&str>) -> Self {
        Self {
            detail,
            frame: Some(frame),
            limb: limb.map(str::to_owned),
        }
    }
}

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

impl Rig {
    fn index(&self) -> Result<BTreeMap<&str, &LimbSpec>, &'static str> {
        if !positive(self.height_m) {
            return Err("rig requires a positive finite height");
        }
        let mut limbs = BTreeMap::new();
        for limb in &self.limbs {
            if limb.name.trim().is_empty()
                || !positive(limb.upper_len_m)
                || !positive(limb.lower_len_m)
                || !(0.5..=1.0).contains(&limb.max_extension_ratio)
            {
                return Err(
                    "limbs require a name, positive finite lengths and extension ratio in 0.5..=1.0",
                );
            }
            if limbs.insert(limb.name.as_str(), limb).is_some() {
                return Err("duplicate limb name in rig");
            }
        }
        Ok(limbs)
    }
}

impl Spec {
    fn validate_samples(&mut self) -> Result<(), Box<dyn Error>> {
        let [start, end] = self.contact.stance_frames;
        if self.frames.len() < 2 || start > end || end >= self.frames.len() {
            return Err(
                "stance requires an ordered inclusive range within at least two frames".into(),
            );
        }
        if self.contact.foot_marker.trim().is_empty()
            || [self.contact.max_slip_m, self.contact.loop_tolerance_m]
                .into_iter()
                .any(|value| !value.is_finite() || value < 0.0)
        {
            return Err("contact requires a marker and finite non-negative tolerances".into());
        }
        for frame in &mut self.frames {
            for coordinate in frame
                .root_pos
                .iter_mut()
                .chain(frame.targets.iter_mut().flat_map(|target| {
                    target
                        .hip_pos
                        .iter_mut()
                        .chain(&mut target.end_effector_pos)
                        .chain(target.foot_contact_pos.iter_mut().flatten())
                }))
            {
                *coordinate = canonical_f64(*coordinate)?;
            }
        }
        Ok(())
    }
}

fn distance([x, y, z]: [f64; 3], [a, b, c]: [f64; 3]) -> f64 {
    (x - a).hypot(y - b).hypot(z - c)
}

fn measure(peak: &mut Option<f64>, sample: f64) {
    if sample.is_finite() {
        *peak = Some(peak.unwrap_or(0.0).max(sample));
    }
}

struct Pipeline<'a> {
    spec: &'a Spec,
    source: BTreeMap<&'a str, &'a LimbSpec>,
    target: BTreeMap<&'a str, &'a LimbSpec>,
}

impl Pipeline<'_> {
    fn proportion(&self, report: &mut Report) -> Result<(), Failure> {
        let ratio = self.spec.target_rig.height_m / self.spec.source_rig.height_m;
        report.scale_ratio = canonical_f64(ratio).ok();
        if !(0.25..=4.0).contains(&ratio) {
            return Err("target/source height ratio outside 0.25..=4.0".into());
        }
        if self.source.is_empty() || !self.source.keys().eq(self.target.keys()) {
            return Err("source and target limb names differ".into());
        }
        Ok(())
    }

    fn ik_reach(&self, report: &mut Report) -> Result<(), Failure> {
        for (frame_index, frame) in self.spec.frames.iter().enumerate() {
            let mut seen = BTreeSet::new();
            for target in &frame.targets {
                let fail = |detail| Failure::at(detail, frame_index, Some(&target.limb));
                let limb = self
                    .target
                    .get(target.limb.as_str())
                    .ok_or_else(|| fail("unknown target limb"))?;
                if !seen.insert(target.limb.as_str()) {
                    return Err(fail("duplicate target for limb"));
                }

                let upper = limb.upper_len_m;
                let lower = limb.lower_len_m;
                let reach = distance(target.hip_pos, target.end_effector_pos);
                let ratio = limb.max_extension_ratio;
                let max = (upper + lower) * ratio;
                let min = (upper - lower).abs();
                let utilization = reach / max;
                measure(&mut report.max_reach_utilization, utilization);
                if !positive(max) || !reach.is_finite() || !utilization.is_finite() {
                    return Err(fail("reach exceeds measurable range"));
                }
                if reach < min || reach > max {
                    return Err(fail("reach outside target limb bounds"));
                }

                let scale = upper.max(lower);
                let (a, b, d) = (upper / scale, lower / scale, reach / scale);
                let cosine = (a * a + b * b - d * d) / (2.0 * a * b);
                let bend = cosine.clamp(-1.0, 1.0).acos();
                if !cosine.is_finite() || !bend.is_finite() {
                    return Err(fail("two-bone bend exceeds measurable range"));
                }
            }
            if let Some(&limb) = self.target.keys().find(|name| !seen.contains(*name)) {
                return Err(Failure::at(
                    "missing target for limb",
                    frame_index,
                    Some(limb),
                ));
            }
        }
        Ok(())
    }

    fn contact_slip(&self, report: &mut Report) -> Result<(), Failure> {
        const NANOMETRES_PER_METRE: f64 = 1_000_000_000.0;
        let contact = &self.spec.contact;
        let marker = contact.foot_marker.as_str();
        let [start, end] = contact.stance_frames;
        let mut origin = None;
        for frame in start..=end {
            let [x, _, z] = self.spec.frames[frame]
                .targets
                .iter()
                .find(|target| target.limb == marker)
                .and_then(|target| target.foot_contact_pos)
                .ok_or_else(|| {
                    Failure::at("missing explicit stance contact", frame, Some(marker))
                })?;
            let (a, c) = *origin.get_or_insert((x, z));
            let slip = (x - a).hypot(z - c);
            measure(&mut report.max_contact_slip_m, slip);
            let nanometres = slip * NANOMETRES_PER_METRE;
            let exceeds_limit = if nanometres.is_finite() {
                nanometres.round_ties_even() > contact.max_slip_m * NANOMETRES_PER_METRE
            } else {
                slip > contact.max_slip_m
            };
            if !slip.is_finite() || exceeds_limit {
                return Err(Failure::at(
                    "horizontal stance slip exceeds tolerance",
                    frame,
                    Some(marker),
                ));
            }
        }
        Ok(())
    }

    fn loop_closure(&self, report: &mut Report) -> Result<(), Failure> {
        let first = &self.spec.frames[0];
        let last_index = self.spec.frames.len() - 1;
        let last = &self.spec.frames[last_index];
        let mut check = |delta: f64, limb: Option<&str>| {
            measure(&mut report.max_loop_error_m, delta);
            if !delta.is_finite() || delta > self.spec.contact.loop_tolerance_m {
                Err(Failure::at(
                    "loop endpoint displacement exceeds tolerance",
                    last_index,
                    limb,
                ))
            } else {
                Ok(())
            }
        };
        check(distance(first.root_pos, last.root_pos), None)?;
        let endpoints: BTreeMap<_, _> = last
            .targets
            .iter()
            .map(|target| (target.limb.as_str(), target.end_effector_pos))
            .collect();
        for target in &first.targets {
            let name = target.limb.as_str();
            check(
                distance(target.end_effector_pos, endpoints[name]),
                Some(name),
            )?;
        }
        Ok(())
    }
}

pub fn assess(path: &Path, limits: ValidationLimits) -> Result<Report, Box<dyn Error>> {
    let bytes = read_bounded(path, limits)?;
    let mut spec: Spec = serde_json::from_slice(&bytes)?;
    spec.validate_samples()?;
    let pipeline = Pipeline {
        source: spec.source_rig.index()?,
        target: spec.target_rig.index()?,
        spec: &spec,
    };
    let mut report = Report {
        input_sha256: digest(&bytes),
        scale_ratio: None,
        failed_stage: None,
        stages: Vec::new(),
        max_reach_utilization: None,
        max_contact_slip_m: None,
        max_loop_error_m: None,
        visual_engine_status: "NOT_RUN",
    };
    type Check<'a> = fn(&Pipeline<'a>, &mut Report) -> Result<(), Failure>;
    let stages: [(&str, &str, Check<'_>); 4] = [
        (
            "proportion",
            "rig heights and limb correspondence verified",
            Pipeline::proportion,
        ),
        (
            "ik_reach",
            "all frame targets satisfy the two-bone reach and bend checks",
            Pipeline::ik_reach,
        ),
        (
            "contact_slip",
            "explicit stance contact stays within horizontal tolerance",
            Pipeline::contact_slip,
        ),
        (
            "loop_closure",
            "root and limb endpoints close within tolerance",
            Pipeline::loop_closure,
        ),
    ];
    for (stage, success, check) in stages {
        let mut trace = Stage {
            stage,
            status: "NOT_RUN",
            detail: "blocked by an earlier stage",
            frame: None,
            limb: None,
        };
        if report.failed_stage.is_none() {
            match check(&pipeline, &mut report) {
                Ok(()) => {
                    trace.status = "PASS";
                    trace.detail = success;
                }
                Err(failure) => {
                    report.failed_stage = Some(stage);
                    trace.status = "FAIL";
                    trace.detail = failure.detail;
                    trace.frame = failure.frame;
                    trace.limb = failure.limb;
                }
            }
        }
        report.stages.push(trace);
    }
    Ok(report)
}
