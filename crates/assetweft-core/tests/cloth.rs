use assetweft_core::assurance::{ClothReport, GarmentBinding, ValidationLimits, checks::*};
use gltf::binary::{Glb, Header};
use serde_json::{Value, json};
use std::{borrow::Cow, fs};

fn rig() -> (Value, Vec<u8>) {
    let document = json!({
        "asset":{"version":"2.0"},
        "nodes":[{"name":"body","mesh":0},{"name":"tunic","mesh":1,"skin":0},
            {"name":"buckle","mesh":0},{"name":"hip","children":[4]},{"name":"spine"}],
        "scene":0,"scenes":[{"nodes":[0,1,2,3]}],
        "skins":[{"joints":[3,4]}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]},
            {"primitives":[{"attributes":{"POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2}}]}],
        "buffers":[{"byteLength":96}],
        "bufferViews":[{"buffer":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":12},
            {"buffer":0,"byteOffset":48,"byteLength":48}],
        "accessors":[
            {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
            {"bufferView":1,"componentType":5121,"count":3,"type":"VEC4"},
            {"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}
        ]
    });
    let positions = [0_f32, 0., 0., 1., 0., 0., 0., 1., 0.];
    let mut bin: Vec<_> = positions.into_iter().flat_map(f32::to_le_bytes).collect();
    bin.extend([0, 1, 0, 0].repeat(3));
    bin.extend(
        [0.75_f32, 0.25, 0., 0.]
            .repeat(3)
            .into_iter()
            .flat_map(f32::to_le_bytes),
    );
    (document, bin)
}

fn spec() -> Value {
    let garment = |node, binding| {
        json!({
            "node_name":node,"binding_kind":binding,"body_mesh_node":"body",
            "max_weight_sum_error":0.0001,"max_stretch_ratio":0.1,
            "max_penetration_m":0.05,"require_supported_structure":true
        })
    };
    let pose = |frame| {
        json!({
            "frame":frame,"body_vertices":[[0,0,0],[1,1,1]],
            "garment_vertices":[[0,0,1.5],[1,0,1.5]],
            "reference_edge_lengths_m":[1],"rest_garment_edges":[[0,1]]
        })
    };
    json!({"garments":[garment("tunic","skinned"),garment("buckle","rigid")],
        "sampled_motion_poses":[pose(0),pose(5)]})
}

fn glb(document: &Value, bin: &[u8]) -> Vec<u8> {
    Glb {
        header: Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: Cow::Owned(serde_json::to_vec(document).unwrap()),
        bin: Some(Cow::Borrowed(bin)),
    }
    .to_vec()
    .unwrap()
}

fn inspect(bytes: &[u8], spec: &str) -> Result<ClothReport, ClothInputError> {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("rig.glb");
    let contract = dir.path().join("spec.json");
    fs::write(&file, bytes).unwrap();
    fs::write(&contract, spec).unwrap();
    inspect_cloth_file(&file, &contract, ValidationLimits::default())
}

#[test]
fn clean_rig_is_classified_and_hash_is_stable_under_frame_reordering() {
    let (document, bin) = rig();
    let bytes = glb(&document, &bin);
    let mut spec = spec();
    let pass = inspect(&bytes, &spec.to_string()).unwrap();
    assert_eq!(pass.status, "PASS");
    assert_eq!(
        pass.garments[0].garment_classification,
        Some(GarmentBinding::Skinned)
    );
    assert_eq!(
        pass.garments[1].garment_classification,
        Some(GarmentBinding::Rigid)
    );
    assert_eq!(pass.max_weight_sum_error, 0.0);
    assert_eq!(pass.visual_engine_status, "NOT_RUN");
    assert_eq!(pass.semantic_hash.len(), 64);
    spec["sampled_motion_poses"]
        .as_array_mut()
        .unwrap()
        .reverse();
    spec["sampled_motion_poses"][0]["garment_vertices"][0][0] = json!(-0.0);
    let swapped = inspect(&bytes, &spec.to_string()).unwrap();
    assert_eq!(pass.semantic_hash, swapped.semantic_hash);
}

#[test]
fn mismatch_unsupported_layout_and_weight_errors_are_distinct() {
    let (document, bin) = rig();
    let mut spec = spec();
    spec["garments"][1]["binding_kind"] = json!("skinned");
    let mismatch = inspect(&glb(&document, &bin), &spec.to_string()).unwrap();
    assert_eq!(mismatch.status, "FAIL");
    assert_eq!(
        json!(mismatch.garments[1].violations),
        json!(["BindingMismatch"])
    );
    spec["garments"][1]["binding_kind"] = json!("rigid");
    let mut bad_weights = bin.clone();
    bad_weights[48..52].copy_from_slice(&0.5_f32.to_le_bytes());
    let report = inspect(&glb(&document, &bad_weights), &spec.to_string()).unwrap();
    assert_eq!(report.max_weight_sum_error, 0.25);
    assert_eq!(
        json!(report.garments[0].violations),
        json!(["InvalidSkinWeights"])
    );

    for (pointer, value, reason) in [
        (
            "/nodes/0/name",
            json!("other"),
            "MissingOrAmbiguousBodyMesh",
        ),
        ("/skins/0/joints", json!([]), "MissingOrDuplicateSkinJoints"),
        (
            "/meshes/1/primitives/0",
            json!({"attributes":{"POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2,"JOINTS_1":1,"WEIGHTS_1":2}}),
            "AdditionalJointWeightSets",
        ),
        (
            "/meshes/1/primitives/0",
            json!({"attributes":{"POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2},"targets":[{"POSITION":0}]}),
            "MorphDeformation",
        ),
    ] {
        let mut unsupported = document.clone();
        *unsupported.pointer_mut(pointer).unwrap() = value;
        spec["garments"][0]["require_supported_structure"] = json!(false);
        let report = inspect(&glb(&unsupported, &bin), &spec.to_string()).unwrap();
        assert_eq!(report.status, "FAIL");
        assert_eq!(report.garments[0].status, "UNSUPPORTED");
        assert!(report.unsupported_structures.contains(reason), "{report:?}");
    }
    let mut physics = document;
    physics["extensionsUsed"] = json!(["VENDOR_cloth"]);
    physics["extensionsRequired"] = json!(["VENDOR_cloth"]);
    let report = inspect(&glb(&physics, &bin), &spec.to_string()).unwrap();
    assert!(
        report
            .unsupported_structures
            .contains("UnsupportedExtension:VENDOR_cloth")
    );
    assert_eq!(report.status, "FAIL");
}

#[test]
fn motion_breaches_are_attributed_to_named_garment_and_worst_frame() {
    let (document, bin) = rig();
    let bytes = glb(&document, &bin);
    let mut spec = spec();
    let poses = spec["sampled_motion_poses"].as_array_mut().unwrap();
    let mut other = poses.clone();
    for pose in &mut *poses {
        pose["node_name"] = json!("tunic");
    }
    for pose in &mut other {
        pose["node_name"] = json!("buckle");
    }
    poses[1]["garment_vertices"] = json!([[0.5, 0.5, 0.5], [2.5, 0.5, 0.5]]);
    poses.extend(other);
    let report = inspect(&bytes, &spec.to_string()).unwrap();
    assert_eq!(report.status, "FAIL");
    assert_eq!(report.max_penetration_m, 0.5);
    assert_eq!(report.max_stretch_ratio, 1.0);
    assert_eq!(report.garments[0].max_penetration_frame, 5);
    assert_eq!(report.garments[0].max_stretch_frame, 5);
    assert_eq!(
        json!(report.garments[0].violations),
        json!(["ClothPenetration", "ExcessiveDeformation"])
    );
    assert_eq!(report.garments[1].status, "PASS");
}

#[test]
fn malformed_input_is_rejected_before_issuing_a_receipt() {
    let (document, mut bin) = rig();
    let bytes = glb(&document, &bin);
    let valid = spec().to_string();
    assert_eq!(inspect(b"glTF", &valid).unwrap_err().field, "file");
    assert_eq!(
        inspect(&bytes, &valid.replace("0.05", "NaN"))
            .unwrap_err()
            .field,
        "spec"
    );
    bin[48..52].copy_from_slice(&f32::NAN.to_le_bytes());
    assert_eq!(
        inspect(&glb(&document, &bin), &valid).unwrap_err().field,
        "file"
    );
    let mut spec = spec();
    spec["sampled_motion_poses"][1]["frame"] = json!(0);
    assert_eq!(
        inspect(&bytes, &spec.to_string()).unwrap_err().field,
        "spec"
    );
    spec["sampled_motion_poses"][1]["frame"] = json!(5);
    spec["garments"][0]["unknown"] = json!(true);
    assert_eq!(
        inspect(&bytes, &spec.to_string()).unwrap_err().field,
        "spec"
    );
}

#[test]
fn normalized_integer_weights_can_pass_a_zero_error_limit() {
    let (mut document, mut bin) = rig();
    document["accessors"][2]["componentType"] = json!(5121);
    document["accessors"][2]["normalized"] = json!(true);
    bin[48..60].copy_from_slice(&[128, 127, 0, 0].repeat(3));
    let mut spec = spec();
    spec["garments"][0]["max_weight_sum_error"] = json!(0.0);
    let pass = inspect(&glb(&document, &bin), &spec.to_string()).unwrap();
    assert_eq!(pass.status, "PASS");
    assert_eq!(pass.max_weight_sum_error, 0.0);

    bin[37] = 2;
    let fail = inspect(&glb(&document, &bin), &spec.to_string()).unwrap();
    assert_eq!(fail.status, "FAIL");
    assert!(fail.unsupported_structures.contains("OutOfRangeSkinJoint"));
}
