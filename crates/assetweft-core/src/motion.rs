use crate::assurance::{
    ValidationLimits, gltf_checks::parse_glb, read_bounded, rig_blender::RIGGED_SIMPLE,
};
use gltf::binary::Glb;
use serde_json::{
    Value, json,
    value::{RawValue, to_raw_value},
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, error::Error, io::Write, path::Path};

pub mod components;
pub mod stance;

const IDLE: &str = "AssetWeft_Idle";
const CLIP: &str = "AssetWeft_InPlaceStrike";

pub(crate) fn digest(bytes: &[u8]) -> String {
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
    write_motion(input, output, limits, false)
}

/// Add a repeatable distal sway alongside the strike, with a fixed root.
/// # Errors
/// Rejects incompatible joint topology, unverified sources and output conflicts.
pub fn add_strike_with_idle(
    input: &Path,
    output: &Path,
    limits: ValidationLimits,
) -> Result<Value, Box<dyn Error>> {
    write_motion(input, output, limits, true)
}

fn check_sway_topology(bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    let (gltf, _) = parse_glb(bytes).map_err(|reason| format!("UNSUPPORTED sway: {reason}"))?;
    let mut skins = gltf.skins();
    let skin = skins.next().ok_or("UNSUPPORTED sway: missing skin")?;
    if skins.next().is_some()
        || !gltf
            .nodes()
            .any(|n| n.mesh().is_some() && n.skin().is_some())
    {
        return Err("UNSUPPORTED sway: expected a mesh bound to a single skin".into());
    }
    let mut joints = skin.joints();
    if joints.len() == 2 {
        let a = joints.next().expect("two joints");
        let b = joints.next().expect("two joints");
        if a.index() != b.index()
            && (a.children().any(|c| c.index() == b.index())
                || b.children().any(|c| c.index() == a.index()))
        {
            return Ok(());
        }
    }
    Err("UNSUPPORTED fixed-root distal sway: requires two joints with a direct parent".into())
}

fn append_clip(
    bin: &mut Vec<u8>,
    views: &mut Vec<Value>,
    accessors: &mut Vec<Box<RawValue>>,
    clips: &mut Vec<Box<RawValue>>,
    name: &str,
    times: &[f32; 4],
    rotations: &[[f32; 4]; 4],
) -> serde_json::Result<()> {
    let time_view = views.len();
    for values in [times.as_slice(), rotations.as_flattened()] {
        let offset = bin.len();
        bin.extend(values.iter().flat_map(|v| v.to_le_bytes()));
        views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":bin.len()-offset}));
    }
    let time = accessors.len();
    accessors.push(to_raw_value(&json!({
        "bufferView":time_view, "componentType":5126, "count":times.len(),
        "type":"SCALAR", "min":[0], "max":[1]
    }))?);
    accessors.push(to_raw_value(
        &json!({"bufferView":time_view+1,"componentType":5126,"count":times.len(),"type":"VEC4"}),
    )?);
    clips.push(to_raw_value(&json!({
        "name":name,
        "channels":[{"sampler":0,"target":{"node":4,"path":"rotation"}}],
        "samplers":[{"input":time,"output":time+1,"interpolation":"LINEAR"}]
    }))?);
    Ok(())
}

fn write_motion(
    input: &Path,
    output: &Path,
    limits: ValidationLimits,
    with_idle: bool,
) -> Result<Value, Box<dyn Error>> {
    let source = read_bounded(input, limits)?;
    if digest(&source) != RIGGED_SIMPLE {
        let reason = if with_idle {
            check_sway_topology(&source)?;
            "UNSUPPORTED source: fixed-root distal sway topology matched, unverified Rigged Simple GLB SHA-256"
        } else {
            "UNSUPPORTED Rigged Simple GLB SHA-256"
        };
        return Err(reason.into());
    }
    let mut glb = Glb::from_slice(&source)?;
    let mut doc: BTreeMap<String, Box<RawValue>> = serde_json::from_slice(&glb.json)?;
    let mut views: Vec<Value> = serde_json::from_str(doc["bufferViews"].get())?;
    let mut accessors: Vec<Box<RawValue>> = serde_json::from_str(doc["accessors"].get())?;
    let mut clips: Vec<Box<RawValue>> = serde_json::from_str(doc["animations"].get())?;
    let bin = glb.bin.as_mut().ok_or("missing BIN")?.to_mut();
    let times = [0.0_f32, 0.25, 0.5, 1.0];
    // glTF 2.0.1 Appendix C: keep quaternion signs in the same hemisphere for shortest-path interpolation.
    let rest = [0.0_f32, -0.00028992252, 0.0, 0.99999994];
    let peak = [0.0, 0.7069017, 0.0, 0.70731175];
    append_clip(
        bin,
        &mut views,
        &mut accessors,
        &mut clips,
        CLIP,
        &times,
        &[rest, rest, peak, rest],
    )?;
    if with_idle {
        let sway = |degrees: f32| {
            let half = degrees.to_radians() / 2.0 + rest[1].atan2(rest[3]);
            [0.0, half.sin(), 0.0, half.cos()]
        };
        append_clip(
            bin,
            &mut views,
            &mut accessors,
            &mut clips,
            IDLE,
            &[0.0, 0.25, 0.75, 1.0],
            &[rest, sway(8.0), sway(-8.0), rest],
        )?;
    }
    doc.insert("animations".into(), to_raw_value(&clips)?);
    doc.insert("accessors".into(), to_raw_value(&accessors)?);
    doc.insert("bufferViews".into(), to_raw_value(&views)?);
    let mut asset: Value = serde_json::from_str(doc["asset"].get())?;
    let changes = if with_idle {
        "the InPlaceStrike and Idle clips"
    } else {
        "the InPlaceStrike clip"
    };
    asset["copyright"] = json!(format!(
        "Rigged Simple © 2017 Cesium, CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/). AssetWeft added {changes}."
    ));
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
    let mut report =
        json!({"clip":CLIP,"source_sha256":RIGGED_SIMPLE,"output_sha256":digest(&bytes)});
    if with_idle {
        report["idle_clip"] = json!(IDLE);
        report["constraint"] = json!("fixed-root distal sway");
    }
    Ok(report)
}
