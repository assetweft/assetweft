use super::digest;
use crate::{assurance::rig_blender::RIGGED_SIMPLE, library::ComponentRole};
use gltf::binary::Glb;
use serde_json::{Value, value::RawValue};
use std::collections::BTreeMap;

pub(crate) const IDLE_STRIKE: &str =
    "667d8bfccdeb68d64c9668ff54ab0af907aa240988a3d488f49ff7b43152b09d";

#[derive(serde::Serialize)]
struct Part<'a> {
    profile: &'static str,
    sections: BTreeMap<&'a str, &'a RawValue>,
    accessors: BTreeMap<usize, &'a RawValue>,
    views: BTreeMap<usize, (&'a Value, &'a [u8])>,
}

/// Project the indexed scene, accessors, buffer views and their complete BIN
/// slices into one JSON object per role. Original numeric literals in scene
/// and accessor records survive, including upstream high-precision bounds.
/// # Errors
/// Only the original Cesium sample and the verified idle + strike derivative are supported.
pub fn rigged_simple_components(
    bytes: &[u8],
) -> Result<Vec<(ComponentRole, Vec<u8>)>, &'static str> {
    if !matches!(digest(bytes).as_str(), RIGGED_SIMPLE | IDLE_STRIKE) {
        return Err("unverified Rigged Simple component profile");
    }
    let glb = Glb::from_slice(bytes).expect("pinned container");
    let bin = glb.bin.as_deref().expect("pinned BIN");
    let doc: BTreeMap<&str, &RawValue> = serde_json::from_slice(&glb.json).expect("pinned JSON");
    let accessors: Vec<&RawValue> =
        serde_json::from_str(doc["accessors"].get()).expect("accessors");
    let views: Vec<Value> = serde_json::from_str(doc["bufferViews"].get()).expect("views");
    let mut motion = vec![5, 6, 7, 8];
    motion.extend(10..accessors.len());
    let parts = [
        (ComponentRole::Mesh, &["meshes"][..], (0..5).collect()),
        (ComponentRole::Materials, &["materials"][..], vec![]),
        (
            ComponentRole::Rig,
            &["scene", "scenes", "nodes", "skins"][..],
            vec![9],
        ),
        (ComponentRole::Motion, &["animations"][..], motion),
    ];
    let mut result = Vec::new();
    for (role, keys, indices) in parts {
        let sections: BTreeMap<_, _> = keys.iter().map(|k| (*k, doc[*k])).collect();
        let mut indexed_accessors = BTreeMap::new();
        let mut indexed_views = BTreeMap::new();
        for i in indices {
            let a = accessors[i];
            let accessor: Value = serde_json::from_str(a.get()).expect("pinned accessor");
            let view_index = accessor["bufferView"].as_u64().expect("bufferView") as usize;
            let view = &views[view_index];
            let offset = view["byteOffset"].as_u64().expect("byteOffset") as usize;
            let length = view["byteLength"].as_u64().expect("byteLength") as usize;
            indexed_accessors.insert(i, a);
            indexed_views.insert(view_index, (view, &bin[offset..offset + length]));
        }
        let part = Part {
            profile: "CESIUM_RIGGED_SIMPLE_V1",
            sections,
            accessors: indexed_accessors,
            views: indexed_views,
        };
        result.push((role, serde_json::to_vec(&part).expect("in-memory JSON")));
    }
    Ok(result)
}
