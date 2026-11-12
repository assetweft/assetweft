//! 职责: AW-017:A 文件验证入口的缺陷型回归：格式/角色/来源分类/NOT_RUN/读取上限。
//! 依赖: assetweft-core 的 validation 模块；所有 fixture 均为脚本合成的 TEST_FIXTURE，非 AI/引擎产物。
//! 调用方: C1（cargo test）；不写数据库、不联网、不执行文件内容。
//! 不变量: 文件 hash 绑定原始字节；可解析≠角色满足；失败/未执行分开；来源只记录声明。

use std::path::PathBuf;

use assetweft_core::validation::{
    CheckStatus, NOT_RUN_ITEMS, RoleRequirement, SourceClass, SourceDeclaration,
    UnknownSourceClass, ValidationError, ValidationLimits, validate_bytes, validate_file,
};

fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures/validation");
    p.push(name);
    p
}

fn source() -> SourceDeclaration {
    SourceDeclaration::declared(SourceClass::TestFixture)
}

#[test]
fn mesh_only_fixture_passes_static_mesh_and_binds_byte_hash() {
    let report = validate_file(
        &fixture("mesh_only.glb"),
        RoleRequirement::StaticMesh,
        source(),
        ValidationLimits::default(),
    )
    .expect("fixture must be readable");
    assert_eq!(report.format_status, CheckStatus::Pass);
    assert_eq!(report.role.status, CheckStatus::Pass);
    assert_eq!(report.role.satisfied, Some(true));
    assert_eq!(report.bytes_read, 516);
    assert_eq!(
        report.file_sha256,
        "4658deaae78fb1ae6050baca75a8c098db6ccad311c74ba02097fceac01e45a5"
    );
    assert!(report.role.present.contains(&"mesh"));
    assert!(!report.role.present.contains(&"skin"));
}

#[test]
fn parseable_mesh_does_not_satisfy_rigged_character() {
    let report = validate_file(
        &fixture("mesh_only.glb"),
        RoleRequirement::RiggedCharacter,
        source(),
        ValidationLimits::default(),
    )
    .expect("fixture must be readable");
    // 格式通过但角色合同不满足：两件事分开报告。
    assert_eq!(report.format_status, CheckStatus::Pass);
    assert_eq!(report.role.status, CheckStatus::Fail);
    assert_eq!(report.role.satisfied, Some(false));
    assert_eq!(report.role.missing, vec!["skin"]);
    // 未做的数值检查不得因解析成功而变成 PASS。
    assert!(report.not_run.contains(&"skin.weights_numeric"));
    assert!(report.not_run.contains(&"rig.bind_matrices_numeric"));
}

#[test]
fn truncated_glb_fails_format_and_does_not_guess_role() {
    let report = validate_file(
        &fixture("truncated.glb"),
        RoleRequirement::StaticMesh,
        source(),
        ValidationLimits::default(),
    )
    .expect("truncated fixture still readable");
    assert_eq!(report.format_status, CheckStatus::Fail);
    assert_eq!(report.role.status, CheckStatus::NotRun);
    assert_eq!(report.role.satisfied, None);
    let format_item = report
        .checks
        .iter()
        .find(|c| c.id == "format.glb_container")
        .expect("format item present");
    assert_eq!(format_item.status, CheckStatus::Fail);
}

#[test]
fn non_glb_bytes_fail_format_instead_of_passing_by_magic() {
    let report = validate_file(
        &fixture("not_a_glb.bin"),
        RoleRequirement::StaticMesh,
        source(),
        ValidationLimits::default(),
    )
    .expect("fixture must be readable");
    assert_eq!(report.format_status, CheckStatus::Fail);
    assert_eq!(report.role.status, CheckStatus::NotRun);
}

