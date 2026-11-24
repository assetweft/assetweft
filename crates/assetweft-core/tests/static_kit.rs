use assetweft_core::assurance::{
    StaticKitReport, ValidationLimits, checks::inspect_static_kit_file,
};
use serde_json::json;
use std::fs;

const WEBP: &[u8] =
    b"RIFF\x1e\0\0\0WEBPVP8L\x11\0\0\0\x2f\0\0\0\0\x07\x50\xc6\x42\x75\xa4\xff\x81\x88\xe8\x7f\0\0";

fn kit_glb(closed: bool, double_sided: bool) -> Vec<u8> {
    let mut vertices: Vec<[f32; 3]> = vec![[0., 0., 0.], [2., 0., 0.], [2., 1., 0.], [0., 1., 0.]];
    let mut indices: Vec<u16> = vec![0, 2, 1, 0, 3, 2];
    if closed {
        vertices.extend([
            [0., 0., 0.25],
            [2., 0., 0.25],
            [2., 1., 0.25],
            [0., 1., 0.25],
        ]);
        indices.extend([
            4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4,
            7,
        ]);
    }
    let mut bin: Vec<_> = vertices
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let index_offset = bin.len();
    bin.extend(indices.iter().flat_map(|v| v.to_le_bytes()));
    let index_len = bin.len() - index_offset;
    let mut document = json!({
        "asset":{"version":"2.0"}, "scene":0, "scenes":[{"nodes":[0]}],
        "nodes":[{"translation":[1,0,0],"children":[1,2]}, {"mesh":0}, {"name":"east","translation":[2,0,0]}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1,"material":0}]}],
        "materials":[{"alphaMode":if closed {"OPAQUE"} else {"MASK"},"doubleSided":double_sided}],
        "accessors":[
            {"bufferView":0,"componentType":5126,"count":vertices.len(),"type":"VEC3","min":[0,0,0],"max":[2,1,if closed {0.25} else {0.0}]},
            {"bufferView":1,"componentType":5123,"count":indices.len(),"type":"SCALAR"}
        ],
        "buffers":[{"byteLength":bin.len()}],
        "bufferViews":[{"buffer":0,"byteLength":index_offset},{"buffer":0,"byteOffset":index_offset,"byteLength":index_len}]
    });
    if closed {
        let image_offset = bin.len();
        bin.extend(WEBP);
        document["buffers"][0]["byteLength"] = json!(bin.len());
        document["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(json!({"buffer":0,"byteOffset":image_offset,"byteLength":WEBP.len()}));
        document["images"] = json!([{"bufferView":2,"mimeType":"image/webp"}]);
        document["textures"] = json!([{"extensions":{"EXT_texture_webp":{"source":0}}}]);
        document["extensionsUsed"] = json!(["EXT_texture_webp"]);
        document["extensionsRequired"] = json!(["EXT_texture_webp"]);
        bin.resize(bin.len().next_multiple_of(4), 0);
    }
    let mut json = serde_json::to_vec(&document).unwrap();
    json.resize(json.len().next_multiple_of(4), b' ');
    let mut glb = b"glTF".to_vec();
    glb.extend(2_u32.to_le_bytes());
    glb.extend(((28 + json.len() + bin.len()) as u32).to_le_bytes());
    glb.extend((json.len() as u32).to_le_bytes());
    glb.extend(b"JSON");
    glb.extend(json);
    glb.extend((bin.len() as u32).to_le_bytes());
    glb.extend(b"BIN\0");
    glb.extend(bin);
    glb
}

#[test]
fn architecture_checks_world_space_socket_scale_and_resource_deltas() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("wall.glb");
    let spec_path = dir.path().join("wall.json");
    fs::write(&file, kit_glb(true, false)).unwrap();
    let mut spec = json!({
        "kind":"architecture","expected_unit_scale":1,"scale_tolerance":0.001,
        "sockets":[{"name":"east","position":[3.001,0,0],"seam_tolerance_m":0.001}],
        "allow_non_watertight_foliage":false,
        "budget_before":{"vertex_count":24,"triangle_count":24,"texture_bytes":128}
    });
    let inspect = |spec: &serde_json::Value| -> StaticKitReport {
        fs::write(&spec_path, spec.to_string()).unwrap();
        inspect_static_kit_file(&file, &spec_path, ValidationLimits::default()).unwrap()
    };
    let report = inspect(&spec);
    assert!(report.violations.is_empty());
    assert_eq!(report.bounding_box_min, [1., 0., 0.]);
    assert_eq!(report.bounding_box_max, [3., 1., 0.25]);
    assert_eq!(report.primitive_count, 1);
    assert_eq!(report.resources.vertex_count, 8);
    assert_eq!(report.resources.triangle_count, 12);
    assert_eq!(report.unclosed_boundary_edges, 0);
    assert_eq!(report.optimization_record.delta_triangles, Some(-12));
    assert_eq!(report.resources.texture_bytes, 38);
    assert_eq!(report.optimization_record.delta_bytes, Some(-90));
    assert_eq!(report.optimization_record.visual_quality_status, "NOT_RUN");

    spec["expected_unit_scale"] = json!(2);
    spec["sockets"][0]["position"] = json!([3.001, 0.001, 0]);
    let report = inspect(&spec);
    assert_eq!(
        report
            .violations
            .iter()
            .map(|v| v.field)
            .collect::<Vec<_>>(),
        ["scale", "seam"]
    );
}

#[test]
fn foliage_policy_admits_open_cards_but_building_policy_requires_closed_shell() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("leaf.glb");
    let spec = dir.path().join("leaf.json");
    let mut contract = json!({
        "kind":"foliage","expected_unit_scale":1,"scale_tolerance":0.001,
        "sockets":[],"allow_non_watertight_foliage":true
    });
    fs::write(&spec, contract.to_string()).unwrap();
    for double_sided in [true, false] {
        fs::write(&file, kit_glb(false, double_sided)).unwrap();
        let report = inspect_static_kit_file(&file, &spec, ValidationLimits::default()).unwrap();
        assert!(report.violations.is_empty());
        assert_eq!(report.unclosed_boundary_edges, 4);
        assert_eq!(report.resources.triangle_count, 2);
        assert_eq!(
            report.double_sided_material_count,
            usize::from(double_sided)
        );
        assert_eq!(report.alpha_modes.into_iter().collect::<Vec<_>>(), ["MASK"]);
    }

    contract["kind"] = json!("architecture");
    contract["allow_non_watertight_foliage"] = json!(false);
    fs::write(&spec, contract.to_string()).unwrap();
    let report = inspect_static_kit_file(&file, &spec, ValidationLimits::default()).unwrap();
    assert_eq!(report.violations.len(), 1);
    assert_eq!(report.violations[0].field, "watertight");

    let mut malformed = kit_glb(false, false);
    let index = malformed.len() - 1;
    malformed[index] = 0xff;
    fs::write(&file, malformed).unwrap();
    let err = inspect_static_kit_file(&file, &spec, ValidationLimits::default())
        .err()
        .unwrap();
    assert_eq!(err.field, "file");
    assert!(err.detail.contains("triangle index"));
}
