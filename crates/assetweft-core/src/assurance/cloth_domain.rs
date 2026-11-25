use super::{canonical_f64, semantic_sha256_hex};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GarmentBinding {
    Skinned,
    Rigid,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Spec {
    pub garments: Vec<Garment>,
    sampled_motion_poses: Vec<Pose>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Garment {
    pub node_name: String,
    pub binding_kind: GarmentBinding,
    pub body_mesh_node: String,
    max_weight_sum_error: f64,
    max_stretch_ratio: f64,
    max_penetration_m: f64,
    pub require_supported_structure: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pose {
    node_name: Option<String>,
    frame: u32,
    body_vertices: Vec<[f64; 3]>,
    garment_vertices: Vec<[f64; 3]>,
    reference_edge_lengths_m: Vec<f64>,
    rest_garment_edges: Vec<[usize; 2]>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct Motion {
    penetration: (f64, u32),
    stretch: (f64, u32),
}

#[derive(Default)]
pub(super) struct BindingFacts {
    pub classification: Option<GarmentBinding>,
    pub unsupported: BTreeSet<String>,
    pub invalid_weights: bool,
    pub weight_sum_error: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub enum ClothViolation {
    BindingMismatch,
    UnsupportedStructure,
    InvalidSkinWeights,
    ClothPenetration,
    ExcessiveDeformation,
}

#[derive(Debug, Serialize)]
pub struct ClothGarmentReport {
    pub node_name: String,
    pub body_mesh_node: String,
    pub binding_kind: GarmentBinding,
    pub garment_classification: Option<GarmentBinding>,
    pub status: &'static str,
    pub unsupported_structures: BTreeSet<String>,
    pub max_weight_sum_error: f64,
    pub max_penetration_m: f64,
    pub max_stretch_ratio: f64,
    pub max_penetration_frame: u32,
    pub max_stretch_frame: u32,
    pub violations: Vec<ClothViolation>,
}

#[derive(Debug, Serialize)]
pub struct ClothReport {
    pub status: &'static str,
    pub garments: Vec<ClothGarmentReport>,
    pub unsupported_structures: BTreeSet<String>,
    pub max_weight_sum_error: f64,
    pub max_penetration_m: f64,
    pub max_stretch_ratio: f64,
    pub semantic_hash: String,
    pub visual_engine_status: &'static str,
}

impl Spec {
    pub(super) fn measure(&self) -> Result<Vec<Motion>, String> {
        if self.garments.is_empty() {
            return Err("garments must not be empty".into());
        }
        let mut garment_names = BTreeSet::new();
        for garment in &self.garments {
            if garment.node_name.trim().is_empty()
                || garment.body_mesh_node.trim().is_empty()
                || garment.node_name == garment.body_mesh_node
                || !garment_names.insert(garment.node_name.as_str())
            {
                return Err(
                    "garment names must be unique and distinct from their body mesh".into(),
                );
            }
            for threshold in [
                garment.max_weight_sum_error,
                garment.max_stretch_ratio,
                garment.max_penetration_m,
            ] {
                if finite(threshold)? < 0.0 {
                    return Err("cloth thresholds must be nonnegative".into());
                }
            }
        }

        let mut series: BTreeMap<Option<&str>, Vec<&Pose>> = BTreeMap::new();
        for pose in &self.sampled_motion_poses {
            if pose
                .node_name
                .as_deref()
                .is_some_and(|name| !garment_names.contains(name))
            {
                return Err("motion sample names an undeclared garment".into());
            }
            series
                .entry(pose.node_name.as_deref())
                .or_default()
                .push(pose);
        }
        if series.contains_key(&None) && series.len() > 1 {
            return Err("motion samples must be all shared or all garment-specific".into());
        }
        if let Some(shared) = series.get_mut(&None) {
            return Ok(vec![measure_series(shared)?; self.garments.len()]);
        }
        self.garments
            .iter()
            .map(|garment| {
                let poses = series
                    .get_mut(&Some(garment.node_name.as_str()))
                    .ok_or("each garment needs at least two motion samples")?;
                measure_series(poses)
            })
            .collect()
    }
}

fn finite(value: f64) -> Result<f64, String> {
    canonical_f64(value).map_err(|err| err.to_string())
}

fn measure_series(poses: &mut [&Pose]) -> Result<Motion, String> {
    if poses.len() < 2 {
        return Err("each motion series needs at least two distinct frames".into());
    }
    poses.sort_unstable_by_key(|pose| pose.frame);
    let first = poses[0];
    if poses.windows(2).any(|pair| pair[0].frame == pair[1].frame) {
        return Err("each motion series needs at least two distinct frames".into());
    }
    if first.garment_vertices.is_empty()
        || first.rest_garment_edges.is_empty()
        || first.rest_garment_edges.len() != first.reference_edge_lengths_m.len()
        || first.rest_garment_edges.iter().any(|&[a, b]| {
            a == b || a >= first.garment_vertices.len() || b >= first.garment_vertices.len()
        })
    {
        return Err("motion series needs garment vertices and valid rest edges and lengths".into());
    }
    for &length in &first.reference_edge_lengths_m {
        if finite(length)? <= 0.0 {
            return Err("reference edge lengths must be positive".into());
        }
    }

    let mut motion = Motion {
        penetration: (0.0, first.frame),
        stretch: (0.0, first.frame),
    };
    for pose in poses {
        if pose.garment_vertices.len() != first.garment_vertices.len()
            || pose.body_vertices.len() != first.body_vertices.len()
            || pose.rest_garment_edges != first.rest_garment_edges
            || pose.reference_edge_lengths_m != first.reference_edge_lengths_m
        {
            return Err(
                "motion series must preserve vertex counts and rest edges and lengths".into(),
            );
        }
        let mut low = [f64::INFINITY; 3];
        let mut high = [f64::NEG_INFINITY; 3];
        for point in &pose.body_vertices {
            for axis in 0..3 {
                let value = finite(point[axis])?;
                low[axis] = low[axis].min(value);
                high[axis] = high[axis].max(value);
            }
        }
        if (0..3).any(|axis| low[axis] >= high[axis]) {
            return Err("body proxy must enclose a nonzero volume in every frame".into());
        }
        for point in &pose.garment_vertices {
            let mut depth = f64::INFINITY;
            for axis in 0..3 {
                let value = finite(point[axis])?;
                depth = depth.min((value - low[axis]).min(high[axis] - value));
            }
            let depth = finite(depth.max(0.0))?;
            if depth > motion.penetration.0 {
                motion.penetration = (depth, pose.frame);
            }
        }
        for (&[a, b], &rest) in pose
            .rest_garment_edges
            .iter()
            .zip(&pose.reference_edge_lengths_m)
        {
            let a = pose.garment_vertices[a];
            let b = pose.garment_vertices[b];
            let length = (a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2]);
            let ratio = finite((length - rest).abs() / rest)?;
            if ratio > motion.stretch.0 {
                motion.stretch = (ratio, pose.frame);
            }
        }
    }
    Ok(motion)
}

impl Garment {
    pub(super) fn report(self, facts: BindingFacts, motion: Motion) -> ClothGarmentReport {
        use ClothViolation::*;
        let violations = [
            (
                facts.classification != Some(self.binding_kind),
                BindingMismatch,
            ),
            (!facts.unsupported.is_empty(), UnsupportedStructure),
            (
                facts.invalid_weights || facts.weight_sum_error > self.max_weight_sum_error,
                InvalidSkinWeights,
            ),
            (
                motion.penetration.0 > self.max_penetration_m,
                ClothPenetration,
            ),
            (
                motion.stretch.0 > self.max_stretch_ratio,
                ExcessiveDeformation,
            ),
        ]
        .into_iter()
        .filter_map(|(breached, violation)| breached.then_some(violation))
        .collect::<Vec<_>>();
        ClothGarmentReport {
            node_name: self.node_name,
            body_mesh_node: self.body_mesh_node,
            binding_kind: self.binding_kind,
            garment_classification: facts.classification,
            status: if !facts.unsupported.is_empty() {
                "UNSUPPORTED"
            } else if violations.is_empty() {
                "PASS"
            } else {
                "FAIL"
            },
            unsupported_structures: facts.unsupported,
            max_weight_sum_error: facts.weight_sum_error,
            max_penetration_m: motion.penetration.0,
            max_stretch_ratio: motion.stretch.0,
            max_penetration_frame: motion.penetration.1,
            max_stretch_frame: motion.stretch.1,
            violations,
        }
    }
}

impl ClothReport {
    pub(super) fn from_garments(garments: Vec<ClothGarmentReport>) -> Result<Self, String> {
        let mut digest = Vec::new();
        for part in &garments {
            let labels = serde_json::to_vec(&(
                &part.node_name,
                &part.body_mesh_node,
                part.binding_kind,
                part.garment_classification,
                &part.unsupported_structures,
                &part.violations,
            ))
            .map_err(|err| err.to_string())?;
            digest.push(labels.len() as f64);
            digest.extend(labels.into_iter().map(f64::from));
            digest.extend([
                part.max_weight_sum_error,
                part.max_penetration_m,
                part.max_stretch_ratio,
                f64::from(part.max_penetration_frame),
                f64::from(part.max_stretch_frame),
            ]);
        }
        let max =
            |value: fn(&ClothGarmentReport) -> f64| garments.iter().map(value).fold(0.0, f64::max);
        Ok(Self {
            status: if garments.iter().all(|part| part.violations.is_empty()) {
                "PASS"
            } else {
                "FAIL"
            },
            unsupported_structures: garments
                .iter()
                .flat_map(|part| part.unsupported_structures.iter().cloned())
                .collect(),
            max_weight_sum_error: max(|part| part.max_weight_sum_error),
            max_penetration_m: max(|part| part.max_penetration_m),
            max_stretch_ratio: max(|part| part.max_stretch_ratio),
            semantic_hash: semantic_sha256_hex(&digest).map_err(|err| err.to_string())?,
            visual_engine_status: "NOT_RUN",
            garments,
        })
    }
}
