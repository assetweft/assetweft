use assetweft_core::{rig, validation::CheckStatus};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};

const WEBP: &str = "EXT_texture_webp";
const PIXEL: &[u8] = b"RIFF\x1a\0\0\0WEBPVP8L\x0e\0\0\0/\0\0\0\0\x07\x10\x11\xfd\x0fDD\xff\x03";
const MESH: &[u8] = include_bytes!("../../assetweft-core/tests/fixtures/validation/mesh_only.glb");
type Change = fn(&mut Value);

fn split_glb(bytes: &[u8]) -> (Value, &[u8]) {
    let json_end = 20 + u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    (
        serde_json::from_slice(&bytes[20..json_end]).unwrap(),
        &bytes[json_end + 8..],
    )
}

fn webp_document() -> Value {
    let (mut doc, bin) = split_glb(MESH);
    doc["extensionsUsed"] = json!([WEBP]);
    doc["extensionsRequired"] = json!([WEBP]);
    doc["buffers"][0]["byteLength"] = json!(bin.len() + PIXEL.len());
    doc["bufferViews"].as_array_mut().unwrap().push(json!({
        "buffer": 0, "byteOffset": bin.len(), "byteLength": PIXEL.len()
    }));
    doc["images"] = json!([{"bufferView": 1, "mimeType": "image/webp"}]);
    doc["textures"] = json!([{"extensions": {(WEBP): {"source": 0}}}]);
    doc
}

fn glb(doc: &Value) -> Vec<u8> {
    let (_, bin) = split_glb(MESH);
    let mut bin = bin.to_vec();
    bin.extend_from_slice(PIXEL);
    encode_glb(doc, bin)
}

fn encode_glb(doc: &Value, bin: Vec<u8>) -> Vec<u8> {
    encode_glb_json(serde_json::to_vec(doc).unwrap(), bin)
}

fn encode_glb_json(mut json: Vec<u8>, mut bin: Vec<u8>) -> Vec<u8> {
    json.resize(json.len().next_multiple_of(4), b' ');
    bin.resize(bin.len().next_multiple_of(4), 0);
    let mut bytes = b"glTF\x02\0\0\0".to_vec();
    bytes.extend_from_slice(&((28 + json.len() + bin.len()) as u32).to_le_bytes());
    bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
    bytes.extend_from_slice(b"JSON");
    bytes.extend_from_slice(&json);
    bytes.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    bytes.extend_from_slice(b"BIN\0");
    bytes.extend_from_slice(&bin);
    bytes
}

fn invoke(command: &str, path: &Path) -> (Option<i32>, Value) {
    let mut cli = Command::new(env!("CARGO_BIN_EXE_assetweft"));
    cli.arg(command);
    if command == "validate" {
        cli.args(["--role", "static-mesh"]);
    }
    let output = cli.arg(path).output().unwrap();
    assert!(output.stderr.is_empty(), "{output:?}");
    assert_eq!(output.stdout.iter().filter(|&&b| b == b'\n').count(), 1);
    (
        output.status.code(),
        serde_json::from_slice(&output.stdout).unwrap(),
    )
}

#[test]
fn required_embedded_webp_reports_original_bytes_and_texture_links() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("required-webp.glb");
    let bytes = glb(&webp_document());
    fs::write(&path, &bytes).unwrap();
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    let (exit, validated) = invoke("validate", &path);
    assert_eq!(exit, Some(0), "{validated}");
    assert_eq!(validated["status"], "PASS");
    assert_eq!(validated["results"]["format_status"], "PASS");
    assert_eq!(validated["results"]["file_sha256"], digest);
    assert_eq!(validated["results"]["bytes_read"], bytes.len());

    let (exit, inspected) = invoke("inspect", &path);
    assert_eq!(exit, Some(0), "{inspected}");
    assert_eq!(inspected["status"], "PASS");
    let geometry = &inspected["results"]["geometry"];
    assert_eq!(geometry["textures"], json!([[0, 0]]));
    assert_eq!(geometry["extensions_used"], json!([WEBP]));
    assert_eq!(geometry["extensions_required"], json!([WEBP]));
    assert_eq!(
        geometry["images"],
        json!([{"BufferView": {"buffer": 0, "offset": 36, "length": PIXEL.len()}}])
    );
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn optional_webp_selects_unresolved_uri_over_png_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fallback.glb");
    let mut doc = webp_document();
    doc.as_object_mut().unwrap().remove("extensionsRequired");
    let uri = "missing/Texture.WeBP?revision=1#preview";
    doc["images"] = json!([{"uri": "missing/fallback.png"}, {"uri": uri}]);
    doc["textures"][0]["source"] = json!(0);
    doc["textures"][0]["extensions"][WEBP]["source"] = json!(1);
    fs::write(&path, glb(&doc)).unwrap();

    let (exit, inspected) = invoke("inspect", &path);
    assert_eq!(exit, Some(0), "{inspected}");
    let geometry = &inspected["results"]["geometry"];
    assert_eq!(geometry["textures"], json!([[0, 1]]));
    assert_eq!(geometry["images"][1], json!({"UriUnresolved": uri}));
    assert_eq!(geometry["extensions_required"], json!([]));
}

