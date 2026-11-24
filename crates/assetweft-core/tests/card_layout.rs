use assetweft_core::assurance::{
    LayoutViolation, ValidationLimits,
    checks::{CardLayoutError, check_card_template_file},
};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn template() -> Value {
    json!({
        "kind": "card",
        "canvas": {"width_px": 100, "height_px": 100},
        "illustration_slot": {"slot_id": "art", "x": 10, "y": 10, "width": 40,
            "height": 40, "editable_independently": true, "fit_mode": "contain"},
        "text_boxes": [
            {"box_id": "name", "x": 0, "y": 50, "width": 50, "height": 20,
                "text": "松林 白鹿", "font_px": 8, "line_height_px": 10,
                "max_chars_per_line": 3, "max_lines": 2},
            {"box_id": "class", "x": 50, "y": 50, "width": 50, "height": 10,
                "text": "林", "font_px": 8, "line_height_px": 10,
                "max_chars_per_line": 1, "max_lines": 1}
        ]
    })
}

fn png(path: &Path, width: u32, height: u32, pixel: u8, animated: bool) {
    let mut encoder = png::Encoder::new(fs::File::create(path).unwrap(), width, height);
    if animated {
        encoder.set_animated(1, 0).unwrap();
    }
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&vec![pixel; (width * height) as usize])
        .unwrap();
    writer.finish().unwrap();
}

#[test]
fn canonical_layout_and_exact_fitted_boxes_are_independent_of_art() {
    let dir = tempfile::tempdir().unwrap();
    let spec_path = dir.path().join("template.json");
    let a_path = dir.path().join("a.png");
    let b_path = dir.path().join("b.png");
    let check = |spec: &Value, art: Option<&Path>| {
        fs::write(&spec_path, spec.to_string()).unwrap();
        check_card_template_file(&spec_path, art, ValidationLimits::default()).unwrap()
    };
    let mut spec = template();
    png(&a_path, 3, 2, 10, false);
    png(&b_path, 2, 3, 90, false);
    let a = check(&spec, Some(&a_path));
    let fitted = a.illustration_fitted_box.unwrap();
    assert_eq!(
        (fitted.width_numerator, fitted.height_numerator),
        (240, 160)
    );
    assert_eq!(
        (fitted.x_numerator, fitted.y_numerator, fitted.denominator),
        (60, 100, 6)
    );
    spec["text_boxes"].as_array_mut().unwrap().reverse();
    spec["illustration_slot"]["art_asset_sha256"] = json!("b".repeat(64));
    let b = check(&spec, Some(&b_path));
    assert_eq!(a.template_layout_sha256, b.template_layout_sha256);
    assert_ne!(a.illustration_sha256, b.illustration_sha256);
    assert_eq!(b.visual_quality_status, "NOT_RUN");
    spec["illustration_slot"]["fit_mode"] = json!("cover");
    let cover = check(&spec, Some(&b_path));
    assert_ne!(a.template_layout_sha256, cover.template_layout_sha256);
    let fitted = cover.illustration_fitted_box.unwrap();
    assert_eq!(
        (fitted.width_numerator, fitted.height_numerator),
        (160, 240)
    );
    assert_eq!(
        (fitted.x_numerator, fitted.y_numerator, fitted.denominator),
        (40, 0, 4)
    );
    let without_art = check(&spec, None);
    assert_eq!(
        without_art.template_layout_sha256,
        cover.template_layout_sha256
    );
    assert!(without_art.illustration_fitted_box.is_none());
    spec["text_boxes"][0]["text"] = json!("土");
    assert_ne!(
        check(&spec, None).template_layout_sha256,
        cover.template_layout_sha256
    );
}

#[test]
fn layout_contract_and_input_errors_are_distinct() {
    use LayoutViolation::*;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("template.json");
    let art = dir.path().join("animated.png");
    let check = |spec: &Value, illustration: Option<&Path>| {
        fs::write(&path, spec.to_string()).unwrap();
        check_card_template_file(&path, illustration, ValidationLimits::default())
    };
    for (pointer, value, expected) in [
        ("/canvas/width_px", json!(4097), CanvasDimensions),
        ("/illustration_slot/x", json!(u32::MAX), IllustrationSlot),
        (
            "/illustration_slot/editable_independently",
            json!(false),
            SlotNotIndependent,
        ),
        ("/text_boxes", json!([]), TextBoxCount),
        ("/text_boxes/1/box_id", json!("name"), TextBoxId),
        ("/text_boxes/1/x", json!(51), TextBoxBounds),
        ("/text_boxes/1/x", json!(49), Overlap),
        ("/text_boxes/0/line_height_px", json!(7), TextMetrics),
        ("/text_boxes/0/max_lines", json!(u32::MAX), TextOverflow),
        ("/text_boxes/0/text", json!("松林白鹿"), TextOverflow),
        ("/text_boxes/0/text", json!("松\n林\n鹿"), TextOverflow),
    ] {
        let mut spec = template();
        *spec.pointer_mut(pointer).unwrap() = value;
        assert!(
            matches!(check(&spec, None), Err(CardLayoutError::Contract(actual)) if actual == expected),
            "{pointer}"
        );
    }

    let mut spec = template();
    spec["text_boxes"][0]["y"] = json!(49);
    check(&spec, None).unwrap();
    spec["allow_text_over_illustration"] = json!(false);
    assert!(matches!(
        check(&spec, None),
        Err(CardLayoutError::Contract(Overlap))
    ));
    for (key, value) in [("kind", json!("unknown")), ("extra", json!(true))] {
        let mut spec = template();
        spec[key] = value;
        assert!(matches!(
            check(&spec, None),
            Err(CardLayoutError::InvalidInput {
                field: "template",
                ..
            })
        ));
    }
    png(&art, 2, 3, 90, true);
    assert!(matches!(
        check(&template(), Some(&art)),
        Err(CardLayoutError::InvalidInput {
            field: "illustration",
            ..
        })
    ));
}
