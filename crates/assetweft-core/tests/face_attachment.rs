use assetweft_core::assurance::{FaceAttachmentReport, ValidationLimits, checks::*};
use serde_json::{Value, json};
use std::fs;

fn glb(document: &Value, bin: &[f32]) -> Vec<u8> {
    let mut json = serde_json::to_vec(document).unwrap();
    json.resize(json.len().next_multiple_of(4), b' ');
    let bin_length = bin.len() as u32 * 4;
    let mut bytes = b"glTF".to_vec();
    bytes.extend(2_u32.to_le_bytes());
    bytes.extend(
        (20 + json.len() as u32 + if bin.is_empty() { 0 } else { 8 + bin_length }).to_le_bytes(),
    );
    bytes.extend((json.len() as u32).to_le_bytes());
    bytes.extend(b"JSON");
    bytes.extend(json);
    if !bin.is_empty() {
        bytes.extend(bin_length.to_le_bytes());
        bytes.extend(b"BIN\0");
        bytes.extend(bin.iter().flat_map(|value| value.to_le_bytes()));
    }
    bytes
}

fn face() -> (Value, Vec<f32>) {
    let document = json!({
        "asset":{"version":"2.0"},
        "nodes":[{"name":"face","mesh":0},{"name":"eye"},{"name":"jaw"},
            {"name":"socket","children":[4]},{"name":"helmet","translation":[0.1004,0,0]}],
        "scenes":[{"nodes":[0,1,2,3]}],"scene":0,
        "meshes":[{"extras":{"targetNames":["blink","jaw_open","phantom"]},
            "primitives":[{"attributes":{"POSITION":0},"targets":[{"POSITION":1},{"POSITION":1}]}]}],
        "animations":[{"name":"facial_clip","channels":[
            {"sampler":0,"target":{"node":0,"path":"weights"}},
            {"sampler":1,"target":{"node":1,"path":"translation"}},
            {"sampler":1,"target":{"node":2,"path":"translation"}}],
            "samplers":[{"input":2,"output":3},{"input":2,"output":4}]}],
        "buffers":[{"byteLength":120}],
        "bufferViews":[{"buffer":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":36},
            {"buffer":0,"byteOffset":72,"byteLength":8},{"buffer":0,"byteOffset":80,"byteLength":16},
            {"buffer":0,"byteOffset":96,"byteLength":24}],
        "accessors":[
            {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
            {"bufferView":1,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[0,0,0.1]},
            {"bufferView":2,"componentType":5126,"count":2,"type":"SCALAR","min":[0],"max":[1]},
            {"bufferView":3,"componentType":5126,"count":4,"type":"SCALAR"},
            {"bufferView":4,"componentType":5126,"count":2,"type":"VEC3"}]
    });
    let bin = vec![
        0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0., 0.1, 0., 0., 0., 0., 0., 0., 0., 1., 0., 0.,
        1., 1., 0., 0., 0., 0., 0.1, 0.,
    ];
    (document, bin)
}

fn spec(mode: &str, part: &str) -> Value {
    json!({"face_mode":mode,"expected_expressions":[],
        "rigid_attachments":[{"socket_node":"socket","attachment_node":part,
        "expected_offset_m":[0.1,0,0],"max_drift_m":0.0005,"lock_mode":"SEMANTIC"}]})
}

fn inspect(bytes: &[u8], spec: &str) -> Result<FaceAttachmentReport, FaceAttachmentInputError> {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.glb");
    let contract = dir.path().join("spec.json");
    fs::write(&file, bytes).unwrap();
    fs::write(&contract, spec).unwrap();
    inspect_face_attachment_file(&file, &contract, ValidationLimits::default())
}

#[test]
fn required_face_uses_bound_morph_targets_and_finite_animation() {
    let (mut document, bin) = face();
    let mut spec = spec("required", "helmet");
    spec["expected_expressions"] = json!(["jaw_open", "blink", "facial_clip"]);
    spec["eye_jaw_nodes"] = json!({"left_eye":"eye","jaw":"jaw"});
    let report = inspect(&glb(&document, &bin), &spec.to_string()).unwrap();
    assert_eq!(report.facial_status, "PASS");
    assert_eq!(report.morph_target_count, 2);
    assert_eq!(
        report.matched_expressions,
        ["blink", "facial_clip", "jaw_open"]
    );
    assert!(report.eye_jaw_playable);
    assert_eq!(report.visual_engine_status, "NOT_RUN");
    assert_eq!(report.attachment_results[0].status, "PASS");
    assert_eq!(report.attachment_results[0].drift_m, Some(0.0004));
    assert_eq!(report.semantic_hash.len(), 64);
    document["nodes"][4]["translation"][0] = json!(0.1004001);
    assert_eq!(
        report.semantic_hash,
        inspect(&glb(&document, &bin), &spec.to_string())
            .unwrap()
            .semantic_hash
    );

    spec["expected_expressions"] = json!(["blink", "phantom"]);
    let missing = inspect(&glb(&document, &bin), &spec.to_string()).unwrap();
    assert_eq!(missing.missing_expressions, ["phantom"]);
    assert_eq!(missing.facial_status, "FAIL");
    spec["expected_expressions"] = json!(["blink"]);
    document["animations"][0]["channels"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    let static_morph = inspect(&glb(&document, &bin), &spec.to_string()).unwrap();
    assert!(static_morph.eye_jaw_playable);
    assert_eq!(static_morph.unplayable_expressions, ["blink"]);
    assert_eq!(static_morph.facial_status, "FAIL");
    document["animations"] = json!([]);
    let static_face = inspect(&glb(&document, &bin), &spec.to_string()).unwrap();
    assert!(!static_face.eye_jaw_playable);
    assert_eq!(static_face.facial_status, "FAIL");
}

#[test]
fn mechanical_character_skips_face_but_enforces_socket_drift() {
    let mut document = json!({"asset":{"version":"2.0"},
        "nodes":[{"name":"socket","children":[1],"scale":[2,1,1]},
            {"name":"shoulder","translation":[0.1002,0,0]}]});
    let mut spec = spec("not-applicable", "shoulder");
    spec["expected_expressions"] = json!(["ignored"]);
    let report = inspect(&glb(&document, &[]), &spec.to_string()).unwrap();
    assert_eq!(report.facial_status, "INAPPLICABLE");
    assert_eq!(report.morph_target_count, 0);
    assert_eq!(report.attachment_results[0].status, "PASS");
    document["nodes"][1]["translation"][0] = json!(0.1004);
    let drift = inspect(&glb(&document, &[]), &spec.to_string()).unwrap();
    assert_eq!(drift.facial_status, "INAPPLICABLE");
    assert_eq!(drift.attachment_results[0].status, "FAIL");
    assert_eq!(drift.attachment_results[0].world_drift_m, Some(0.0008));
}

#[test]
fn aliased_roles_and_skinned_attachment_fail() {
    let (mut document, bin) = face();
    let mut spec = spec("required", "helmet");
    spec["eye_jaw_nodes"] = json!({"left_eye":"eye","right_eye":"eye"});
    let alias = inspect(&glb(&document, &bin), &spec.to_string()).unwrap();
    assert_eq!(alias.facial_status, "FAIL");
    document["skins"] = json!([{"joints":[2]}]);
    document["nodes"][4]["skin"] = json!(0);
    document["nodes"][4]["mesh"] = json!(1);
    document["meshes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"primitives":[{"attributes":{"POSITION":0}}]}));
    spec["face_mode"] = json!("not-applicable");
    let skinned = inspect(&glb(&document, &bin), &spec.to_string()).unwrap();
    assert_eq!(skinned.attachment_results[0].status, "FAIL");
}

#[test]
fn malformed_inputs_are_rejected() {
    let (mut document, mut bin) = face();
    let spec = spec("required", "helmet").to_string();
    assert_eq!(inspect(b"glTF", &spec).err().unwrap().field, "file");
    for invalid in [
        spec.replace("0.0005", "NaN"),
        spec.replace("0.0005", "-0.0000001"),
        spec.replace("\"lock_mode\"", "\"unknown\":true,\"lock_mode\""),
    ] {
        assert_eq!(
            inspect(&glb(&document, &bin), &invalid)
                .err()
                .unwrap()
                .field,
            "spec"
        );
    }
    bin[20] = f32::NAN;
    assert_eq!(
        inspect(&glb(&document, &bin), &spec).err().unwrap().field,
        "file"
    );
    bin[20] = 0.;
    document["bufferViews"][3]["byteLength"] = json!(12);
    assert_eq!(
        inspect(&glb(&document, &bin), &spec).err().unwrap().field,
        "file"
    );
}

#[test]
fn sparse_facial_keys_are_checked_and_playable() {
    let (mut document, mut bin) = face();
    bin.push(f32::from_le_bytes([0, 1, 2, 3]));
    document["buffers"][0]["byteLength"] = json!(124);
    document["bufferViews"]
        .as_array_mut()
        .unwrap()
        .push(json!({"buffer":0,"byteOffset":120,"byteLength":4}));
    document["accessors"][3] = json!({
        "componentType":5126,"count":4,"type":"SCALAR",
        "sparse":{"count":4,"indices":{"bufferView":5,"componentType":5121},
            "values":{"bufferView":3}}
    });
    let mut spec = spec("required", "helmet");
    spec["expected_expressions"] = json!(["blink"]);
    assert_eq!(
        inspect(&glb(&document, &bin), &spec.to_string())
            .unwrap()
            .facial_status,
        "PASS"
    );
    *bin.last_mut().unwrap() = f32::from_le_bytes([0, 1, 1, 3]);
    assert_eq!(
        inspect(&glb(&document, &bin), &spec.to_string())
            .err()
            .unwrap()
            .field,
        "file"
    );
}