#[test]
fn oversized_input_is_rejected_without_full_read() {
    let limits = ValidationLimits { max_bytes: 32 };
    let err = validate_file(
        &fixture("mesh_only.glb"),
        RoleRequirement::StaticMesh,
        source(),
        limits,
    )
    .expect_err("516-byte fixture must exceed the 32-byte limit");
    match err {
        ValidationError::InputTooLarge {
            limit,
            actual,
            bytes_read,
        } => {
            assert_eq!(limit, 32);
            assert_eq!(actual, 516);
            assert_eq!(bytes_read, 0, "must reject before reading contents");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn source_class_is_declared_not_verified_and_unknown_strings_reject() {
    assert_eq!(
        SourceClass::parse("AI_GENERATED"),
        Ok(SourceClass::AiGenerated)
    );
    assert_eq!(
        SourceClass::parse("MANUALLY_EDITED"),
        Ok(SourceClass::ManuallyEdited)
    );
    assert_eq!(
        SourceClass::parse("TEST_FIXTURE"),
        Ok(SourceClass::TestFixture)
    );
    for bad in ["ai_generated", "AiGenerated", "", "GENERATED"] {
        assert_eq!(SourceClass::parse(bad), Err(UnknownSourceClass));
    }
    let report = validate_bytes(
        b"glTF",
        RoleRequirement::StaticMesh,
        SourceDeclaration::declared(SourceClass::AiGenerated),
    );
    assert_eq!(report.source.class, SourceClass::AiGenerated);
    assert!(!report.source.verified_by_checker());
    assert_eq!(report.source.declared_license, None);
    // 来源标签不影响格式判定：非法字节仍然 FAIL。
    assert_eq!(report.format_status, CheckStatus::Fail);
}

#[test]
fn not_run_items_are_explicit() {
    let report = validate_file(
        &fixture("mesh_only.glb"),
        RoleRequirement::AnimatedCharacter,
        source(),
        ValidationLimits::default(),
    )
    .expect("fixture must be readable");
    assert_eq!(report.not_run.len(), 6);
    for item in NOT_RUN_ITEMS {
        assert!(report.not_run.contains(item), "missing NOT_RUN item {item}");
        assert!(
            !report
                .checks
                .iter()
                .any(|c| c.id == *item && c.status == CheckStatus::Pass),
            "NOT_RUN item {item} must never be reported as PASS"
        );
    }
    assert_eq!(report.role.missing, vec!["skin", "animation"]);
}

/// 用给定 JSON + 36 字节 BIN 构造一个合法 GLB 容器（TEST_FIXTURE，仅测试用）。
fn glb_from_json(json: &str) -> Vec<u8> {
    let mut js = json.as_bytes().to_vec();
    while !js.len().is_multiple_of(4) {
        js.push(b' ');
    }
    let bin = [0u8; 36];
    let total = 12 + 8 + js.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(js.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&js);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    out
}

#[test]
fn plain_json_is_not_a_declared_glb() {
    let bytes = std::fs::read(fixture("mesh_only.glb")).expect("fixture readable");
    let json_len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    let json_chunk = &bytes[20..20 + json_len];
    let report = validate_bytes(
        json_chunk,
        RoleRequirement::StaticMesh,
        SourceDeclaration::declared(SourceClass::TestFixture),
    );
    assert_eq!(report.format_status, CheckStatus::Fail);
    let item = report
        .checks
        .iter()
        .find(|c| c.id == "format.glb_container")
        .expect("format item");
    assert!(
        item.detail.contains("magic is not glTF"),
        "detail: {}",
        item.detail
    );
}

#[test]
fn max_limit_does_not_panic_or_wrap() {
    let report = validate_file(
        &fixture("mesh_only.glb"),
        RoleRequirement::StaticMesh,
        source(),
        ValidationLimits {
            max_bytes: u64::MAX,
        },
    )
    .expect("u64::MAX limit must not panic and must not silently read zero bytes");
    assert_eq!(report.bytes_read, 516);
    assert_eq!(report.format_status, CheckStatus::Pass);
}

#[test]
fn unbound_skin_does_not_satisfy_rigged_character() {
    // skins 数组存在，但 mesh node 不引用它：全局 count 不能充当 rig。
    let json = r#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0},{"name":"joint"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,1]}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],"buffers":[{"byteLength":36}],"skins":[{"joints":[1]}]}"#;
    let bytes = glb_from_json(json);
    let report = validate_bytes(
        &bytes,
        RoleRequirement::RiggedCharacter,
        SourceDeclaration::declared(SourceClass::TestFixture),
    );
    assert_eq!(
        report.format_status,
        CheckStatus::Pass,
        "container must parse"
    );
    assert_eq!(report.role.status, CheckStatus::Fail);
    assert_eq!(report.role.satisfied, Some(false));
    assert_eq!(report.role.missing, vec!["skin"]);
    let skin_item = report
        .checks
        .iter()
        .find(|c| c.id == "components.skin")
        .expect("skin item");
    assert_eq!(skin_item.status, CheckStatus::NotRun);
    assert!(skin_item.detail.contains("unbound skin"));
}

/// 构造「mesh node 绑定 skin0(joint=node1)，另有未绑定 skin1(joint=node2)，动画只 target 指定 node」的 GLB。
fn glb_two_skins_animation(target_node: u32) -> Vec<u8> {
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0,"skin":0}},{{"name":"j1"}},{{"name":"j2"}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],"skins":[{{"joints":[1]}},{{"joints":[2]}}],"animations":[{{"channels":[{{"sampler":0,"target":{{"node":{target_node},"path":"translation"}}}}],"samplers":[{{"input":1,"output":2}}]}}],"accessors":[{{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,1]}},{{"bufferView":0,"byteOffset":0,"componentType":5126,"count":2,"type":"SCALAR","min":[0],"max":[1]}},{{"bufferView":0,"byteOffset":8,"componentType":5126,"count":2,"type":"VEC3"}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],"buffers":[{{"byteLength":36}}]}}"#
    );
    glb_from_json(&json)
}

