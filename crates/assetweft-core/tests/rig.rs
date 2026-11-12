//! Responsibility: contract/defect tests for the partial bound-skin inspector.
//! Dependencies: public rig and validation APIs, serde_json for TEST_FIXTURE JSON.
//! Callers: cargo test and C1. Invariant: synthetic bytes are neither F0 nor AI assets.
use assetweft_core::{
    rig::{RigReport, inspect_bytes, inspect_file},
    validation::{CheckStatus, ValidationLimits},
};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<u8>) {
    let j = json!({"asset":{"version":"2.0","generator":"TEST_FIXTURE non-F0 non-AI"},
        "buffers":[{"byteLength":96}],
        "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":12},{"buffer":0,"byteOffset":12,"byteLength":4},{"buffer":0,"byteOffset":16,"byteLength":16},{"buffer":0,"byteOffset":32,"byteLength":64}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":1,"type":"VEC3","min":[0,0,0],"max":[0,0,0]},
            {"bufferView":1,"componentType":5121,"count":1,"type":"VEC4"},
            {"bufferView":2,"componentType":5126,"count":1,"type":"VEC4"},
            {"bufferView":3,"componentType":5126,"count":1,"type":"MAT4"}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2},"mode":0}]}],
        "nodes":[{"mesh":0,"skin":0},{}],"skins":[{"joints":[1],"inverseBindMatrices":3}],"scenes":[{"nodes":[0,1]}],"scene":0});
    let mut b = vec![0; 96];
    put(&mut b, 16, 1.);
    for i in 0..4 {
        put(&mut b, 32 + i * 20, 1.);
    }
    (j, b)
}
fn put(b: &mut [u8], offset: usize, value: f32) {
    b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn glb(j: &Value, b: &[u8]) -> Vec<u8> {
    let mut j = serde_json::to_vec(j).unwrap();
    while !j.len().is_multiple_of(4) {
        j.push(b' ');
    }
    let mut b = b.to_vec();
    while !b.len().is_multiple_of(4) {
        b.push(0);
    }
    let mut out = b"glTF".to_vec();
    out.extend(2u32.to_le_bytes());
    out.extend(((28 + j.len() + b.len()) as u32).to_le_bytes());
    out.extend((j.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(j);
    out.extend((b.len() as u32).to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(b);
    out
}
fn has(r: &RigReport, id: &str, status: CheckStatus, text: &str) -> bool {
    r.checks
        .iter()
        .any(|c| c.id == id && c.status == status && c.detail.contains(text))
}
#[test]
fn bound_known_values_and_original_report_are_separate() {
    let (j, b) = fixture();
    let r = inspect_bytes(&glb(&j, &b));
    assert_eq!(r.validation.format_status, CheckStatus::Pass);
    assert_eq!(r.bound_nodes, vec![0]);
    assert!(has(
        &r,
        "rig.weights",
        CheckStatus::Pass,
        "decoded 1 vertices"
    ));
    assert!(has(
        &r,
        "rig.bind_matrices",
        CheckStatus::Pass,
        "decoded finite"
    ));
    assert!(r.validation.not_run.contains(&"skin.weights_numeric"));
    assert!(r.not_run.contains(&"pose_mesh_deformation"));
}
#[test]
fn invalid_weight_values_locate_vertex_and_component() {
    for x in [-1., f32::NAN, f32::INFINITY] {
        let (j, mut b) = fixture();
        put(&mut b, 16, x);
        let r = inspect_bytes(&glb(&j, &b));
        assert!(
            has(&r, "rig.weights", CheckStatus::Fail, "vertex[0][0]"),
            "{r:?}"
        );
        assert_eq!(r.validation.format_status, CheckStatus::Pass);
    }
}
#[test]
fn joint_range_is_skin_local_even_for_zero_weight() {
    let (j, mut b) = fixture();
    b[15] = 1;
    let r = inspect_bytes(&glb(&j, &b));
    assert!(has(&r, "rig.weights", CheckStatus::Fail, "vertex[0][3]"));
}
#[test]
fn float_sum_policy_and_duplicate_nonzero_influence() {
    let (j, mut b) = fixture();
    put(&mut b, 16, 0.5);
    assert!(has(
        &inspect_bytes(&glb(&j, &b)),
        "rig.weights",
        CheckStatus::Fail,
        "sum=0.5"
    ));
    put(&mut b, 20, 0.5);
    assert!(has(
        &inspect_bytes(&glb(&j, &b)),
        "rig.weights",
        CheckStatus::Fail,
        "duplicate influence"
    ));
}
#[test]
fn quantized_weights_require_exact_raw_sum() {
    for (kind, target) in [(5121, 255u16), (5123, 65535)] {
        let (mut j, mut b) = fixture();
        j["accessors"][2]["componentType"] = json!(kind);
        j["accessors"][2]["normalized"] = json!(true);
        b[16..32].fill(0);
        b[16..18].copy_from_slice(&target.to_le_bytes());
        if kind == 5121 {
            b[17] = 0;
        }
        assert!(has(
            &inspect_bytes(&glb(&j, &b)),
            "rig.weights",
            CheckStatus::Pass,
            "decoded"
        ));
        b[16] -= 1;
        assert!(has(
            &inspect_bytes(&glb(&j, &b)),
            "rig.weights",
            CheckStatus::Fail,
            "tolerance=0"
        ));
    }
}
#[test]
fn matrices_shape_count_finite_affine_and_identity_default() {
    let (mut j, b) = fixture();
    j["skins"][0]
        .as_object_mut()
        .unwrap()
        .remove("inverseBindMatrices");
    assert!(has(
        &inspect_bytes(&glb(&j, &b)),
        "rig.bind_matrices",
        CheckStatus::Pass,
        "identity default"
    ));
    for (field, value) in [("type", json!("VEC4")), ("count", json!(0))] {
        let (mut j, b) = fixture();
        j["accessors"][3][field] = value;
        let r = inspect_bytes(&glb(&j, &b));
        assert!(
            r.validation.format_status == CheckStatus::Fail
                || has(&r, "rig.bind_matrices", CheckStatus::Fail, "FLOAT MAT4")
        );
    }
    for x in [f32::NAN, 0.5] {
        let (j, mut b) = fixture();
        put(&mut b, 44, x);
        assert!(has(
            &inspect_bytes(&glb(&j, &b)),
            "rig.bind_matrices",
            CheckStatus::Fail,
            "[0]"
        ));
    }
    let (mut j, b) = fixture();
    j["nodes"].as_array_mut().unwrap().push(json!({}));
    j["skins"][0]["joints"] = json!([1, 2]);
    assert!(has(
        &inspect_bytes(&glb(&j, &b)),
        "rig.bind_matrices",
        CheckStatus::Fail,
        "count >= 2"
    ));
}
#[test]
fn hierarchy_cycle_multiparent_and_invalid_reference() {
    let (mut j, b) = fixture();
    j["nodes"][1]["children"] = json!([1]);
    assert!(has(
        &inspect_bytes(&glb(&j, &b)),
        "rig.hierarchy",
        CheckStatus::Fail,
        "nodes[1]"
    ));
    j["nodes"][0]["children"] = json!([1]);
    j["nodes"][1].as_object_mut().unwrap().remove("children");
    j["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"children":[1]}));
    assert!(has(
        &inspect_bytes(&glb(&j, &b)),
        "rig.hierarchy",
        CheckStatus::Fail,
        "multiple incoming"
    ));
    j["nodes"][0]["children"] = json!([999]);
    let r = inspect_bytes(&glb(&j, &b));
    assert_eq!(r.validation.format_status, CheckStatus::Fail);
    assert!(
        r.validation
            .checks
            .iter()
            .any(|c| c.detail.contains("children")),
        "{r:?}"
    );
}
#[test]
fn static_and_orphan_skins_do_not_claim_numeric_pass() {
    let (mut j, b) = fixture();
    j["nodes"][0].as_object_mut().unwrap().remove("skin");
    for remove in [false, true] {
        if remove {
            j.as_object_mut().unwrap().remove("skins");
        }
        let r = inspect_bytes(&glb(&j, &b));
        assert!(r.bound_nodes.is_empty());
        assert!(has(&r, "rig.numeric", CheckStatus::NotRun, "no mesh-node"));
        assert!(
            !r.checks
                .iter()
                .any(|c| c.id == "rig.weights" && c.status == CheckStatus::Pass)
        );
    }
}
#[test]
fn unresolved_unsupported_and_truncated_accessors_are_not_pass() {
    let (mut j, b) = fixture();
    j["buffers"][0]["uri"] = json!("never-open-me.bin");
    assert!(has(
        &inspect_bytes(&glb(&j, &b)),
        "rig.accessor",
        CheckStatus::NotRun,
        "external URI"
    ));
    let (mut j, b) = fixture();
    j["extensionsUsed"] = json!(["UNKNOWN_resource"]);
    assert!(has(
        &inspect_bytes(&glb(&j, &b)),
        "rig.numeric",
        CheckStatus::NotRun,
        "extension"
    ));
    let (mut j, b) = fixture();
    j["accessors"][2]["byteOffset"] = json!(16);
    assert!(has(
        &inspect_bytes(&glb(&j, &b)),
        "rig.accessor",
        CheckStatus::Fail,
        "WEIGHTS_0"
    ));
    let (mut j, b) = fixture();
    j["meshes"][0]["primitives"][0]["attributes"]
        .as_object_mut()
        .unwrap()
        .remove("WEIGHTS_0");
    assert!(has(
        &inspect_bytes(&glb(&j, &b)),
        "rig.attributes",
        CheckStatus::Fail,
        "must pair"
    ));
    let (mut j, b) = fixture();
    j["meshes"][0]["primitives"][0]["attributes"]["JOINTS_1"] = json!(1);
    j["meshes"][0]["primitives"][0]["attributes"]["WEIGHTS_1"] = json!(2);
    assert!(has(
        &inspect_bytes(&glb(&j, &b)),
        "rig.weights",
        CheckStatus::NotRun,
        "multiple influence"
    ));
}
#[test]
fn bounded_real_file_is_read_and_temp_ownership_is_exclusive() {
    let path = std::env::temp_dir().join(format!("assetweft-aw022-rig-{}", std::process::id()));
    std::fs::create_dir(&path).expect("exclusive TEST_FIXTURE directory");
    let file = path.join("fixture.glb");
    let (j, b) = fixture();
    let bytes = glb(&j, &b);
    std::fs::write(&file, &bytes).unwrap();
    let result = inspect_file(
        &file,
        ValidationLimits {
            max_bytes: bytes.len() as u64,
        },
    );
    let small = inspect_file(&file, ValidationLimits { max_bytes: 1 });
    std::fs::remove_file(&file).unwrap();
    std::fs::remove_dir(&path).unwrap();
    assert!(has(
        &result.unwrap(),
        "rig.weights",
        CheckStatus::Pass,
        "decoded"
    ));
    assert!(small.is_err());
}
#[test]
fn sparse_missing_bin_and_count_mismatch_are_explicit() {
    let (mut j, b) = fixture();
    j["accessors"][2]["sparse"] = json!({"count":1,"indices":{"bufferView":1,"componentType":5121},"values":{"bufferView":2}});
    assert!(has(
        &inspect_bytes(&glb(&j, &b)),
        "rig.accessor",
        CheckStatus::NotRun,
        "sparse"
    ));
    let (j, b) = fixture();
    let mut bytes = glb(&j, &b);
    bytes.truncate(bytes.len() - 104);
    let n = bytes.len() as u32;
    bytes[8..12].copy_from_slice(&n.to_le_bytes());
    assert!(has(
        &inspect_bytes(&bytes),
        "rig.accessor",
        CheckStatus::NotRun,
        "missing BIN"
    ));
    let (mut j, b) = fixture();
    j["accessors"][2]["count"] = json!(2);
    assert!(has(
        &inspect_bytes(&glb(&j, &b)),
        "rig.attributes",
        CheckStatus::Fail,
        "count"
    ));
}
#[test]
fn binding_selects_each_nodes_actual_skin() {
    let (mut j, mut b) = fixture();
    j["nodes"]
        .as_array_mut()
        .unwrap()
        .extend([json!({}), json!({"mesh":0,"skin":1})]);
    j["skins"] = json!([{"joints":[1]},{"joints":[1,2]}]);
    b[12] = 1;
    let r = inspect_bytes(&glb(&j, &b));
    assert!(has(&r, "rig.weights", CheckStatus::Fail, "nodes[0].skin=0"));
    assert!(has(&r, "rig.weights", CheckStatus::Pass, "nodes[3].skin=1"));
}
