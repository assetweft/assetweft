use assetweft_core::assurance::{
    ValidationLimits, VfxReport,
    checks::{VfxInputError, inspect_vfx_file},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn spec() -> Value {
    json!({
        "vfx_mode": "parametric",
        "authoring_state": "native_editable",
        "requires_native_editable": true,
        "timing": {"fps": 20, "frame_count": 2, "duration_ms": 101, "loop_cycle": true, "blend_mode": "additive"},
        "parameters": [{"name": "intensity", "value": -0.0, "min": 0, "max": 1}]
    })
}

fn inspect(spec: &str, png: Option<&[u8]>) -> Result<VfxReport, VfxInputError> {
    let dir = tempfile::tempdir().unwrap();
    let spec_path = dir.path().join("vfx.json");
    let png_path = dir.path().join("atlas.png");
    fs::write(&spec_path, spec).unwrap();
    if let Some(png) = png {
        fs::write(&png_path, png).unwrap();
    }
    inspect_vfx_file(
        &spec_path,
        png.map(|_| png_path.as_path()),
        ValidationLimits::default(),
    )
}

fn atlas(alpha: u8) -> Vec<u8> {
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, 2, 1);
    encoder.set_color(png::ColorType::Rgba);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&[0, 0, 0, alpha, 1, 2, 3, 255])
        .unwrap();
    writer.finish().unwrap();
    png
}

#[test]
fn parametric_observes_timing_and_parameters() {
    let spec = spec().to_string();
    let report = inspect(&spec, None).unwrap();
    assert!(report.violations.is_empty());
    assert_eq!(report.parameters[0].value.to_bits(), 0);
    assert_eq!(report.vfx_spec_sha256, digest(spec.as_bytes()));
    assert_eq!(report.flipbook_sha256, None);
    assert_eq!(report.visual_engine_status, "NOT_RUN");
}

#[test]
fn duration_uses_micro_unit_rounding_at_one_millisecond_boundary() {
    for (expected_ms, passes) in [(100.000_000_4, true), (100.000_000_6, false)] {
        let mut spec = spec();
        spec["timing"]["fps"] = json!(2000.0 / expected_ms);
        spec["timing"]["duration_ms"] = json!(99);
        let report = inspect(&spec.to_string(), None).unwrap();
        assert_eq!(report.violations.is_empty(), passes, "{report:?}");
    }
}

#[test]
fn parametric_rejects_inconsistent_declarations() {
    for (pointer, value, field) in [
        ("/authoring_state", json!("baked"), "authoring_state"),
        ("/timing/duration_ms", json!(102), "timing.duration_ms"),
        ("/timing/fps", json!(0), "timing"),
        ("/parameters/0/value", json!(2), "parameters.value"),
        ("/parameters/0/name", json!("  "), "parameters.name"),
        ("/parameters", json!([]), "parameters"),
    ] {
        let mut invalid = spec();
        *invalid.pointer_mut(pointer).unwrap() = value;
        let report = inspect(&invalid.to_string(), None).unwrap();
        assert_eq!(report.violations.len(), 1, "{pointer}");
        assert_eq!(report.violations[0].field, field);
    }
}

#[test]
fn flipbook_and_hybrid_require_matching_atlas_with_transparency() {
    let mut spec = spec();
    spec["flipbook"] = json!({
        "frame_width": 1, "frame_height": 1, "columns": 2, "rows": 1,
        "require_transparency": true
    });
    let transparent = atlas(128);
    for mode in ["flipbook", "hybrid"] {
        spec["vfx_mode"] = json!(mode);
        let report = inspect(&spec.to_string(), Some(&transparent)).unwrap();
        assert!(report.violations.is_empty());
        let facts = report.flipbook_facts.unwrap();
        assert!(facts.has_alpha_channel);
        assert_eq!(facts.transparent_pixel_count, 1);
        assert_eq!(report.flipbook_sha256.unwrap(), digest(&transparent));
        assert_eq!(report.visual_engine_status, "NOT_RUN");
    }
    let missing = inspect(&spec.to_string(), None).unwrap();
    assert_eq!(missing.violations[0].field, "flipbook");
    let opaque = inspect(&spec.to_string(), Some(&atlas(255))).unwrap();
    assert_eq!(opaque.violations[0].field, "flipbook.require_transparency");
    spec["flipbook"]["columns"] = json!(3);
    let mismatch = inspect(&spec.to_string(), Some(&transparent)).unwrap();
    assert_eq!(mismatch.violations[0].field, "flipbook");
}

#[test]
fn invalid_json_and_png_are_input_errors() {
    for input in ["{", &spec().to_string().replace("-0.0", "NaN")] {
        assert_eq!(inspect(input, None).unwrap_err().field, "spec");
    }
    let mut extra = spec();
    extra["timing"]["extra"] = json!(true);
    assert_eq!(inspect(&extra.to_string(), None).unwrap_err().field, "spec");
    assert_eq!(
        inspect(&spec().to_string(), Some(b"broken png"))
            .unwrap_err()
            .field,
        "flipbook"
    );
}