#[test]
fn animation_targeting_bound_skin_joint_satisfies_animated_character() {
    let bytes = glb_two_skins_animation(1); // node1 是已绑定 skin0 的 joint
    let report = validate_bytes(
        &bytes,
        RoleRequirement::AnimatedCharacter,
        SourceDeclaration::declared(SourceClass::TestFixture),
    );
    assert_eq!(
        report.format_status,
        CheckStatus::Pass,
        "container must parse"
    );
    assert_eq!(report.role.status, CheckStatus::Pass);
    assert_eq!(report.role.satisfied, Some(true));
    assert!(report.role.present.contains(&"animation"));
}

#[test]
fn animation_targeting_unbound_skin_joint_does_not_satisfy_animated_character() {
    let bytes = glb_two_skins_animation(2); // node2 属于未被 mesh node 引用的 skin1
    let report = validate_bytes(
        &bytes,
        RoleRequirement::AnimatedCharacter,
        SourceDeclaration::declared(SourceClass::TestFixture),
    );
    assert_eq!(
        report.format_status,
        CheckStatus::Pass,
        "container must parse"
    );
    assert_eq!(report.role.status, CheckStatus::Fail);
    assert_eq!(report.role.missing, vec!["animation"]);
    let item = report
        .checks
        .iter()
        .find(|c| c.id == "components.animation")
        .expect("animation item");
    assert_eq!(item.status, CheckStatus::NotRun);
    assert!(item.detail.contains("orphan animation"));
}

// TEST_FIXTURE chunk assembly; depends on glb_from_json, called only by regressions.
// Invariant: preserve payloads and update the declared length after each append.
fn append_chunk(bytes: &mut Vec<u8>, ty: &[u8; 4], payload: &[u8]) {
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(ty);
    bytes.extend_from_slice(payload);
    let len = bytes.len() as u32;
    bytes[8..12].copy_from_slice(&len.to_le_bytes());
}

