use assetweft_core::assurance::{EngineContractReport, ValidationLimits, checks};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn spec(engine: &str) -> Value {
    let source = [
        0., 1., 0., 0., 0., 0., 1., 0., 1., 0., 0., 0., 4., 5., 6., 1.,
    ];
    let (adapter, units, position, ibm, winding, tangent, texture) = match engine {
        "unity" => (
            "unity_csharp",
            "m",
            [-1., 2., 3.],
            [
                0., -1., 0., 0., 0., 0., 1., 0., -1., 0., 0., 0., -4., 5., 6., 1.,
            ],
            [2, 4, 3],
            -1.,
            json!({"map_role": "metallic_smoothness", "srgb": false,
                   "channel_order": ["metallic", "unused", "unused", "smoothness"],
                   "smoothness_inverted_from_roughness": true}),
        ),
        "unreal" => (
            "unreal_cpp",
            "cm",
            [100., 300., 200.],
            [
                0., 0., 1., 0., 1., 0., 0., 0., 0., 1., 0., 0., 400., 600., 500., 1.,
            ],
            [2, 4, 3],
            -1.,
            json!({"map_role": "orm", "srgb": false, "channel_order": ["ao", "roughness", "metallic"]}),
        ),
        "godot" => (
            "godot_gdscript",
            "m",
            [1., 2., 3.],
            source,
            [2, 3, 4],
            1.,
            json!({"map_role": "orm", "srgb": false, "channel_order": "RGB=ORM"}),
        ),
        _ => panic!("unexpected test engine"),
    };
    json!({
        "target_engine": engine, "asset_category": "rigged_action", "engine_version": "1.2.3",
        "plugin_version": "0.1.0", "adapter_kind": adapter, "adapter_entrypoint": "Import",
        "scale_units": units, "dependencies": ["b@1", "a@2"],
        "transform_sample": {
            "source_position_m": [1., 2., 3.], "target_position": position,
            "source_triangle": [2, 3, 4], "target_triangle": winding,
            "source_tangent_w": 1., "target_tangent_w": tangent,
            "source_ibm_col_major": source, "target_ibm_col_major": ibm, "max_tolerance": 0.
        },
        "pbr_textures": [texture, {"map_role": "normal", "srgb": false}, {"map_role": "base_color", "srgb": true}]
    })
}

fn inspect(path: &Path, spec: &Value) -> Result<EngineContractReport, String> {
    fs::write(path, spec.to_string()).unwrap();
    checks::inspect_engine_contract_file(path, ValidationLimits::default())
}

#[test]
fn three_bases_preserve_the_correct_rotation_and_translation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("engine.json");
    for engine in ["unity", "unreal", "godot"] {
        let mut spec = spec(engine);
        for category in [
            "static",
            "rigged_action",
            "2d_sprite",
            "presentation_bundle",
        ] {
            spec["asset_category"] = json!(category);
            let report = inspect(&path, &spec).unwrap();
            assert!(report.violations.is_empty(), "{engine}: {report:?}");
            assert_eq!(report.engine_contract_sha256.len(), 64);
            assert_eq!(report.engine_editor_runtime_status, "NOT_RUN");
            assert!(!report.headless_import_is_render_pass);
        }
    }
    let mut unreal = spec("unreal");
    unreal["adapter_kind"] = json!("unreal_python");
    unreal["transform_sample"]["source_ibm_col_major"][7] = json!(50);
    unreal["transform_sample"]["target_ibm_col_major"][11] = json!(0.5);
    assert!(inspect(&path, &unreal).unwrap().violations.is_empty());

    let mut unity = spec("unity");
    unity["pbr_textures"][0]["map_role"] = json!("mask_map");
    unity["pbr_textures"][0]["channel_order"] =
        json!(["metallic", "ao", "detail_mask", "smoothness"]);
    assert!(inspect(&path, &unity).unwrap().violations.is_empty());
}

