//! Responsibility: bounded, read-only inspection of bound GLB skin numeric data.
//! Dependencies: validation for input/format/hash, gltf 1.4.1 utils for decoding.
//! Callers: library consumers and rig integration tests; no application wiring yet.
//! Invariants: never resolve URIs; unsupported data is NOT_RUN; format and numeric
//! results are separate; no pose, semantic digest, visual or engine claims.
use crate::validation::{
    self, CheckItem, CheckStatus, RoleRequirement, SourceClass, SourceDeclaration, ValidationError,
    ValidationLimits, ValidationReport,
};
use gltf::{
    accessor::{DataType, Dimensions},
    mesh::Semantic,
};
use std::{
    collections::{BTreeSet, VecDeque},
    path::Path,
};

#[derive(Debug)]
pub struct RigReport {
    /// Original report, including its unchanged numeric NOT_RUN entries.
    pub validation: ValidationReport,
    pub checks: Vec<CheckItem>,
    pub bound_nodes: Vec<usize>,
    pub not_run: &'static [&'static str],
}
impl RigReport {
    fn add(&mut self, id: &'static str, status: CheckStatus, detail: impl Into<String>) {
        self.checks.push(CheckItem {
            id,
            status,
            detail: detail.into(),
        });
    }
}
/// Read only the supplied file, applying the existing input byte limit.
/// # Errors
/// Returns the existing bounded-read error on IO, non-file or size violations.
pub fn inspect_file(path: &Path, limits: ValidationLimits) -> Result<RigReport, ValidationError> {
    Ok(inspect_bytes(&validation::read_bounded(path, limits)?))
}
/// Inspect already resident GLB bytes. File callers must use `inspect_file`.
/// Float sum policy follows the glTF 2.0.1 validator implementation note:
/// tolerance = 2e-7 times nonzero influences; this is not an industry tolerance.
pub fn inspect_bytes(bytes: &[u8]) -> RigReport {
    use CheckStatus::{Fail, NotRun, Pass};
    let mut r = RigReport {
        validation: validation::validate_bytes(
            bytes,
            RoleRequirement::StaticMesh,
            SourceDeclaration::declared(SourceClass::Unknown),
        ),
        checks: vec![],
        bound_nodes: vec![],
        not_run: &[
            "pose_mesh_deformation",
            "joint_common_root_scene_membership",
            "rest_transforms",
            "semantic_digest",
            "F0",
            "AI_generation",
            "visual",
            "engine",
        ],
    };
    if r.validation.format_status != Pass {
        r.add(
            "rig.numeric",
            NotRun,
            "format/references rejected; see validation checks for field paths",
        );
        return r;
    }
    let g = match gltf::Gltf::from_slice(bytes) {
        Ok(g) => g,
        Err(e) => {
            r.add("rig.numeric", NotRun, e.to_string());
            return r;
        }
    };
    // Iterative traversal avoids stack overflow on deeply nested hostile trees.
    let mut indegree = vec![0usize; g.nodes().len()];
    let mut graph_ok = true;
    for n in g.nodes() {
        for c in n.children() {
            indegree[c.index()] += 1;
            if indegree[c.index()] > 1 {
                graph_ok = false;
                r.add(
                    "rig.hierarchy",
                    Fail,
                    format!(
                        "nodes[{}].children: nodes[{}] has multiple incoming edges",
                        n.index(),
                        c.index()
                    ),
                );
            }
        }
    }
    let mut queue: VecDeque<_> = indegree
        .iter()
        .enumerate()
        .filter_map(|(i, &d)| (d == 0).then_some(i))
        .collect();
    let nodes: Vec<_> = g.nodes().collect();
    while let Some(i) = queue.pop_front() {
        for c in nodes[i].children() {
            indegree[c.index()] -= 1;
            if indegree[c.index()] == 0 {
                queue.push_back(c.index());
            }
        }
    }
    if let Some(i) = indegree.iter().position(|&d| d != 0) {
        graph_ok = false;
        r.add(
            "rig.hierarchy",
            Fail,
            format!("nodes[{i}].children: cycle or descendant of cycle"),
        );
    }
    if graph_ok {
        r.add(
            "rig.hierarchy",
            Pass,
            "all node child references form a forest",
        );
    }
    let extensions: Vec<_> = g.extensions_used().chain(g.extensions_required()).collect();
    for node in g.nodes() {
        let (Some(mesh), Some(skin)) = (node.mesh(), node.skin()) else {
            continue;
        };
        r.bound_nodes.push(node.index());
        let loc = format!(
            "nodes[{}].skin={} meshes[{}]",
            node.index(),
            skin.index(),
            mesh.index()
        );
        if !extensions.is_empty() {
            r.add(
                "rig.numeric",
                NotRun,
                format!("{loc}: extension semantics unsupported: {extensions:?}"),
            );
            continue;
        }
        let joints: Vec<_> = skin.joints().map(|j| j.index()).collect();
        if joints.is_empty() || joints.iter().collect::<BTreeSet<_>>().len() != joints.len() {
            r.add(
                "rig.joints",
                Fail,
                format!("skins[{}].joints: empty or duplicate nodes", skin.index()),
            );
        }
        let ibloc = format!("skins[{}].inverseBindMatrices", skin.index());
        if let Some(a) = skin.inverse_bind_matrices() {
            if a.data_type() != DataType::F32
                || a.dimensions() != Dimensions::Mat4
                || a.normalized()
                || a.count() < joints.len()
            {
                r.add(
                    "rig.bind_matrices",
                    Fail,
                    format!("{ibloc}: requires FLOAT MAT4, count >= {}", joints.len()),
                );
            } else if ready(&a, g.blob.as_deref(), false, &ibloc, &mut r) {
                let reader = skin.reader(|b| bin(b, g.blob.as_deref()));
                if let Some(values) = reader.read_inverse_bind_matrices() {
                    let mut ok = true;
                    for (i, m) in values.enumerate() {
                        if !m.iter().flatten().all(|x| x.is_finite())
                            || [m[0][3], m[1][3], m[2][3], m[3][3]] != [0., 0., 0., 1.]
                        {
                            ok = false;
                            r.add(
                                "rig.bind_matrices",
                                Fail,
                                format!("{ibloc}[{i}]: nonfinite value or fourth row != [0,0,0,1]"),
                            );
                            break;
                        }
                    }
                    if ok {
                        r.add(
                            "rig.bind_matrices",
                            Pass,
                            format!("{ibloc}: decoded finite affine matrices"),
                        );
                    }
                } else {
                    r.add(
                        "rig.bind_matrices",
                        NotRun,
                        format!("{ibloc}: reader unavailable"),
                    );
                }
            }
        } else {
            r.add(
                "rig.bind_matrices",
                Pass,
                format!("{ibloc}: omitted; glTF identity default"),
            );
        }
        for p in mesh.primitives() {
            let ploc = format!("{loc}.primitives[{}]", p.index());
            let js: BTreeSet<_> = p
                .attributes()
                .filter_map(|(s, _)| {
                    if let Semantic::Joints(n) = s {
                        Some(n)
                    } else {
                        None
                    }
                })
                .collect();
            let ws: BTreeSet<_> = p
                .attributes()
                .filter_map(|(s, _)| {
                    if let Semantic::Weights(n) = s {
                        Some(n)
                    } else {
                        None
                    }
                })
                .collect();
            if js.is_empty() || js != ws || !js.contains(&0) {
                r.add(
                    "rig.attributes",
                    Fail,
                    format!("{ploc}: JOINTS_n {js:?} / WEIGHTS_n {ws:?} must pair starting at 0"),
                );
                continue;
            }
            if js.len() != 1 {
                r.add(
                    "rig.weights",
                    NotRun,
                    format!("{ploc}: multiple influence sets unsupported; no partial sum checked"),
                );
                continue;
            }
            let j = p.get(&Semantic::Joints(0)).expect("checked set");
            let w = p.get(&Semantic::Weights(0)).expect("checked set");
            let count = p.get(&Semantic::Positions).map(|a| a.count());
            if j.dimensions() != Dimensions::Vec4
                || !matches!(j.data_type(), DataType::U8 | DataType::U16)
                || j.normalized()
                || w.dimensions() != Dimensions::Vec4
                || !matches!(
                    (w.data_type(), w.normalized()),
                    (DataType::F32, false) | (DataType::U8 | DataType::U16, true)
                )
                || j.count() != w.count()
                || count != Some(j.count())
            {
                r.add(
                    "rig.attributes",
                    Fail,
                    format!(
                        "{ploc}: JOINTS_0/WEIGHTS_0 invalid type, normalization or POSITION count"
                    ),
                );
                continue;
            }
            let jr = ready(
                &j,
                g.blob.as_deref(),
                true,
                &format!("{ploc}.JOINTS_0"),
                &mut r,
            );
            let wr = ready(
                &w,
                g.blob.as_deref(),
                true,
                &format!("{ploc}.WEIGHTS_0"),
                &mut r,
            );
            if !jr || !wr {
                continue;
            }
            let reader = p.reader(|b| bin(b, g.blob.as_deref()));
            let (Some(ji), Some(wi)) = (reader.read_joints(0), reader.read_weights(0)) else {
                r.add("rig.weights", NotRun, format!("{ploc}: reader unavailable"));
                continue;
            };
            use gltf::mesh::util::ReadWeights;
            // Keep integer sums exact; do not round-trip through normalized f32.
            let (weights, target): (Box<dyn Iterator<Item = [f64; 4]> + '_>, f64) = match wi {
                ReadWeights::U8(it) => (Box::new(it.map(|v| v.map(f64::from))), 255.),
                ReadWeights::U16(it) => (Box::new(it.map(|v| v.map(f64::from))), 65535.),
                ReadWeights::F32(it) => (Box::new(it.map(|v| v.map(f64::from))), 1.),
            };
            let mut ok = true;
            for (v, (indices, weights)) in ji.into_u16().zip(weights).enumerate() {
                let mut seen = BTreeSet::new();
                for k in 0..4 {
                    if usize::from(indices[k]) >= joints.len()
                        || !weights[k].is_finite()
                        || weights[k] < 0.
                        || (weights[k] > 0. && !seen.insert(indices[k]))
                    {
                        ok = false;
                        r.add("rig.weights",Fail,format!("{ploc}.JOINTS_0/WEIGHTS_0 vertex[{v}][{k}]: joint={}, weight={}; out of range, nonfinite, negative or duplicate influence",indices[k],weights[k]));
                        break;
                    }
                }
                if !ok {
                    break;
                }
                let sum: f64 = weights.iter().sum();
                let tolerance = if target == 1. {
                    2e-7 * weights.iter().filter(|&&x| x > 0.).count() as f64
                } else {
                    0.
                };
                if (sum - target).abs() > tolerance {
                    ok = false;
                    r.add("rig.weights",Fail,format!("{ploc}.WEIGHTS_0 vertex[{v}]: sum={sum}, target={target}, tolerance={tolerance} (float validator policy / exact quantized sum)"));
                    break;
                }
            }
            if ok {
                r.add("rig.weights",Pass,format!("{ploc}: decoded {} vertices, joint ranges, finite nonnegative unique influences and weight sums",j.count()));
            }
        }
    }
    if r.bound_nodes.is_empty() {
        r.add(
            "rig.numeric",
            NotRun,
            "no mesh-node binding to a skin; static geometry remains eligible",
        );
    }
    r
}
fn bin<'a>(b: gltf::Buffer<'_>, blob: Option<&'a [u8]>) -> Option<&'a [u8]> {
    match b.source() {
        gltf::buffer::Source::Bin => blob.and_then(|v| v.get(..b.length())),
        _ => None,
    }
}
/// Validate arithmetic before invoking reader (which uses unchecked offset math).
fn ready(
    a: &gltf::Accessor<'_>,
    blob: Option<&[u8]>,
    vertex_attribute: bool,
    loc: &str,
    r: &mut RigReport,
) -> bool {
    use CheckStatus::{Fail, NotRun};
    let loc = format!("{loc} accessors[{}]", a.index());
    if a.sparse().is_some() || a.view().is_none() {
        r.add(
            "rig.accessor",
            NotRun,
            format!("{loc}: sparse or viewless accessor unsupported"),
        );
        return false;
    }
    let v = a.view().expect("checked view");
    let b = v.buffer();
    if !matches!(b.source(), gltf::buffer::Source::Bin) || blob.is_none() {
        r.add(
            "rig.accessor",
            NotRun,
            format!(
                "{loc}: buffers[{}] external URI or missing BIN; no resource fetched",
                b.index()
            ),
        );
        return false;
    }
    let size = a.size();
    let stride = v.stride().unwrap_or(size);
    let end = a
        .count()
        .checked_sub(1)
        .and_then(|n| n.checked_mul(stride))
        .and_then(|n| n.checked_add(a.offset()))
        .and_then(|n| n.checked_add(size));
    let vend = v.offset().checked_add(v.length());
    // glTF 2.0.1 §3.6.2.4: relative AND absolute component alignment.
    // Vertex attributes additionally align relative offsets/declared strides to 4;
    // IBM is not a vertex attribute and only uses its FLOAT component rules.
    let absolute = v.offset().checked_add(a.offset());
    if stride < size
        || !stride.is_multiple_of(a.data_type().size())
        || !a.offset().is_multiple_of(a.data_type().size())
        || absolute.is_none_or(|n| !n.is_multiple_of(a.data_type().size()))
        || (vertex_attribute
            && (!a.offset().is_multiple_of(4) || v.stride().is_some_and(|n| !n.is_multiple_of(4))))
        || end.is_none_or(|n| n > v.length())
        || vend.is_none_or(|n| n > b.length())
        || bin(b, blob).is_none()
    {
        r.add(
            "rig.accessor",
            Fail,
            format!("{loc}: invalid count/stride/alignment or range outside bufferView/BIN"),
        );
        return false;
    }
    true
}