fn duplicate_known_chunk_fails_container(ty: &[u8; 4]) {
    let mut bytes = json_only_glb(r#"{"asset":{"version":"2.0"}}"#);
    append_chunk(&mut bytes, b"BIN\0", &[0; 4]);
    append_chunk(&mut bytes, ty, &[0; 4]);
    let report = validate_bytes(&bytes, RoleRequirement::StaticMesh, source());
    assert_eq!(report.format_status, CheckStatus::Fail, "duplicate {ty:?}");
    assert_eq!(report.role.status, CheckStatus::NotRun);
    assert!(
        report
            .checks
            .iter()
            .any(|c| c.id == "format.glb_container" && c.status == CheckStatus::Fail)
    );
}

#[test]
fn duplicate_known_bin_fails_container() {
    duplicate_known_chunk_fails_container(b"BIN\0");
}

#[test]
fn duplicate_known_json_fails_container() {
    duplicate_known_chunk_fails_container(b"JSON");
}

#[test]
fn known_chunk_order_and_unknown_tail_contract() {
    for tail in [vec![], vec![b"BIN\0"], vec![b"BIN\0", b"TEST", b"TEST"]] {
        let mut bytes = json_only_glb(r#"{"asset":{"version":"2.0"}}"#);
        for ty in tail {
            append_chunk(&mut bytes, ty, &[0; 4]);
        }
        assert_eq!(
            validate_bytes(&bytes, RoleRequirement::StaticMesh, source()).format_status,
            CheckStatus::Pass
        );
    }
    for tail in [
        vec![b"JSON"],
        vec![b"TEST", b"BIN\0"],
        vec![b"BIN\0", b"TEST", b"BIN\0"],
        vec![b"BIN\0", b"TEST", b"JSON"],
    ] {
        let mut bytes = json_only_glb(r#"{"asset":{"version":"2.0"}}"#);
        for ty in tail {
            append_chunk(&mut bytes, ty, &[0; 4]);
        }
        assert_eq!(
            validate_bytes(&bytes, RoleRequirement::StaticMesh, source()).format_status,
            CheckStatus::Fail
        );
    }
}

#[test]
fn malformed_chunk_inputs_fail_without_panicking() {
    let good = json_only_glb(r#"{"asset":{"version":"2.0"}}"#);
    let mut cases = Vec::new();
    for n in 0..good.len() {
        cases.push(good[..n].to_vec());
    }
    for len in [0u32, 12, u32::MAX] {
        let mut bytes = good.clone();
        bytes[8..12].copy_from_slice(&len.to_le_bytes());
        cases.push(bytes);
    }
    for len in [1u32, 3, u32::MAX - 3, u32::MAX] {
        let mut bytes = good.clone();
        bytes[12..16].copy_from_slice(&len.to_le_bytes());
        cases.push(bytes);
    }
    let mut first_bin = good.clone();
    first_bin[16..20].copy_from_slice(b"BIN\0");
    cases.push(first_bin);
    let mut trailing_header = good.clone();
    trailing_header.extend_from_slice(&[0; 4]);
    let len = trailing_header.len() as u32;
    trailing_header[8..12].copy_from_slice(&len.to_le_bytes());
    cases.push(trailing_header);
    for bytes in cases {
        let report = validate_bytes(&bytes, RoleRequirement::StaticMesh, source());
        assert_eq!(report.format_status, CheckStatus::Fail);
        assert_eq!(report.role.status, CheckStatus::NotRun);
    }
}

// Synthetic JSON-only container derived from the established fixture helper.
fn json_only_glb(json: &str) -> Vec<u8> {
    let mut bytes = glb_from_json(json);
    let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    bytes.truncate(20 + json_len);
    let len = bytes.len() as u32;
    bytes[8..12].copy_from_slice(&len.to_le_bytes());
    bytes
}
