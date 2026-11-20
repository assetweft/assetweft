//! M3 motion preset called by the CLI. Reuses the accepted Rigged Simple identity
//! and glTF container codec. Appends a clip to an immutable source and publishes
//! to a new path only after serialization succeeds.
use crate::assurance::{ValidationLimits, read_bounded, rig_blender::RIGGED_SIMPLE};
use gltf::binary::Glb;
use serde_json::{
    Value, json,
    value::{RawValue, to_raw_value},
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, error::Error, io::Write, path::Path};

const CLIP: &str = "AssetWeft_InPlaceStrike";

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}

/// Add one child-joint rotation track to the exact supported two-joint cylinder.
/// # Errors
/// Rejects other source bytes, read limits, and existing or unwritable destinations.
pub fn add_strike(
    input: &Path,
    output: &Path,
    limits: ValidationLimits,
) -> Result<Value, Box<dyn Error>> {
    let source = read_bounded(input, limits)?;
    if digest(&source) != RIGGED_SIMPLE {
        return Err("UNSUPPORTED Rigged Simple GLB SHA-256".into());
    }
    let mut glb = Glb::from_slice(&source)?;
    // Keep upstream numeric literals intact, including the original accessor bounds.
    let mut doc: BTreeMap<String, Box<RawValue>> = serde_json::from_slice(&glb.json)?;
    let mut views: Vec<Value> = serde_json::from_str(doc["bufferViews"].get())?;
    let mut accessors: Vec<Box<RawValue>> = serde_json::from_str(doc["accessors"].get())?;
    let mut clips: Vec<Box<RawValue>> = serde_json::from_str(doc["animations"].get())?;
    let bin = glb.bin.as_mut().ok_or("missing BIN")?.to_mut();
    let times = [0.0_f32, 0.25, 0.5, 1.0];
    // Local Y rotation at rest and at peak (rest composed with +90 degrees).
    // Quaternion signs share the same hemisphere for shortest-arc interpolation.
    let rest = [0.0_f32, -0.00028992252, 0.0, 0.99999994];
    let peak = [0.0, 0.7069017, 0.0, 0.70731175];
    let time_view = views.len();
    for values in [times.as_slice(), [rest, rest, peak, rest].as_flattened()] {
        let offset = bin.len();
        bin.extend(values.iter().flat_map(|v| v.to_le_bytes()));
        views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":bin.len()-offset}));
    }
    let time = accessors.len();
    accessors.push(to_raw_value(&json!({
        "bufferView":time_view, "componentType":5126, "count":4,
        "type":"SCALAR", "min":[0], "max":[1]
    }))?);
    accessors.push(to_raw_value(
        &json!({"bufferView":time_view+1,"componentType":5126,"count":4,"type":"VEC4"}),
    )?);
    clips.push(to_raw_value(&json!({
        "name":CLIP,
        "channels":[{"sampler":0,"target":{"node":4,"path":"rotation"}}],
        "samplers":[{"input":time,"output":time+1,"interpolation":"LINEAR"}]
    }))?);
    doc.insert("animations".into(), to_raw_value(&clips)?);
    doc.insert("accessors".into(), to_raw_value(&accessors)?);
    doc.insert("bufferViews".into(), to_raw_value(&views)?);
    let mut asset: Value = serde_json::from_str(doc["asset"].get())?;
    asset["copyright"] = json!(
        "Rigged Simple © 2017 Cesium, CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/). AssetWeft added the InPlaceStrike clip."
    );
    doc.insert("asset".into(), to_raw_value(&asset)?);
    doc.insert(
        "buffers".into(),
        to_raw_value(&json!([{"byteLength":bin.len()}]))?,
    );
    glb.json = serde_json::to_vec(&doc)?.into();
    let bytes = glb.to_vec()?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(&bytes)?;
    staged.persist_noclobber(output)?;
    Ok(json!({"clip":CLIP,"source_sha256":RIGGED_SIMPLE,"output_sha256":digest(&bytes)}))
}
