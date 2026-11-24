use assetweft_core::assurance::{
    SpriteReport, ValidationLimits,
    checks::{SpriteError, inspect_sprite_file},
};
use png::{BitDepth, ColorType, Encoder};
use serde_json::{Value, json};
use sha2::Digest;
use std::fs;

fn manifest() -> Value {
    json!({
        "sheet": {"frame_width": 2, "frame_height": 1, "columns": 2, "rows": 2},
        "frames": (0..4).map(|index| json!({
            "index": index, "col": index % 2, "row": index / 2,
            "anchor_x": -0.0, "anchor_y": 1.0,
        })).collect::<Vec<_>>(),
        "looping": {"loop_cycle": true, "start_frame": 0, "end_frame": 3}
    })
}

const REPEATED: [u8; 8] = [9, 8, 7, 0, 6, 5, 4, 255];

fn rgba_sheet() -> Vec<u8> {
    let pixels = [REPEATED, [3, 2, 1, 128, 1, 2, 3, 255], REPEATED, [0; 8]].concat();
    png(4, 2, ColorType::Rgba, BitDepth::Eight, &pixels, |_| {})
}

fn png(
    width: u32,
    height: u32,
    color: ColorType,
    depth: BitDepth,
    pixels: &[u8],
    configure: impl FnOnce(&mut Encoder<'_, &mut Vec<u8>>),
) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(&mut bytes, width, height);
    encoder.set_color(color);
    encoder.set_depth(depth);
    configure(&mut encoder);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(pixels).unwrap();
    writer.finish().unwrap();
    bytes
}

fn inspect(png: &[u8], spec: &Value) -> Result<SpriteReport, SpriteError> {
    let dir = tempfile::tempdir().unwrap();
    let sheet = dir.path().join("sheet.png");
    let manifest = dir.path().join("spec.json");
    fs::write(&sheet, png).unwrap();
    fs::write(&manifest, serde_json::to_vec(spec).unwrap()).unwrap();
    inspect_sprite_file(&sheet, &manifest, ValidationLimits::default())
}

#[test]
fn reports_order_alpha_exact_duplicates_and_loop() {
    let report = inspect(&rgba_sheet(), &manifest()).unwrap();
    assert_eq!((report.width, report.height, report.frame_count), (4, 2, 4));
    assert!(report.has_transparency);
    assert_eq!(report.empty_frames, [3]);
    assert_eq!(report.duplicate_frame_groups, [vec![0, 2]]);
    assert_eq!(report.semantic_quality_status, "NOT_RUN");
    assert!(report.looping.loop_cycle);
    assert_eq!(
        (report.looping.start_frame, report.looping.end_frame),
        (0, 3)
    );
    for (pos, frame) in report.frames.iter().enumerate() {
        assert_eq!(frame.index as usize, pos);
        assert_eq!(frame.anchor[0].to_bits(), 0);
        assert_eq!(frame.anchor[1], 1.0);
        assert!(frame.has_alpha_channel);
    }
    assert_eq!(
        report.frames[0].frame_sha256,
        "873f5cd919d577795fa722f8c6a7aa4a345ff9e8bc0f08306c95a5256872440e"
    );
    assert_eq!(report.frames[1].transparent_pixels, 1);
    assert_eq!(report.frames[1].opaque_pixels, 1);
    assert!(!report.frames[1].empty_frame);
    assert!(report.frames[3].empty_frame);
}

#[test]
fn rejects_structural_negative_controls() {
    let sheet = rgba_sheet();
    for (pointer, mutation, reason) in [
        ("/frames/1/index", json!(2), "contiguous"),
        ("/frames/2/row", json!(0), "more than once"),
        ("/frames/1/col", json!(2), "outside sheet"),
        ("/frames/0/anchor_x", json!(1.2), "normalized"),
        ("/sheet/frame_height", json!(2), "exactly match"),
        ("/sheet/frame_width", json!(u32::MAX), "exactly match"),
        ("/sheet/rows", json!(0), "exactly match"),
        ("/looping/end_frame", json!(4), "loop bounds"),
        ("/looping/start_frame", json!(4), "loop bounds"),
        ("/frames", json!([]), "loop bounds"),
    ] {
        let mut spec = manifest();
        *spec.pointer_mut(pointer).unwrap() = mutation;
        assert!(
            matches!(inspect(&sheet, &spec), Err(SpriteError::Contract(e)) if e.contains(reason)),
            "{pointer}"
        );
    }
}

fn single_frame() -> Value {
    let mut spec = manifest();
    spec["sheet"] = json!({"frame_width": 1, "frame_height": 1, "columns": 1, "rows": 1});
    spec["frames"].as_array_mut().unwrap().truncate(1);
    spec["looping"] = json!({"loop_cycle": false, "start_frame": 0, "end_frame": 0});
    spec
}

#[test]
fn normalizes_supported_png_colors_to_rgba() {
    for (color, pixels, alpha, expected) in [
        (ColorType::Rgb, &[77, 77, 77][..], false, [77, 77, 77, 255]),
        (ColorType::Grayscale, &[77][..], false, [77, 77, 77, 255]),
        (
            ColorType::GrayscaleAlpha,
            &[77, 128][..],
            true,
            [77, 77, 77, 128],
        ),
        (ColorType::Indexed, &[0][..], true, [77, 77, 77, 128]),
    ] {
        let bytes = png(1, 1, color, BitDepth::Eight, pixels, |e| {
            if color == ColorType::Indexed {
                e.set_palette(vec![77; 3]);
                e.set_trns(vec![128]);
            }
        });
        let report = inspect(&bytes, &single_frame()).unwrap();
        let frame = &report.frames[0];
        assert_eq!(frame.has_alpha_channel, alpha);
        assert_eq!(
            frame.frame_sha256,
            sha2::Sha256::digest(expected)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
    }
}

#[test]
fn rejects_truncated_trailing_animated_and_unsupported_png() {
    let bytes = rgba_sheet();
    let mut trailing = bytes.clone();
    trailing.push(0);
    let animated = png(1, 1, ColorType::Rgba, BitDepth::Eight, &[0; 4], |e| {
        e.set_animated(1, 0).unwrap();
    });
    let mut malformed_animation = Vec::new();
    let mut encoder = Encoder::new(&mut malformed_animation, 1, 1);
    encoder.set_color(ColorType::Rgba);
    let mut writer = encoder.write_header().unwrap();
    writer.write_chunk(png::chunk::acTL, &[0; 8]).unwrap();
    writer.write_image_data(&[0; 4]).unwrap();
    writer.finish().unwrap();
    let deep = png(1, 1, ColorType::Rgba, BitDepth::Sixteen, &[0; 8], |_| {});
    let bad_index = png(1, 1, ColorType::Indexed, BitDepth::Eight, &[1], |e| {
        e.set_palette(vec![77; 3]);
    });
    for bad in [
        &b"invalid"[..],
        &bytes[..bytes.len() - 1],
        &trailing,
        &animated,
        &malformed_animation,
        &deep,
        &bad_index,
    ] {
        assert!(matches!(
            inspect(bad, &single_frame()),
            Err(SpriteError::InvalidInput { field: "file", .. })
        ));
    }
}