#[test]
fn extensionless_external_webp_and_fallback_stay_unresolved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("external.glb");
    let mut doc = webp_document();
    doc.as_object_mut().unwrap().remove("extensionsRequired");
    let uris = ["tiles/fallback", "tiles/albedo?revision=1"];
    doc["images"] = json!([{"uri": uris[0]}, {"uri": uris[1]}]);
    doc["textures"][0] = json!({"source": 0, "extensions": {(WEBP): {"source": 1}}});
    fs::write(&path, glb(&doc)).unwrap();
    let (validated, validation) = invoke("validate", &path);
    let (inspected, inspection) = invoke("inspect", &path);
    assert_eq!(
        (validated, inspected),
        (Some(0), Some(0)),
        "{validation} {inspection}"
    );
    let geometry = &inspection["results"]["geometry"];
    assert_eq!(geometry["textures"], json!([[0, 1]]));
    assert_eq!(
        geometry["images"],
        json!([{"UriUnresolved": uris[0]}, {"UriUnresolved": uris[1]}])
    );
}

#[test]
fn webp_source_accepts_exact_decimal_and_exponent_integers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("integer.glb");
    let mut doc = webp_document();
    doc["textures"][0]["extensions"][WEBP]["source"] = json!("SOURCE");
    let raw = serde_json::to_string(&doc).unwrap();
    let bytes = glb(&webp_document());
    let (_, bin) = split_glb(&bytes);
    let failures: Vec<_> = ["0.0", "0e0", "0.0001e-999", "4294967296", "\"0\""]
        .into_iter()
        .filter_map(|source| {
            let bytes =
                encode_glb_json(raw.replace("\"SOURCE\"", source).into_bytes(), bin.to_vec());
            fs::write(&path, bytes).unwrap();
            let actual = (invoke("validate", &path).0, invoke("inspect", &path).0);
            let expected = if matches!(source, "0.0" | "0e0") {
                (Some(0), Some(0))
            } else {
                (Some(1), Some(2))
            };
            (actual != expected).then_some((source, actual, expected))
        })
        .collect();
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn invalid_webp_document_reports_the_gltf_field_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad-alpha.glb");
    let mut doc = webp_document();
    doc["materials"] = json!([{"alphaMode": "BOGUS"}]);
    fs::write(&path, glb(&doc)).unwrap();
    for (command, code) in [("validate", 1), ("inspect", 2)] {
        let (exit, reply) = invoke(command, &path);
        assert_eq!(exit, Some(code), "{reply}");
        assert!(
            reply["errors"][0]["message"]
                .as_str()
                .unwrap()
                .contains("materials[0].alphaMode"),
            "{command}: {reply}"
        );
    }
}

