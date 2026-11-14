//! Responsibility: Rig SDK traversal and streaming numeric decoding.
//! Dependencies: gltf and pure Rig policies.
//! Callers: Rig inspection capability.
//! Invariants: no URI reads and no bulk numeric array clones.

use super::{domain::CheckStatus, rig_domain::*, rig_ports::RigNumeric};
use gltf::{
    accessor::{DataType, Dimensions},
    mesh::Semantic,
};
use std::collections::BTreeSet;
pub(crate) struct GltfRig;
impl RigNumeric for GltfRig {
    fn inspect(&self, bytes: &[u8], r: &mut RigReport) {
        use CheckStatus::{Fail, NotRun, Pass};
        let g = match gltf::Gltf::from_slice(bytes) {
            Ok(g) => g,
            Err(e) => {
                r.add("rig.numeric", NotRun, e.to_string());
                return;
            }
        };
        let children: Vec<Vec<usize>> = g
            .nodes()
            .map(|n| n.children().map(|c| c.index()).collect())
            .collect();
        forest(&children, r);
        let extensions: Vec<_> = g.extensions_used().chain(g.extensions_required()).collect();
        let meshes: Vec<_> = g.meshes().collect();
        let skins: Vec<_> = g.skins().collect();
        for node in g.nodes() {
            let Some(binding) = bound_skin(NodeBinding {
                node: node.index(),
                mesh: node.mesh().map(|m| m.index()),
                skin: node.skin().map(|s| s.index()),
            }) else {
                continue;
            };
            let mesh = &meshes[binding.mesh];
            let skin = &skins[binding.skin];
            r.bound_nodes.push(binding.node);
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
            if !valid_joints(&joints) {
                r.add(
                    "rig.joints",
                    Fail,
                    format!("skins[{}].joints: empty or duplicate nodes", skin.index()),
                );
            }
            let ibloc = format!("skins[{}].inverseBindMatrices", skin.index());
            if let Some(a) = skin.inverse_bind_matrices() {
                if !valid_ibm(metadata(&a), joints.len()) {
                    r.add(
                        "rig.bind_matrices",
                        Fail,
                        format!("{ibloc}: requires FLOAT MAT4, count >= {}", joints.len()),
                    );
                } else if ready(&a, g.blob.as_deref(), false, &ibloc, r) {
                    let reader = skin.reader(|b| bin(b, g.blob.as_deref()));
                    if let Some(values) = reader.read_inverse_bind_matrices() {
                        let mut ok = true;
                        for (i, m) in values.enumerate() {
                            if !affine(&m) {
                                ok = false;
                                r.add(
                                    "rig.bind_matrices",
                                    Fail,
                                    format!(
                                        "{ibloc}[{i}]: nonfinite value or fourth row != [0,0,0,1]"
                                    ),
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
                if !paired(&js, &ws) {
                    r.add(
                        "rig.attributes",
                        Fail,
                        format!(
                            "{ploc}: JOINTS_n {js:?} / WEIGHTS_n {ws:?} must pair starting at 0"
                        ),
                    );
                    continue;
                }
                if js.len() != 1 {
                    r.add(
                        "rig.weights",
                        NotRun,
                        format!(
                            "{ploc}: multiple influence sets unsupported; no partial sum checked"
                        ),
                    );
                    continue;
                }
                let j = p.get(&Semantic::Joints(0)).expect("checked set");
                let w = p.get(&Semantic::Weights(0)).expect("checked set");
                let count = p.get(&Semantic::Positions).map(|a| a.count());
                if !valid_attributes(metadata(&j), metadata(&w), count) {
                    r.add(
                    "rig.attributes",
                    Fail,
                    format!(
                        "{ploc}: JOINTS_0/WEIGHTS_0 invalid type, normalization or POSITION count"
                    ),
                );
                    continue;
                }
                let jr = ready(&j, g.blob.as_deref(), true, &format!("{ploc}.JOINTS_0"), r);
                let wr = ready(&w, g.blob.as_deref(), true, &format!("{ploc}.WEIGHTS_0"), r);
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
                    if !vertex(indices, weights, joints.len(), target, v, &ploc, r) {
                        ok = false;
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
    }
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
    if !layout(
        a.count(),
        a.size(),
        a.offset(),
        a.data_type().size(),
        v.stride(),
        v.offset(),
        v.length(),
        b.length(),
        bin(b, blob).is_some(),
        vertex_attribute,
    ) {
        r.add(
            "rig.accessor",
            Fail,
            format!("{loc}: invalid count/stride/alignment or range outside bufferView/BIN"),
        );
        return false;
    }
    true
}

fn metadata(a: &gltf::Accessor<'_>) -> NumericAccessor {
    NumericAccessor {
        kind: match a.data_type() {
            DataType::F32 => NumericKind::Float,
            DataType::U8 => NumericKind::U8,
            DataType::U16 => NumericKind::U16,
            _ => NumericKind::Other,
        },
        shape: match a.dimensions() {
            Dimensions::Vec4 => NumericShape::Vec4,
            Dimensions::Mat4 => NumericShape::Mat4,
            _ => NumericShape::Other,
        },
        normalized: a.normalized(),
        count: a.count(),
    }
}
