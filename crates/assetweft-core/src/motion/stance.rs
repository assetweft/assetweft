use super::digest;
use crate::assurance::{ValidationLimits, read_bounded};
use serde::{Deserialize, Serialize};
use std::{error::Error, path::Path};

const SLIP_LIMIT_M: f64 = 0.01;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MotionRole {
    GroundedHumanoid,
    Hovering,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Stance {
    contact: String,
    frames: [usize; 2],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Track {
    marker: String,
    world_y_up_m: Vec<Option<[f64; 3]>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Clip {
    stance: Option<Stance>,
    tracks: Vec<Track>,
}

#[derive(Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    Pass,
    Fail,
    Inapplicable,
    NotRun,
}

#[derive(Serialize)]
pub struct Report {
    pub status: Status,
    rule: &'static str,
    declared_role: MotionRole,
    input_sha256: String,
    declared_stance: Option<Stance>,
    limit_m: f64,
    max_slip_m: Option<f64>,
    samples_evaluated: usize,
    reason: Option<&'static str>,
}

fn measure(stance: Option<&Stance>, tracks: &[Track]) -> Result<(f64, usize), &'static str> {
    let stance = stance.ok_or("missing stance declaration")?;
    let [start, end] = stance.frames;
    if start >= end || stance.contact.trim().is_empty() {
        return Err("stance requires a named contact and at least two ordered frames");
    }
    let mut matches = tracks.iter().filter(|track| track.marker == stance.contact);
    let track = matches.next().ok_or("declared contact track is missing")?;
    if matches.next().is_some() {
        return Err("declared contact matches multiple tracks");
    }
    let samples = track
        .world_y_up_m
        .get(start..=end)
        .ok_or("contact track does not cover stance")?;
    let [x, _, z] = samples[0].ok_or("missing contact sample in stance")?;
    let mut peak = 0.0_f64;
    for sample in samples {
        let [sx, _, sz] = sample.ok_or("missing contact sample in stance")?;
        let distance = (sx - x).hypot(sz - z);
        if !distance.is_finite() {
            return Err("horizontal distance exceeds measurable range");
        }
        peak = peak.max(distance);
    }
    Ok((peak, samples.len()))
}

/// Check a declared support foot against a one-centimetre horizontal slip limit.
///
/// # Errors
/// Returns an error on unreadable, oversized or malformed input. Incomplete
/// measurement evidence returns a report with a reason.
pub fn assess(
    path: &Path,
    role: MotionRole,
    limits: ValidationLimits,
) -> Result<Report, Box<dyn Error>> {
    let bytes = read_bounded(path, limits)?;
    let clip: Clip = serde_json::from_slice(&bytes)?;
    let (status, max_slip_m, samples_evaluated, reason) = match role {
        MotionRole::Hovering => (
            Status::Inapplicable,
            None,
            0,
            Some("declared hovering motion has no required planted foot"),
        ),
        MotionRole::GroundedHumanoid => match measure(clip.stance.as_ref(), &clip.tracks) {
            Ok((peak, count)) => (
                if peak > SLIP_LIMIT_M {
                    Status::Fail
                } else {
                    Status::Pass
                },
                Some(peak),
                count,
                None,
            ),
            Err(reason) => (Status::NotRun, None, 0, Some(reason)),
        },
    };
    Ok(Report {
        status,
        rule: "stance_horizontal_slip",
        declared_role: role,
        input_sha256: digest(&bytes),
        declared_stance: clip.stance,
        limit_m: SLIP_LIMIT_M,
        max_slip_m,
        samples_evaluated,
        reason,
    })
}