#[test]
fn contract_rejects_wrong_basis_packing_color_space_and_pins() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("engine.json");
    for (engine, pointer, value, code) in [
        (
            "unity",
            "/transform_sample/target_triangle",
            json!([2, 3, 4]),
            "WindingOrderViolation",
        ),
        (
            "unreal",
            "/transform_sample/target_triangle",
            json!([2, 3, 4]),
            "WindingOrderViolation",
        ),
        (
            "unity",
            "/transform_sample/target_tangent_w",
            json!(1),
            "TangentHandednessViolation",
        ),
        (
            "unreal",
            "/transform_sample/target_tangent_w",
            json!(1),
            "TangentHandednessViolation",
        ),
        (
            "unity",
            "/transform_sample/target_ibm_col_major/12",
            json!(4),
            "InverseBindMatrixMismatch",
        ),
        (
            "unreal",
            "/transform_sample/target_ibm_col_major/13",
            json!(6),
            "InverseBindMatrixMismatch",
        ),
        (
            "unreal",
            "/transform_sample/target_position",
            json!([100, 200, 300]),
            "CoordinateBasisMismatch",
        ),
        (
            "unreal",
            "/scale_units",
            json!("m"),
            "CoordinateBasisMismatch",
        ),
        (
            "godot",
            "/transform_sample/target_triangle",
            json!([2, 4, 3]),
            "WindingOrderViolation",
        ),
        (
            "unity",
            "/pbr_textures/1/srgb",
            json!(true),
            "DataTextureSrgbViolation",
        ),
        (
            "unreal",
            "/pbr_textures/0/srgb",
            json!(true),
            "DataTextureSrgbViolation",
        ),
        (
            "unity",
            "/pbr_textures/0/smoothness_inverted_from_roughness",
            json!(false),
            "PbrChannelPackingViolation",
        ),
        (
            "unity",
            "/pbr_textures/0/smoothness_inverted_from_roughness",
            Value::Null,
            "PbrChannelPackingViolation",
        ),
        (
            "unreal",
            "/pbr_textures/0/channel_order",
            json!(["ao", "metallic", "roughness"]),
            "PbrChannelPackingViolation",
        ),
        (
            "unreal",
            "/adapter_kind",
            json!("unity_csharp"),
            "EngineAdapterMismatch",
        ),
        (
            "godot",
            "/engine_version",
            json!(" "),
            "IncompleteEnginePin",
        ),
        ("godot", "/plugin_version", json!(""), "IncompleteEnginePin"),
        (
            "godot",
            "/adapter_entrypoint",
            json!(""),
            "IncompleteEnginePin",
        ),
    ] {
        let mut spec = spec(engine);
        *spec.pointer_mut(pointer).unwrap() = value;
        let report = inspect(&path, &spec).unwrap();
        let codes: Vec<_> = report.violations.iter().map(|v| v.code).collect();
        assert_eq!(codes, [code], "{engine} {pointer}");
        assert_eq!(report.engine_editor_runtime_status, "NOT_RUN");
    }
}

#[test]
fn strict_schema_and_bounds_reject_invalid_input() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("engine.json");
    assert!(checks::inspect_engine_contract_file(&path, ValidationLimits::default()).is_err());
    for (pointer, value) in [
        ("/transform_sample/source_tangent_w", json!(0)),
        ("/transform_sample/target_tangent_w", json!(2)),
        ("/transform_sample/source_triangle", json!([1, 1, 2])),
        ("/transform_sample/max_tolerance", json!(-0.0000001)),
        ("/transform_sample/target_position/0", json!(1e20)),
        ("/pbr_textures", json!([])),
    ] {
        let mut spec = spec("godot");
        *spec.pointer_mut(pointer).unwrap() = value;
        assert!(inspect(&path, &spec).is_err(), "{pointer}");
    }
    for pointer in ["", "/transform_sample", "/pbr_textures/0"] {
        let mut spec = spec("godot");
        spec.pointer_mut(pointer).unwrap()["unknown"] = json!(true);
        assert!(inspect(&path, &spec).is_err(), "{pointer}");
    }
    let valid = spec("godot").to_string();
    for invalid in ["NaN", "1e999"] {
        let invalid = valid.replace(
            "\"max_tolerance\":0.0",
            &format!("\"max_tolerance\":{invalid}"),
        );
        assert_ne!(invalid, valid);
        fs::write(&path, invalid).unwrap();
        assert!(checks::inspect_engine_contract_file(&path, ValidationLimits::default()).is_err());
    }
    fs::write(&path, valid).unwrap();
    assert!(
        checks::inspect_engine_contract_file(&path, ValidationLimits { max_bytes: 5 }).is_err()
    );
}

#[test]
fn digest_normalizes_texture_order_aliases_and_signed_zero() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("engine.json");
    let mut spec = spec("unreal");
    let hash = inspect(&path, &spec).unwrap().engine_contract_sha256;
    spec["pbr_textures"][0]["channel_order"] = json!("RGB=ORM");
    spec["pbr_textures"].as_array_mut().unwrap().reverse();
    spec["dependencies"].as_array_mut().unwrap().reverse();
    spec["transform_sample"]["target_ibm_col_major"][0] = json!(-0.0);
    assert_eq!(hash, inspect(&path, &spec).unwrap().engine_contract_sha256);
    spec["transform_sample"]["target_ibm_col_major"][0] = json!(0.0000001);
    let report = inspect(&path, &spec).unwrap();
    assert_eq!(hash, report.engine_contract_sha256);
    assert_eq!(report.violations[0].code, "InverseBindMatrixMismatch");
    spec["engine_version"] = json!("2.0");
    assert_ne!(hash, inspect(&path, &spec).unwrap().engine_contract_sha256);
}
