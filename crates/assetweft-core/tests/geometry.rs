//! 职责: 测试 AW-022:A 静态几何公开合同与缺陷边界。
//! 依赖: assetweft-core 与 std；每个内存生成 GLB 均为 TEST_FIXTURE，非真实素材。
//! 调用方: cargo test 与 C1。
//! 不变量: 不要求 Rig；解析、外部资源、用户尺度与视觉结论分开。
use assetweft_core::geometry::{
    GeometryError, LocalTransform, Resource, inspect_bytes, inspect_file,
};
use assetweft_core::validation::{ValidationError, ValidationLimits};

// TEST_FIXTURE: programmatically creates a JSON-only GLB; external buffer is never opened.
fn glb(json: &str) -> Vec<u8> {
    let mut json = json.as_bytes().to_vec();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let len = 20 + json.len();
    let mut out = Vec::new();
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(len as u32).to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json);
    out
}
fn fixture_json(material: usize) -> String {
    format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":36,"uri":"missing.bin"}}],"bufferViews":[{{"buffer":0,"byteLength":36}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}}],"images":[{{"uri":"missing.png"}}],"textures":[{{"source":0}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"material":{material}}}]}}],"nodes":[{{"mesh":0,"scale":[2,3,4]}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
    )
}
#[test]
fn static_mesh_scale_material_and_unresolved_references() {
    let report = inspect_bytes(&glb(&fixture_json(0))).expect("TEST_FIXTURE static GLB");
    assert_eq!(report.input_units, "UNKNOWN");
    assert_eq!(report.input_coordinates, "UNKNOWN");
    assert_eq!(report.nodes[0].mesh, Some(0));
    assert!(matches!(
        report.nodes[0].transform,
        LocalTransform::Trs {
            scale: [2.0, 3.0, 4.0],
            ..
        }
    ));
    assert_eq!(report.primitives[0].material, Some(0));
    assert_eq!(report.accessors[0].view, Some(0));
    assert_eq!(report.buffer_views[0].buffer, 0);
    assert!(report.buffer_views[0].within_declared_buffer);
    assert_eq!(report.materials[0].textures, vec![("base_color", 0)]);
    assert_eq!(report.textures, vec![(0, 0)]);
    assert_eq!(
        report.buffers,
        vec![Resource::UriUnresolved("missing.bin".into())]
    );
    assert_eq!(
        report.images,
        vec![Resource::UriUnresolved("missing.png".into())]
    );
    assert!(report.not_run.contains(&"rendering"));
}
#[test]
fn bad_reference_and_truncated_container_are_rejected() {
    let error = inspect_bytes(&glb(&fixture_json(5))).unwrap_err();
    assert!(matches!(error, GeometryError::Invalid(ref msg) if msg.contains("material")));
    let mut data = glb(&fixture_json(0));
    data.truncate(data.len() - 3);
    assert!(matches!(
        inspect_bytes(&data),
        Err(GeometryError::Invalid(_))
    ));
}
#[test]
fn file_size_limit_applies_before_parsing() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/validation/mesh_only.glb");
    assert!(matches!(
        inspect_file(&path, ValidationLimits { max_bytes: 16 }),
        Err(GeometryError::Read(ValidationError::InputTooLarge { .. }))
    ));
}

#[test]
fn buffer_view_past_declared_length_is_reported() {
    let json = fixture_json(0).replace(
        "\"buffer\":0,\"byteLength\":36",
        "\"buffer\":0,\"byteLength\":40",
    );
    let report = inspect_bytes(&glb(&json)).expect("TEST_FIXTURE remains structurally parseable");
    assert!(!report.buffer_views[0].within_declared_buffer);
}

#[test]
fn missing_binary_chunk_is_explicit() {
    let json = fixture_json(0).replace(",\"uri\":\"missing.bin\"", "");
    let report = inspect_bytes(&glb(&json)).expect("TEST_FIXTURE document parses");
    assert_eq!(report.buffers[0], Resource::Embedded { available: false });
}

#[test]
fn embedded_static_mesh_needs_no_rig() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/validation/mesh_only.glb");
    let report =
        inspect_file(&path, ValidationLimits::default()).expect("TEST_FIXTURE static mesh");
    assert!(!report.primitives.is_empty());
    assert_eq!(report.buffers[0], Resource::Embedded { available: true });
}
