//! Responsibility: Rig report and pure graph/numeric policy.
//! Dependencies: Assurance validation values and collections.
//! Callers: Rig application and streaming SDK adapter.
//! Invariants: checked layout math and ordered original diagnostics.

use super::domain::{CheckItem, CheckStatus, ValidationReport};
use std::collections::{BTreeSet, VecDeque};
#[derive(Debug)]
pub struct RigReport {
    /// Original report, including its unchanged numeric NOT_RUN entries.
    pub validation: ValidationReport,
    pub checks: Vec<CheckItem>,
    pub bound_nodes: Vec<usize>,
    pub not_run: &'static [&'static str],
}
impl RigReport {
    pub(crate) fn add(&mut self, id: &'static str, status: CheckStatus, detail: impl Into<String>) {
        self.checks.push(CheckItem {
            id,
            status,
            detail: detail.into(),
        });
    }
}

pub(crate) fn forest(children: &[Vec<usize>], r: &mut RigReport) {
    use CheckStatus::{Fail, Pass};
    // Iterative traversal avoids stack overflow on deeply nested hostile trees.
    let mut indegree = vec![0usize; children.len()];
    let mut graph_ok = true;
    for (n, cs) in children.iter().enumerate() {
        for &c in cs {
            indegree[c] += 1;
            if indegree[c] > 1 {
                graph_ok = false;
                r.add(
                    "rig.hierarchy",
                    Fail,
                    format!(
                        "nodes[{}].children: nodes[{}] has multiple incoming edges",
                        n, c
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
    while let Some(i) = queue.pop_front() {
        for &c in &children[i] {
            indegree[c] -= 1;
            if indegree[c] == 0 {
                queue.push_back(c);
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
}
pub(crate) fn affine(m: &[[f32; 4]; 4]) -> bool {
    m.iter().flatten().all(|x| x.is_finite())
        && [m[0][3], m[1][3], m[2][3], m[3][3]] == [0., 0., 0., 1.]
}
pub(crate) fn vertex(
    indices: [u16; 4],
    weights: [f64; 4],
    joint_count: usize,
    target: f64,
    v: usize,
    ploc: &str,
    r: &mut RigReport,
) -> bool {
    use CheckStatus::Fail;
    let mut seen = BTreeSet::new();
    for k in 0..4 {
        if usize::from(indices[k]) >= joint_count
            || !weights[k].is_finite()
            || weights[k] < 0.
            || (weights[k] > 0. && !seen.insert(indices[k]))
        {
            r.add("rig.weights",Fail,format!("{ploc}.JOINTS_0/WEIGHTS_0 vertex[{v}][{k}]: joint={}, weight={}; out of range, nonfinite, negative or duplicate influence",indices[k],weights[k]));
            return false;
        }
    }
    let sum: f64 = weights.iter().sum();
    let tolerance = if target == 1. {
        2e-7 * weights.iter().filter(|&&x| x > 0.).count() as f64
    } else {
        0.
    };
    if (sum - target).abs() > tolerance {
        r.add("rig.weights",Fail,format!("{ploc}.WEIGHTS_0 vertex[{v}]: sum={sum}, target={target}, tolerance={tolerance} (float validator policy / exact quantized sum)"));
        return false;
    }
    true
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn layout(
    count: usize,
    size: usize,
    offset: usize,
    component_size: usize,
    stride_declared: Option<usize>,
    view_offset: usize,
    view_length: usize,
    buffer_length: usize,
    bin_available: bool,
    vertex_attribute: bool,
) -> bool {
    let stride = stride_declared.unwrap_or(size);
    let end = count
        .checked_sub(1)
        .and_then(|n| n.checked_mul(stride))
        .and_then(|n| n.checked_add(offset))
        .and_then(|n| n.checked_add(size));
    let vend = view_offset.checked_add(view_length);
    // glTF 2.0.1 §3.6.2.4: relative AND absolute component alignment.
    // Vertex attributes additionally align relative offsets/declared strides to 4;
    // §3.6.1.1 forbids byteStride on non-vertex data (including IBM).
    // IBM still uses FLOAT alignment; no extra MAT4 16-byte rule.
    let absolute = view_offset.checked_add(offset);
    !((!vertex_attribute && stride_declared.is_some())
        || stride < size
        || !stride.is_multiple_of(component_size)
        || !offset.is_multiple_of(component_size)
        || absolute.is_none_or(|n| !n.is_multiple_of(component_size))
        || (vertex_attribute
            && (!offset.is_multiple_of(4)
                || stride_declared.is_some_and(|n| !n.is_multiple_of(4))))
        || end.is_none_or(|n| n > view_length)
        || vend.is_none_or(|n| n > buffer_length)
        || !bin_available)
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum NumericKind {
    Float,
    U8,
    U16,
    Other,
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum NumericShape {
    Vec4,
    Mat4,
    Other,
}
pub(crate) struct NumericAccessor {
    pub kind: NumericKind,
    pub shape: NumericShape,
    pub normalized: bool,
    pub count: usize,
}
pub(crate) fn valid_joints(joints: &[usize]) -> bool {
    !joints.is_empty() && joints.iter().collect::<BTreeSet<_>>().len() == joints.len()
}
pub(crate) fn paired(js: &BTreeSet<u32>, ws: &BTreeSet<u32>) -> bool {
    !js.is_empty() && js == ws && js.contains(&0)
}
pub(crate) fn valid_ibm(a: NumericAccessor, joints: usize) -> bool {
    a.kind == NumericKind::Float
        && a.shape == NumericShape::Mat4
        && !a.normalized
        && a.count >= joints
}
pub(crate) fn valid_attributes(
    j: NumericAccessor,
    w: NumericAccessor,
    positions: Option<usize>,
) -> bool {
    j.shape == NumericShape::Vec4
        && matches!(j.kind, NumericKind::U8 | NumericKind::U16)
        && !j.normalized
        && w.shape == NumericShape::Vec4
        && matches!(
            (w.kind, w.normalized),
            (NumericKind::Float, false) | (NumericKind::U8 | NumericKind::U16, true)
        )
        && j.count == w.count
        && positions == Some(j.count)
}

/// Actual node bindings; unbound skin objects cannot become numeric consumers.
pub(crate) struct NodeBinding {
    pub node: usize,
    pub mesh: Option<usize>,
    pub skin: Option<usize>,
}
pub(crate) struct BoundSkin {
    pub node: usize,
    pub mesh: usize,
    pub skin: usize,
}
pub(crate) fn bound_skin(node: NodeBinding) -> Option<BoundSkin> {
    Some(BoundSkin {
        node: node.node,
        mesh: node.mesh?,
        skin: node.skin?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_actual_mesh_node_skin_binding_is_selected() {
        assert!(
            bound_skin(NodeBinding {
                node: 7,
                mesh: None,
                skin: Some(2)
            })
            .is_none()
        );
        assert!(
            bound_skin(NodeBinding {
                node: 7,
                mesh: Some(4),
                skin: None
            })
            .is_none()
        );
        let b = bound_skin(NodeBinding {
            node: 7,
            mesh: Some(4),
            skin: Some(2),
        })
        .unwrap();
        assert_eq!((b.node, b.mesh, b.skin), (7, 4, 2));
    }
}
