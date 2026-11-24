use super::determinism_domain::{DeterminismError, canonical_vec3, quantize_1e6_units};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KitKind {
    Architecture,
    Foliage,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Spec {
    kind: KitKind,
    expected_unit_scale: f64,
    scale_tolerance: f64,
    sockets: Vec<Socket>,
    allow_non_watertight_foliage: bool,
    budget_before: Option<Resources>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Socket {
    name: String,
    position: [f64; 3],
    seam_tolerance_m: f64,
}

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    pub vertex_count: u64,
    pub triangle_count: u64,
    pub texture_bytes: u64,
}

#[derive(Serialize)]
pub struct OptimizationRecord {
    pub before: Option<Resources>,
    pub after: Resources,
    #[serde(serialize_with = "serialize_delta")]
    pub delta_triangles: Option<i128>,
    #[serde(serialize_with = "serialize_delta")]
    pub delta_bytes: Option<i128>,
    pub visual_quality_status: &'static str,
}

fn serialize_delta<S: serde::Serializer>(
    value: &Option<i128>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        None => serializer.serialize_none(),
        Some(value) => match i64::try_from(*value) {
            Ok(value) => serializer.serialize_i64(value),
            Err(_) => serializer.collect_str(value),
        },
    }
}

#[derive(Serialize)]
pub struct StaticKitReport {
    pub scope: &'static str,
    pub kind: KitKind,
    pub primitive_count: u64,
    #[serde(flatten)]
    pub resources: Resources,
    pub bounding_box_min: [f64; 3],
    pub bounding_box_max: [f64; 3],
    pub double_sided_material_count: usize,
    pub alpha_modes: BTreeSet<&'static str>,
    pub unclosed_boundary_edges: u64,
    pub non_manifold_edges: u64,
    pub degenerate_triangles: u64,
    pub optimization_record: OptimizationRecord,
    pub visual_quality_status: &'static str,
    pub violations: Vec<Violation>,
}

#[derive(Serialize)]
pub struct Violation {
    pub field: &'static str,
    pub detail: String,
}

pub(super) type Point = [i64; 3];
pub(super) type Matrix = [[f64; 4]; 4];

pub(super) fn point_units(point: [f64; 3]) -> Result<Point, DeterminismError> {
    let [x, y, z] = point;
    Ok([
        quantize_1e6_units(x)?,
        quantize_1e6_units(y)?,
        quantize_1e6_units(z)?,
    ])
}

pub(super) fn multiply(parent: Matrix, local: Matrix) -> Matrix {
    local.map(|column| std::array::from_fn(|row| (0..4).map(|i| parent[i][row] * column[i]).sum()))
}

pub(super) fn transform(matrix: Matrix, [x, y, z]: [f32; 3]) -> [f64; 3] {
    let [x, y, z] = [x, y, z].map(f64::from);
    std::array::from_fn(|i| matrix[0][i] * x + matrix[1][i] * y + matrix[2][i] * z + matrix[3][i])
}

#[derive(Default)]
pub(super) struct Topology {
    edges: BTreeMap<(Point, Point), (u64, u64)>,
    pub degenerate_triangles: u64,
}

impl Topology {
    pub fn add(&mut self, [a, b, c]: [Point; 3]) {
        if a == b || b == c || c == a {
            self.degenerate_triangles += 1;
            return;
        }
        for (start, end) in [(a, b), (b, c), (c, a)] {
            if start < end {
                self.edges.entry((start, end)).or_default().0 += 1;
            } else {
                self.edges.entry((end, start)).or_default().1 += 1;
            }
        }
    }

    pub fn edge_counts(&self) -> (u64, u64) {
        let mut boundary = 0;
        let mut non_manifold = 0;
        for &(forward, reverse) in self.edges.values() {
            match (forward, reverse) {
                (1, 0) | (0, 1) => boundary += 1,
                (1, 1) => {}
                _ => non_manifold += 1,
            }
        }
        (boundary, non_manifold)
    }
}

