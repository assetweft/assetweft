//! Responsibility: GLB container and gltf SDK adapter translating structural facts.
//! Dependencies: gltf, sha2, std and Assurance StructureCheck.
//! Callers: Assurance composition.
//! Invariants: no URI resolution; bound mesh/skin/joint relationships retained.

use super::{
    domain::{CheckedBytes, ComponentFacts},
    ports::StructureCheck,
};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// 结构化校验 GLB 2.0 二进制容器：12 字节头 + 分块表（magic/version/声明长度/chunk 边界与类型）。
///
/// 只做容器层检查，不解析 JSON 内容、不替代成熟结构解析器；用于阻止把 plain JSON 当 GLB。
fn check_glb_container(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 12 {
        return Err(format!("too short for a GLB header: {} bytes", bytes.len()));
    }
    if &bytes[0..4] != b"glTF" {
        return Err(
            "magic is not glTF; plain JSON or other formats are outside the declared GLB scope"
                .to_owned(),
        );
    }
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if version != 2 {
        return Err(format!(
            "unsupported GLB container version {version}; only 2 is in scope"
        ));
    }
    let declared = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    if declared != bytes.len() {
        return Err(format!(
            "declared length {declared} != actual {} bytes",
            bytes.len()
        ));
    }
    let mut offset = 12usize;
    let mut chunk_index = 0usize;
    while offset < bytes.len() {
        if offset + 8 > bytes.len() {
            return Err("truncated chunk header".to_owned());
        }
        let len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let ty = &bytes[offset + 4..offset + 8];
        if chunk_index == 0 && ty != b"JSON" {
            return Err("first GLB chunk must be JSON".to_owned());
        }
        // glTF 2.0.1 §4.4.3: JSON exactly once at index 0, optional BIN
        // only at index 1. Unknown extension chunks remain allowed in the tail.
        if ty == b"JSON" && chunk_index != 0 {
            return Err("JSON chunk must occur exactly once, first".to_owned());
        }
        if ty == b"BIN\0" && chunk_index != 1 {
            return Err("BIN chunk may occur only once, second".to_owned());
        }
        chunk_index += 1;
        if !len.is_multiple_of(4) {
            return Err(format!("chunk length {len} is not 4-byte aligned"));
        }
        let start = offset + 8;
        if start + len > bytes.len() {
            return Err("chunk length exceeds the file".to_owned());
        }
        offset = start + len;
    }
    Ok(())
}

pub(crate) struct GltfChecks;

impl StructureCheck for GltfChecks {
    fn check(&self, bytes: &[u8]) -> CheckedBytes {
        let digest = Sha256::digest(bytes);
        // 只做十六进制展示（sha2 0.11 的摘要类型不再实现 LowerHex）；哈希算法仍由 sha2 提供。
        let file_sha256 = digest
            .iter()
            .fold(String::with_capacity(64), |mut acc, byte| {
                use std::fmt::Write as _;
                let _ = write!(acc, "{byte:02x}");
                acc
            });
        let structure = check_glb_container(bytes).and_then(|()| {
            let gltf = gltf::Gltf::from_slice(bytes)
                .map_err(|err| format!("gltf parser rejected the GLB structure: {err}"))?;
            let document = gltf.document;
            let mesh_count = document.meshes().count();
            let skin_count = document.skins().count();
            let animation_count = document.animations().count();
            // 角色关联：mesh node 必须实际引用 skin；动画必须命中该 rig 的 joint。
            let has_bound_skinned_mesh = document
                .nodes()
                .any(|n| n.mesh().is_some() && n.skin().is_some());
            // 只收集被 mesh node 实际引用的 skin 的 joint；未绑定 skin 的 joint 不参与动画判定。
            let bound_skins: HashSet<usize> = document
                .nodes()
                .filter(|n| n.mesh().is_some())
                .filter_map(|n| n.skin().map(|s| s.index()))
                .collect();
            let joints: HashSet<usize> = document
                .skins()
                .filter(|s| bound_skins.contains(&s.index()))
                .flat_map(|s| s.joints())
                .map(|n| n.index())
                .collect();
            let has_joint_animation = document.animations().any(|a| {
                a.channels()
                    .any(|c| joints.contains(&c.target().node().index()))
            });

            Ok(ComponentFacts {
                mesh_count,
                skin_count,
                animation_count,
                has_bound_skinned_mesh,
                has_joint_animation,
            })
        });
        CheckedBytes {
            file_sha256,
            bytes_read: bytes.len(),
            structure,
        }
    }
}