#[test]
fn embedded_webp_and_core_texture_sources_obey_image_layout() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("texture.glb");
    let cases: &[(&str, Change)] = &[
        ("byteStride", |doc| {
            doc["bufferViews"][1]["byteStride"] = json!(4)
        }),
        ("target", |doc| {
            doc["bufferViews"][1]["target"] = json!(34962)
        }),
        ("missing nonzero URI", |doc| {
            doc["buffers"]
                .as_array_mut()
                .unwrap()
                .push(json!({"byteLength": 128}));
            doc["bufferViews"][1]["buffer"] = json!(1);
        }),
        ("empty buffer URI", |doc| {
            doc["buffers"][0]["uri"] = json!("")
        }),
        ("bare core WebP", |doc| {
            let fields = doc.as_object_mut().unwrap();
            fields.remove("extensionsUsed");
            fields.remove("extensionsRequired");
            doc["textures"][0] = json!({"source": 0});
        }),
        ("bare core WebP alongside extension", |doc| {
            doc["textures"]
                .as_array_mut()
                .unwrap()
                .push(json!({"source": 0}));
        }),
    ];
    let mut failures = Vec::new();
    for (name, change) in cases {
        let mut doc = webp_document();
        change(&mut doc);
        fs::write(&path, glb(&doc)).unwrap();
        let actual = (invoke("validate", &path).0, invoke("inspect", &path).0);
        if actual != (Some(1), Some(2)) {
            failures.push((name, actual));
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn motion_idle_malformed_glb_returns_one_error_receipt_without_output() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("malformed.glb");
    let output = dir.path().join("clips.glb");
    let mut mismatched = MESH.to_vec();
    mismatched.extend_from_slice(b"tail");
    for (bytes, reason) in [
        (&MESH[..10], "too short for a GLB header"),
        (&mismatched, "declared length"),
    ] {
        fs::write(&input, bytes).unwrap();
        let process = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .arg("motion-strike")
            .arg(&input)
            .args(["--with-idle", "--out"])
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(process.status.code(), Some(1), "{process:?}");
        assert!(process.stderr.is_empty(), "{process:?}");
        assert_eq!(process.stdout.split_inclusive(|&b| b == b'\n').count(), 1);
        let reply: Value = serde_json::from_slice(&process.stdout).unwrap();
        assert_eq!(reply["status"], "NOT_RUN", "{reply}");
        assert!(reply["results"].is_null(), "{reply}");
        assert!(
            reply["errors"][0]["message"]
                .as_str()
                .unwrap()
                .contains(reason),
            "{reply}"
        );
        assert!(!output.exists());
    }
}

#[test]
fn rig_checks_still_decode_original_bin_with_webp_texture() {
    let input = include_bytes!("../../../examples/rigged-simple/RiggedSimple.glb");
    let baseline = rig::inspect_bytes(input);
    let (mut doc, bin) = split_glb(input);
    doc["extensionsUsed"] = json!([WEBP]);
    doc["extensionsRequired"] = json!([WEBP]);
    doc["images"] = json!([{"uri": "missing.webp"}]);
    doc["textures"] = json!([{"extensions": {(WEBP): {"source": 0}}}]);

    let report = rig::inspect_bytes(&encode_glb(&doc, bin.to_vec()));
    assert_eq!(report.validation.format_status, CheckStatus::Pass);
    assert_eq!(report.bound_nodes, baseline.bound_nodes);
    assert!(
        baseline
            .checks
            .iter()
            .any(|check| { check.id == "rig.weights" && check.status == CheckStatus::Pass })
    );
    assert_eq!(report.checks, baseline.checks);
}

#[test]
fn malformed_webp_and_unrelated_invalid_textures_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let cases: &[(&str, Change)] = &[
        ("unknown-required", |doc| {
            doc["extensionsUsed"] = json!([WEBP, "EXT_unknown_test"]);
            doc["extensionsRequired"] = doc["extensionsUsed"].clone();
        }),
        ("no-source", |doc| {
            doc["textures"].as_array_mut().unwrap().push(json!({}));
        }),
        ("out-of-bounds", |doc| {
            doc["textures"][0]["extensions"][WEBP]["source"] = json!(1);
        }),
        ("fractional", |doc| {
            doc["textures"][0]["extensions"][WEBP]["source"] = json!(0.5);
        }),
        ("negative", |doc| {
            doc["textures"][0]["extensions"][WEBP]["source"] = json!(-1);
        }),
        ("null", |doc| {
            doc["textures"][0]["extensions"][WEBP]["source"] = Value::Null;
        }),
        ("non-object-extension", |doc| {
            doc["textures"][0]["extensions"][WEBP] = json!([0]);
        }),
        ("null-extension", |doc| {
            doc["textures"][0]["extensions"][WEBP] = Value::Null;
        }),
        ("missing-extension-source", |doc| {
            doc["textures"][0]["extensions"][WEBP] = json!({});
        }),
        ("wrong-mime", |doc| {
            doc["images"][0]["mimeType"] = json!("image/png");
        }),
        ("no-resource", |doc| {
            doc["images"][0] = json!({"mimeType": "image/webp"});
        }),
        ("bad-buffer-view", |doc| {
            doc["images"][0]["bufferView"] = json!(2);
        }),
        ("buffer-view-outside-buffer", |doc| {
            doc["bufferViews"][1]["byteLength"] = json!(100);
        }),
        ("both-resources", |doc| {
            doc["images"][0]["uri"] = json!("texture.webp");
        }),
        ("uri-is-not-webp", |doc| {
            doc["images"][0] = json!({"uri": "image.png?name=image.webp"});
        }),
        ("data-uri-with-wrong-mime", |doc| {
            doc["images"][0] =
                json!({"uri": "data:image/png;base64,eA==", "mimeType": "image/webp"});
        }),
        ("malformed-data-uri", |doc| {
            doc["images"][0] = json!({"uri": "data:image/webp;base64"});
        }),
        ("malformed-external-uri", |doc| {
            doc["images"][0] = json!({"uri": "tiles/%GG"});
        }),
        ("undeclared-with-fallback", |doc| {
            doc.as_object_mut().unwrap().remove("extensionsUsed");
            doc.as_object_mut().unwrap().remove("extensionsRequired");
            doc["images"]
                .as_array_mut()
                .unwrap()
                .push(json!({"uri": "fallback.png"}));
            doc["textures"][0]["source"] = json!(1);
        }),
        ("required-but-not-used", |doc| {
            doc.as_object_mut().unwrap().remove("extensionsUsed");
        }),
        ("optional-without-fallback", |doc| {
            doc.as_object_mut().unwrap().remove("extensionsRequired");
        }),
        ("invalid-fallback-index", |doc| {
            doc["textures"][0]["source"] = json!(1);
        }),
        ("unused-declaration", |doc| {
            doc["textures"] = json!([]);
        }),
    ];
    for (name, change) in cases {
        let mut doc = webp_document();
        change(&mut doc);
        let path = dir.path().join(format!("{name}.glb"));
        fs::write(&path, glb(&doc)).unwrap();
        let (exit, validated) = invoke("validate", &path);
        assert_eq!(exit, Some(1), "{name}: {validated}");
        assert_eq!(validated["results"]["format_status"], "FAIL", "{name}");
        let (exit, inspected) = invoke("inspect", &path);
        assert_eq!(exit, Some(2), "{name}: {inspected}");
        assert_eq!(inspected["status"], "INVALID_INPUT", "{name}");
    }
}