pub(super) struct Facts {
    pub primitive_count: u64,
    pub resources: Resources,
    pub bounds: [[f64; 3]; 2],
    pub materials: BTreeMap<Option<usize>, (bool, &'static str)>,
    pub boundary: u64,
    pub non_manifold: u64,
    pub degenerate: u64,
    pub mesh_scales: Vec<(usize, Point)>,
    pub named_origins: BTreeMap<String, Option<Point>>,
}

pub(super) struct Contract {
    kind: KitKind,
    expected_scale: i64,
    scale_tolerance: i64,
    sockets: Vec<(String, Point, u128)>,
    allow_open: bool,
    before: Option<Resources>,
}

impl Spec {
    pub fn validate(self) -> Result<Contract, String> {
        let units = |value| quantize_1e6_units(value).map_err(|e| e.to_string());
        let expected_scale = units(self.expected_unit_scale)?;
        let scale_tolerance = units(self.scale_tolerance)?;
        if self.expected_unit_scale <= 0.0 || self.scale_tolerance <= 0.0 {
            return Err("expected_unit_scale and scale_tolerance must be positive".into());
        }
        if self.kind == KitKind::Architecture && self.allow_non_watertight_foliage {
            return Err("allow_non_watertight_foliage requires kind foliage".into());
        }
        let mut names = BTreeSet::new();
        let mut sockets = Vec::new();
        for socket in self.sockets {
            if socket.name.trim().is_empty() || !names.insert(socket.name.clone()) {
                return Err("socket names must be nonempty and unique".into());
            }
            if socket.seam_tolerance_m < 0.0 {
                return Err("seam_tolerance_m must be nonnegative".into());
            }
            let position = point_units(socket.position).map_err(|e| e.to_string())?;
            sockets.push((
                socket.name,
                position,
                units(socket.seam_tolerance_m)? as u128,
            ));
        }
        Ok(Contract {
            kind: self.kind,
            expected_scale,
            scale_tolerance,
            sockets,
            allow_open: self.allow_non_watertight_foliage,
            before: self.budget_before,
        })
    }
}

impl Contract {
    pub fn inspect(self, facts: Facts) -> StaticKitReport {
        let mut violations = Vec::new();
        let mut fail = |field, detail| violations.push(Violation { field, detail });
        for (node, scale) in facts.mesh_scales {
            if scale
                .iter()
                .any(|v| *v <= 0 || v.abs_diff(self.expected_scale) > self.scale_tolerance as u64)
            {
                fail(
                    "scale",
                    format!("mesh node {node} scale exceeds scale_tolerance"),
                );
            }
        }
        for (name, expected, tolerance) in self.sockets {
            match facts.named_origins.get(&name) {
                Some(Some(actual)) => {
                    let offsets = std::array::from_fn::<_, 3, _>(|axis| {
                        u128::from(actual[axis].abs_diff(expected[axis]))
                    });
                    if offsets.iter().any(|&offset| offset > tolerance)
                        || offsets.iter().map(|offset| offset * offset).sum::<u128>()
                            > tolerance * tolerance
                    {
                        fail("seam", format!("socket {name:?} exceeds seam_tolerance_m"));
                    }
                }
                _ => fail(
                    "seam",
                    format!("socket {name:?} must match exactly one node"),
                ),
            }
        }
        if !self.allow_open
            && (facts.boundary > 0 || facts.non_manifold > 0 || facts.degenerate > 0)
        {
            fail(
                "watertight",
                "mesh has open or non-manifold edges or collapsed triangles".into(),
            );
        }
        let after = facts.resources;
        let delta = |before, after| i128::from(after) - i128::from(before);
        StaticKitReport {
            scope: "inspected_static_kit",
            kind: self.kind,
            primitive_count: facts.primitive_count,
            resources: after,
            bounding_box_min: facts.bounds[0],
            bounding_box_max: facts.bounds[1],
            double_sided_material_count: facts.materials.values().filter(|(side, _)| *side).count(),
            alpha_modes: facts.materials.values().map(|(_, mode)| *mode).collect(),
            unclosed_boundary_edges: facts.boundary,
            non_manifold_edges: facts.non_manifold,
            degenerate_triangles: facts.degenerate,
            optimization_record: OptimizationRecord {
                before: self.before,
                after,
                delta_triangles: self
                    .before
                    .map(|b| delta(b.triangle_count, after.triangle_count)),
                delta_bytes: self
                    .before
                    .map(|b| delta(b.texture_bytes, after.texture_bytes)),
                visual_quality_status: "NOT_RUN",
            },
            visual_quality_status: "NOT_RUN",
            violations,
        }
    }
}

pub(super) fn canonical_bounds(bounds: [[f64; 3]; 2]) -> Result<[[f64; 3]; 2], DeterminismError> {
    let [min, max] = bounds;
    Ok([canonical_vec3(min)?, canonical_vec3(max)?])
}
