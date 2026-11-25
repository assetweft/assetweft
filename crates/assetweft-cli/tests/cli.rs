use assetweft_core::{
    application::Application,
    generation_spec as spec, validation as val,
    workflow::{Outcome, State, Step},
};
use serde_json::{Value, json};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn invoke(args: &[&OsStr]) -> (Output, Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_assetweft"))
        .args(args)
        .output()
        .unwrap();
    parse_output(out)
}

fn parse_output(out: Output) -> (Output, Value) {
    let v: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("stdout protocol: {e}: {:?}", out));
    assert_eq!(out.stdout.iter().filter(|b| **b == b'\n').count(), 1);
    assert_eq!(v["schema_version"], 1);
    assert!(v["command"].is_string());
    assert!(v["status"].is_string());
    assert!(v["errors"].is_array());
    assert!(v.get("results").is_some());
    (out, v)
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../assetweft-core/tests/fixtures/validation")
        .join(name)
}

#[test]
fn face_attachment_inspect_wire_and_input_limits() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("face.glb");
    let spec = dir.path().join("face.json");
    let write_glb = |document: &Value, bin: &[f32]| {
        let mut document = serde_json::to_vec(document).unwrap();
        document.resize(document.len().next_multiple_of(4), b' ');
        let mut glb = b"glTF".to_vec();
        glb.extend(2_u32.to_le_bytes());
        let bin_length = bin.len() as u32 * 4;
        glb.extend(
            (20 + document.len() as u32 + if bin.is_empty() { 0 } else { 8 + bin_length })
                .to_le_bytes(),
        );
        glb.extend((document.len() as u32).to_le_bytes());
        glb.extend(b"JSON");
        glb.extend(document);
        if !bin.is_empty() {
            glb.extend(bin_length.to_le_bytes());
            glb.extend(b"BIN\0");
            glb.extend(bin.iter().flat_map(|sample| sample.to_le_bytes()));
        }
        fs::write(&file, glb).unwrap();
    };
    let mut document = json!({"asset":{"version":"2.0"},
        "nodes":[{"name":"socket","children":[1]},{"name":"part"}]});
    write_glb(&document, &[]);
    let mut contract = json!({
        "face_mode":"not-applicable", "expected_expressions":[],
        "rigid_attachments":[{"socket_node":"socket","attachment_node":"part",
            "expected_offset_m":[0,0,0],"max_drift_m":0.0005,"lock_mode":"BYTE"}]
    });
    let args = [
        OsStr::new("--call-id"),
        OsStr::new("face-1"),
        OsStr::new("face-attachment-inspect"),
        file.as_os_str(),
        OsStr::new("--spec"),
        spec.as_os_str(),
    ];
    fs::write(&spec, contract.to_string()).unwrap();
    let (out, pass) = invoke(&args);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(pass["command"], "face-attachment-inspect");
    assert_eq!(pass["call_id"], "face-1");
    assert_eq!(pass["status"], "PASS");
    assert_eq!(pass["results"]["facial_status"], "INAPPLICABLE");
    assert_eq!(pass["results"]["visual_engine_status"], "NOT_RUN");
    assert_eq!(out.stdout, invoke(&args).0.stdout);

    contract["rigid_attachments"][0]["expected_offset_m"] = json!([0.001, 0, 0]);
    fs::write(&spec, contract.to_string()).unwrap();
    let (out, drift) = invoke(&args);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(drift["results"]["facial_status"], "INAPPLICABLE");
    assert_eq!(drift["results"]["attachment_results"][0]["status"], "FAIL");
    contract["rigid_attachments"][0]["expected_offset_m"] = json!([0, 0, 0]);

    contract["face_mode"] = json!("required");
    fs::write(&spec, contract.to_string()).unwrap();
    let (out, fail) = invoke(&args);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(fail["results"]["facial_status"], "FAIL");

    for (limit, field) in [("1", "file"), ("150", "spec"), ("0", "arguments")] {
        let (out, invalid) = invoke(
            &[
                args.as_slice(),
                &[OsStr::new("--max-bytes"), OsStr::new(limit)],
            ]
            .concat(),
        );
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(invalid["command"], "face-attachment-inspect");
        assert_eq!(invalid["errors"][0]["field_path"], field);
    }
    fs::write(&spec, contract.to_string().replace("0.0005", "NaN")).unwrap();
    assert_eq!(invoke(&args).0.status.code(), Some(2));
    fs::write(&spec, contract.to_string()).unwrap();
    fs::write(&file, b"glTF").unwrap();
    assert_eq!(invoke(&args).0.status.code(), Some(2));

    document["nodes"]
        .as_array_mut()
        .unwrap()
        .extend([json!({"mesh":0}), json!({"name":"jaw"})]);
    document["meshes"] = json!([{"extras":{"targetNames":["blink","jaw_open"]},
        "primitives":[{"attributes":{"POSITION":0},"targets":[{"POSITION":1},{"POSITION":1}]}]}]);
    document["buffers"] = json!([{"byteLength":120}]);
    document["bufferViews"] = json!([
        {"buffer":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":36},
        {"buffer":0,"byteOffset":72,"byteLength":8},{"buffer":0,"byteOffset":80,"byteLength":16},
        {"buffer":0,"byteOffset":96,"byteLength":24}
    ]);
    document["accessors"] = json!([
        {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
        {"bufferView":1,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[0,0,0.1]},
        {"bufferView":2,"componentType":5126,"count":2,"type":"SCALAR","min":[0],"max":[1]},
        {"bufferView":3,"componentType":5126,"count":4,"type":"SCALAR"},
        {"bufferView":4,"componentType":5126,"count":2,"type":"VEC3"}
    ]);
    document["animations"] = json!([{"channels":[
        {"sampler":0,"target":{"node":2,"path":"weights"}},
        {"sampler":1,"target":{"node":3,"path":"translation"}}],
        "samplers":[{"input":2,"output":3},{"input":2,"output":4}]}]);
    let bin = [
        0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0., 0.1, 0., 0., 0., 0., 0., 0., 0., 1., 0., 0.,
        1., 1., 0., 0., 0., 0., 0.1, 0.,
    ];
    write_glb(&document, &bin);
    contract["expected_expressions"] = json!(["blink", "jaw_open"]);
    contract["eye_jaw_nodes"] = json!({"jaw":"jaw"});
    fs::write(&spec, contract.to_string()).unwrap();
    let (out, face) = invoke(&args);
    assert_eq!(out.status.code(), Some(0), "{face}");
    assert_eq!(face["results"]["facial_status"], "PASS");
    assert_eq!(face["results"]["eye_jaw_playable"], true);
    assert_eq!(
        face["results"]["matched_expressions"],
        json!(["blink", "jaw_open"])
    );
    document["animations"] = json!([]);
    write_glb(&document, &bin);
    let (out, static_face) = invoke(&args);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(static_face["results"]["eye_jaw_playable"], false);
}

#[test]
fn audio_inspect_reports_deterministic_measurements_and_contract_failures() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("loop.wav");
    let spec_file = dir.path().join("audio.json");
    let wav = b"RIFF\x2c\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x04\0\0\0\x08\0\0\0\x02\0\x10\0data\x08\0\0\0\0\0\0\x40\0\xc0\0\0";
    fs::write(&file, wav).unwrap();
    let mut spec = json!({
        "expected_channels":1, "expected_sample_rate_hz":4, "min_duration_ms":1000,
        "max_duration_ms":1000, "transcript_text":"loop音",
        "loop_region":{"start_sample":0,"end_sample":4,"max_step_delta":0,"max_zero_crossing_offset":0}
    });
    fs::write(&spec_file, spec.to_string()).unwrap();
    let args = [
        OsStr::new("--call-id"),
        OsStr::new("audio-1"),
        OsStr::new("audio-inspect"),
        file.as_os_str(),
        OsStr::new("--spec"),
        spec_file.as_os_str(),
    ];
    let (out, pass) = invoke(&args);
    assert_eq!(pass["command"], "audio-inspect");
    assert_eq!(pass["call_id"], "audio-1");
    assert_eq!(pass["status"], "PASS");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, invoke(&args).0.stdout);
    assert_eq!(pass["results"]["asr_playback_status"], "NOT_RUN");
    assert_eq!(pass["results"]["chars_per_sec"], 5.0);
    assert_eq!(pass["results"]["transcript_text"], "loop音");

    spec["expected_channels"] = json!(2);
    spec["min_duration_ms"] = json!(1001);
    spec["max_duration_ms"] = json!(1002);
    spec["loop_region"]["end_sample"] = json!(3);
    fs::write(&spec_file, spec.to_string()).unwrap();
    let (out, fail) = invoke(&args);
    assert_eq!(fail["status"], "FAIL");
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        fail["results"]["violations"],
        json!(["ChannelMismatch", "DurationMismatch", "LoopBoundaryClick"])
    );

    let assert_invalid = |args: &[&OsStr], path: &str| {
        let (out, invalid) = invoke(args);
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(invalid["command"], "audio-inspect");
        assert_eq!(invalid["status"], "INVALID_INPUT");
        assert_eq!(invalid["errors"][0]["field_path"], path);
    };
    for (limit, field) in [("51", "file"), ("52", "spec"), ("0", "arguments")] {
        let bounded = [
            args.as_slice(),
            &[OsStr::new("--max-bytes"), OsStr::new(limit)],
        ]
        .concat();
        assert_invalid(&bounded, field);
    }
    for bad_spec in [
        "[1,4,1000,1000,null,null]",
        r#"{"loop_region":[0,4,0,0]}"#,
        r#"{"expected_channels":1,"expected_channels":2}"#,
        r#"{"extra":0}"#,
        r#"{"loop_region":{"start_sample":0,"end_sample":4,"max_step_delta":0,"max_zero_crossing_offset":0,"extra":0}}"#,
    ] {
        fs::write(&spec_file, bad_spec).unwrap();
        assert_invalid(&args, "spec");
    }
    fs::write(&file, &wav[..wav.len() - 1]).unwrap();
    assert_invalid(&args[..4], "file");
    let mut unsupported = *wav;
    unsupported[20] = 6;
    fs::write(&file, unsupported).unwrap();
    assert_invalid(&args[..4], "file");
    let mut float = *wav;
    float[20] = 3;
    float[28] = 16;
    float[32] = 4;
    float[34] = 32;
    float[44..48].copy_from_slice(&f32::NAN.to_le_bytes());
    fs::write(&file, float).unwrap();
    assert_invalid(&args[..4], "file");
}
fn static_kit_glb(closed: bool, double_sided: bool) -> Vec<u8> {
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
    let document = json!({
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
fn static_kit_inspect_reports_contract_breaches_and_bounded_input_errors() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("static kit.glb");
    let spec_path = dir.path().join("kit spec.json");
    let mut spec = json!({
        "kind":"architecture","expected_unit_scale":1,"scale_tolerance":0.001,
        "sockets":[{"name":"east","position":[3,0,0],"seam_tolerance_m":0}],
        "allow_non_watertight_foliage":false,
        "budget_before":{"vertex_count":24,"triangle_count":24,"texture_bytes":128}
    });
    let args = [
        OsStr::new("--call-id"),
        OsStr::new("static-1"),
        OsStr::new("static-kit-inspect"),
        file.as_os_str(),
        OsStr::new("--spec"),
        spec_path.as_os_str(),
    ];
    let run = |spec: &Value, code, status| {
        fs::write(&spec_path, spec.to_string()).unwrap();
        let (out, value) = invoke(&args);
        assert_eq!(out.status.code(), Some(code));
        assert_eq!(value["command"], "static-kit-inspect");
        assert_eq!(value["call_id"], "static-1");
        assert_eq!(value["status"], status);
        assert_eq!(out.stdout, invoke(&args).0.stdout);
        value
    };
    fs::write(&file, static_kit_glb(true, false)).unwrap();
    let pass = run(&spec, 0, "PASS");
    assert_eq!(pass["results"]["scope"], "inspected_static_kit");
    assert_eq!(
        pass["results"]["optimization_record"]["delta_triangles"],
        -12
    );
    assert_eq!(pass["results"]["optimization_record"]["delta_bytes"], -128);
    assert_eq!(pass["results"]["visual_quality_status"], "NOT_RUN");
    spec["budget_before"]["texture_bytes"] = json!(u64::MAX);
    assert_eq!(
        run(&spec, 0, "PASS")["results"]["optimization_record"]["delta_bytes"],
        "-18446744073709551615"
    );

    spec["expected_unit_scale"] = json!(2);
    let fail = run(&spec, 1, "FAIL");
    assert_eq!(fail["results"]["triangle_count"], 12);
    assert_eq!(fail["results"]["violations"][0]["field"], "scale");
    spec["expected_unit_scale"] = json!(1);
    spec["sockets"][0]["position"] = json!([3.01, 0, 0]);
    assert_eq!(
        run(&spec, 1, "FAIL")["results"]["violations"][0]["field"],
        "seam"
    );
    spec["sockets"][0]["position"] = json!([3, 0, 0]);

    fs::write(&file, static_kit_glb(false, true)).unwrap();
    assert_eq!(
        run(&spec, 1, "FAIL")["results"]["violations"][0]["field"],
        "watertight"
    );
    spec["kind"] = json!("foliage");
    spec["allow_non_watertight_foliage"] = json!(true);
    let foliage = run(&spec, 0, "PASS");
    assert_eq!(foliage["results"]["double_sided_material_count"], 1);
    assert_eq!(foliage["results"]["alpha_modes"], json!(["MASK"]));
    assert_eq!(foliage["results"]["unclosed_boundary_edges"], 4);

    let invalid_spec = spec
        .to_string()
        .replace("\"expected_unit_scale\":1", "\"expected_unit_scale\":NaN");
    fs::write(&spec_path, invalid_spec).unwrap();
    let (out, invalid) = invoke(&args);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(invalid["status"], "INVALID_INPUT");

    spec["expected_unit_scale"] = json!(-1);
    run(&spec, 2, "INVALID_INPUT");
    spec["expected_unit_scale"] = json!(1);
    spec["kind"] = json!("forest");
    run(&spec, 2, "INVALID_INPUT");
    spec["kind"] = json!("foliage");
    let (out, invalid) =
        invoke(&[&args[..], &[OsStr::new("--max-bytes"), OsStr::new("0")]].concat());
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(invalid["status"], "INVALID_INPUT");
    assert_eq!(invalid["command"], "static-kit-inspect");
    let (out, invalid) =
        invoke(&[&args[..], &[OsStr::new("--max-bytes"), OsStr::new("12")]].concat());
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(invalid["status"], "INVALID_INPUT");
    fs::write(&file, b"not a GLB").unwrap();
    run(&spec, 2, "INVALID_INPUT");
}

fn cases() -> Value {
    serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../assetweft-core/tests/fixtures/generation_spec/cases.json"),
        )
        .unwrap(),
    )
    .unwrap()
}
fn plan(dir: &Path, request: &Value, context: &Value) -> (Output, Value) {
    let req = dir.join("请求 file.json");
    let ctx = dir.join("合同 context.json");
    let rb = serde_json::to_vec(request).unwrap();
    let cb = serde_json::to_vec(context).unwrap();
    fs::write(&req, &rb).unwrap();
    fs::write(&ctx, &cb).unwrap();
    let args = [
        OsStr::new("--call-id"),
        OsStr::new("重复-id"),
        OsStr::new("plan"),
        OsStr::new("--request"),
        req.as_os_str(),
        OsStr::new("--trusted-context"),
        ctx.as_os_str(),
    ];
    let (out, v) = invoke(&args);
    let (again, _) = invoke(&args);
    assert_eq!(out.stdout, again.stdout);
    assert_eq!(out.status.code(), again.status.code());
    assert!(out.stderr.is_empty());
    assert_eq!(v["call_id"], "重复-id");
    assert_eq!(v["command"], "plan");
    assert_eq!(fs::read(&req).unwrap(), rb);
    assert_eq!(fs::read(&ctx).unwrap(), cb);
    (out, v)
}
#[test]
fn glb_structure_roles_paths_and_replay() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("中文 mesh file.glb");
    let bytes = fs::read(fixture("mesh_only.glb")).unwrap();
    fs::write(&path, &bytes).unwrap();
    for command in ["inspect", "validate"] {
        let args = [
            OsStr::new(command),
            path.as_os_str(),
            OsStr::new("--call-id"),
            OsStr::new("stable-01"),
        ];
        let (out, v) = invoke(&args);
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert_eq!(v["command"], command);
        assert_eq!(v["call_id"], "stable-01");
        assert_eq!(v["status"], "PASS");
        assert!(out.stderr.is_empty());
        let (again, _) = invoke(&args);
        assert_eq!(out.stdout, again.stdout);
        if command == "inspect" {
            assert!(
                !v["results"]["geometry"]["primitives"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(v["results"]["geometry"]["input_units"], "UNKNOWN");
        } else {
            assert_eq!(v["results"]["source"]["verified_by_checker"], false);
            assert!(!v["results"]["not_run"].as_array().unwrap().is_empty());
        }
    }
    let (out, v) = invoke(&[
        OsStr::new("validate"),
        path.as_os_str(),
        OsStr::new("--role"),
        OsStr::new("rigged-character"),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(v["results"]["role"]["missing"], json!(["skin"]));
    assert_eq!(fs::read(&path).unwrap(), bytes);
}
#[test]
fn malformed_plain_json_role_and_bounded_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain.json");
    fs::write(&plain, b"{\"asset\":{\"version\":\"2.0\"}}").unwrap();
    for path in [fixture("truncated.glb"), fixture("not_a_glb.bin"), plain] {
        for command in ["inspect", "validate"] {
            let (out, v) = invoke(&[OsStr::new(command), path.as_os_str()]);
            assert_eq!(
                out.status.code(),
                Some(if command == "inspect" { 2 } else { 1 }),
                "{v}"
            );
            assert!(!v["errors"].as_array().unwrap().is_empty());
        }
    }
    for command in ["inspect", "validate"] {
        let (out, v) = invoke(&[
            OsStr::new(command),
            fixture("mesh_only.glb").as_os_str(),
            OsStr::new("--max-bytes"),
            OsStr::new("8"),
        ]);
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(v["errors"][0]["field_path"], "file");
    }
}
#[test]
fn plans_reuse_all_core_cases_and_explicit_negatives() {
    let dir = tempfile::tempdir().unwrap();
    let all = cases();
    for case in all["cases"].as_array().unwrap() {
        let req: spec::GenerationSpecRequest =
            serde_json::from_value(case["request"].clone()).unwrap();
        let ctx: spec::TrustedContext = serde_json::from_value(case["context"].clone()).unwrap();
        let core = spec::validate(&req, &ctx);
        let (out, v) = plan(dir.path(), &case["request"], &case["context"]);
        assert_eq!(
            out.status.success(),
            core.is_declaration_consistent(),
            "{}: {v}",
            case["id"]
        );
        assert_eq!(v["results"]["execution"], "NOT_RUN");
        assert_eq!(v["results"]["authenticity"], "NOT_VERIFIED");
        assert_eq!(v["results"]["declaration_status"], core.status);
        for issue in core.issues {
            assert!(
                v["errors"].as_array().unwrap().iter().any(
                    |e| e["field_path"] == issue.field_path && e["code"] == issue.code.as_str()
                )
            );
        }
    }
    let c = &all["cases"][0];
    let (out, v) = plan(dir.path(), &c["request"], &c["context"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(v["status"], "DECLARATION_CONSISTENT");
    assert_eq!(v["results"]["recipe"]["budget"]["cost_value"], Value::Null);
    let mut req = c["request"].clone();
    req["locked_components"] = json!([]);
    let (out, v) = plan(dir.path(), &req, &c["context"]);
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(v["status"], "LOCK_VIOLATION");
    for (field, value, code) in [
        ("base_version", Value::Null, 3),
        ("base_version", json!("   "), 3),
        (
            "budget",
            json!({"max_calls":0,"cost_status":"unknown","cost_value":0}),
            2,
        ),
    ] {
        let mut req = c["request"].clone();
        req[field] = value;
        let (out, _) = plan(dir.path(), &req, &c["context"]);
        assert_eq!(out.status.code(), Some(code), "{field}");
    }
    let (out, v) = plan(dir.path(), &c["request"], &json!({}));
    assert!(!out.status.success());
    assert!(!v["errors"].as_array().unwrap().is_empty());
}
#[test]
fn json_errors_limits_and_cli_protocol() {
    for (args, code, status) in [
        (vec!["--help"], 0, "HELP"),
        (vec!["--version"], 0, "VERSION"),
        (vec!["--unknown"], 2, "INVALID_INPUT"),
        (vec!["plan"], 2, "INVALID_INPUT"),
        (vec!["validate", "x", "--role", "bogus"], 2, "INVALID_INPUT"),
        (
            vec!["--max-bytes", "16777217", "inspect", "x"],
            2,
            "INVALID_INPUT",
        ),
    ] {
        let os: Vec<_> = args.iter().map(OsStr::new).collect();
        let (out, v) = invoke(&os);
        assert_eq!(out.status.code(), Some(code));
        assert_eq!(v["status"], status);
        assert_eq!(out.stderr.is_empty(), code == 0);
    }
    let dir = tempfile::tempdir().unwrap();
    let req = dir.path().join("bad.json");
    let ctx = dir.path().join("context.json");
    fs::write(&req, b"{broken").unwrap();
    fs::write(&ctx, b"{}").unwrap();
    let args = [
        OsStr::new("plan"),
        OsStr::new("--request"),
        req.as_os_str(),
        OsStr::new("--trusted-context"),
        ctx.as_os_str(),
    ];
    let (out, v) = invoke(&args);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(v["errors"][0]["field_path"], "request");
    fs::write(&req, b"{}                  ").unwrap();
    let mut limited = args.to_vec();
    limited.extend([OsStr::new("--max-bytes"), OsStr::new("8")]);
    let (out, _) = invoke(&limited);
    assert_eq!(out.status.code(), Some(2));
    fs::write(&req, b"{}").unwrap();
    fs::write(&ctx, b"{broken").unwrap();
    let (out, v) = invoke(&args);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(v["errors"][0]["field_path"], "trusted_context");
    let (out, v) = invoke(&[
        OsStr::new("--call-id"),
        OsStr::new("help-id"),
        OsStr::new("--help"),
    ]);
    assert!(out.status.success());
    assert_eq!(v["call_id"], "help-id");
}
#[test]
fn validation_matches_core() {
    let path = fixture("mesh_only.glb");
    let core = val::validate_file(
        &path,
        val::RoleRequirement::AnimatedCharacter,
        val::SourceDeclaration::declared(val::SourceClass::Unknown),
        val::ValidationLimits::default(),
    )
    .unwrap();
    let (_, v) = invoke(&[
        OsStr::new("validate"),
        path.as_os_str(),
        OsStr::new("--role"),
        OsStr::new("animated-character"),
    ]);
    assert_eq!(v["results"]["file_sha256"], core.file_sha256);
    assert_eq!(v["results"]["role"]["status"], core.role.status.as_str());
    assert_eq!(
        v["results"]["checks"].as_array().unwrap().len(),
        core.checks.len()
    );
}

#[test]
fn call_id_survives_help_and_version() {
    for flag in ["--help", "--version"] {
        let (out, v) = invoke(&[
            OsStr::new(flag),
            OsStr::new("--call-id"),
            OsStr::new("late-id"),
        ]);
        assert!(out.status.success());
        assert_eq!(v["call_id"], "late-id");
    }
    let (out, v) = invoke(&[
        OsStr::new("--call-id"),
        OsStr::new("error-id"),
        OsStr::new("--bogus"),
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(v["call_id"], "error-id");
}

#[test]
fn semantic_identity_plan_cannot_bypass_core() {
    let dir = tempfile::tempdir().unwrap();
    let all = cases();
    let c = &all["cases"][0];
    let (out, v) = plan(dir.path(), &c["request"], &c["context"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(v["results"]["execution"], "NOT_RUN");
    for (value, code) in [
        (Value::Null, "INPUT_INCOMPLETE"),
        (json!("other"), "LOCK_VIOLATION"),
    ] {
        let mut req = c["request"].clone();
        req["locked_components"][2]["canonicalization_id"] = value;
        let (out, v) = plan(dir.path(), &req, &c["context"]);
        assert_eq!(
            out.status.code(),
            Some(if code == "INPUT_INCOMPLETE" { 3 } else { 4 })
        );
        assert!(
            v["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["code"] == code
                    && e["field_path"] == "locked_components[2].canonicalization_id")
        );
        assert_eq!(v["results"]["execution"], "NOT_RUN");
    }
}

#[test]
fn plan_rejects_duplicate_trusted_lock_roles() {
    let dir = tempfile::tempdir().unwrap();
    let all = cases();
    let c = &all["cases"][0];
    for change in [
        Some("canonicalization_id"),
        Some("canonicalization_version"),
        None,
    ] {
        for reverse in [false, true] {
            let mut context = c["context"].clone();
            let locks = context["declared_locks"].as_array_mut().unwrap();
            let i = locks.len();
            let mut duplicate = locks[2].clone();
            if let Some(field) = change {
                duplicate[field] = json!("other");
            }
            locks.push(duplicate);
            if reverse {
                locks.swap(2, i);
            }
            let (out, v) = plan(dir.path(), &c["request"], &context);
            let field = format!("context.declared_locks[{i}].role");
            assert_eq!(out.status.code(), Some(2), "{change:?} {reverse}: {v}");
            assert!(
                v["errors"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|e| e["code"] == "INVALID_INPUT" && e["field_path"] == field),
                "{change:?} {reverse}: {v}"
            );
            assert_eq!(v["results"]["execution"], "NOT_RUN");
        }
    }
}

// POSIX shutdown(SHUT_WR) disables writes even while another process holds the peer.
#[cfg(unix)]
fn broken_output() -> std::process::Stdio {
    use std::{
        io::Write,
        net::Shutdown,
        os::{fd::OwnedFd, unix::net::UnixStream},
    };
    let (mut writer, reader) = UnixStream::pair().unwrap();
    writer.shutdown(Shutdown::Write).unwrap();
    assert!(writer.write_all(b"must not write").is_err());
    drop(reader);
    std::process::Stdio::from(OwnedFd::from(writer))
}

// Windows FILE_SHARE_DELETE keeps the inherited read-only handle valid after the temp path is removed.
#[cfg(windows)]
fn broken_output() -> std::process::Stdio {
    use std::io::Write;
    let temp = tempfile::NamedTempFile::new().unwrap();
    let mut readonly = fs::File::open(temp.path()).unwrap();
    assert!(readonly.write_all(b"must not write").is_err());
    std::process::Stdio::from(readonly)
}

#[cfg(any(unix, windows))]
#[test]
fn stderr_failure_preserves_json_protocol() {
    let path = fixture("mesh_only.glb");
    let invalid = fixture("truncated.glb");
    for args in [
        vec![
            OsStr::new("--call-id"),
            OsStr::new("error-id"),
            OsStr::new("--unknown"),
        ],
        vec![
            OsStr::new("--call-id"),
            OsStr::new("help-id"),
            OsStr::new("--help"),
        ],
        vec![
            OsStr::new("--call-id"),
            OsStr::new("file-id"),
            OsStr::new("inspect"),
            path.as_os_str(),
        ],
        vec![
            OsStr::new("--call-id"),
            OsStr::new("invalid-id"),
            OsStr::new("inspect"),
            invalid.as_os_str(),
        ],
    ] {
        let ordinary = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(&args)
            .output()
            .unwrap();
        let failed_stderr = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(&args)
            .stderr(broken_output())
            .output()
            .unwrap();
        assert_eq!(
            failed_stderr.status.code(),
            ordinary.status.code(),
            "{args:?}"
        );
        assert_eq!(failed_stderr.stdout, ordinary.stdout, "{args:?}");
        let value: Value = serde_json::from_slice(&failed_stderr.stdout).unwrap();
        assert_eq!(value["call_id"], args[1].to_str().unwrap());
        assert_eq!(
            failed_stderr.stdout.iter().filter(|b| **b == b'\n').count(),
            1
        );
    }
}

#[cfg(any(unix, windows))]
#[test]
fn stdout_and_stderr_failure_preserves_io_exit() {
    for args in [vec!["--help"], vec!["--unknown"]] {
        let status = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(args)
            .stdout(broken_output())
            .stderr(broken_output())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(6));
    }
}

fn step(id: &str, dependencies: &[&str]) -> Step {
    Step {
        id: id.into(),
        dependencies: dependencies.iter().map(|d| (*d).into()).collect(),
    }
}
fn workflow_args<'a>(action: &'a str, db: &'a OsStr, run: &'a OsStr) -> Vec<&'a OsStr> {
    vec![
        OsStr::new("workflow"),
        OsStr::new(action),
        OsStr::new("--db"),
        db,
        OsStr::new("--run-id"),
        run,
    ]
}
fn seed_workflow(dir: &Path) -> PathBuf {
    let path = dir.join("库 状态 db.sqlite");
    let mut app = Application::open(&path).unwrap();
    app.initialize_library().unwrap();
    app.create_workflow(
        "运行-α",
        &[
            step("准备", &[]),
            step("生成", &["准备"]),
            step("校验", &["生成"]),
        ],
    )
    .unwrap();
    assert!(
        app.run_workflow_step("运行-α", |id| {
            assert_eq!(id, "准备");
            Outcome::Success {
                raw: "raw:准备\nline2".into(),
                evidence: Some("artifact:准备".into()),
            }
        })
        .unwrap()
    );
    assert_eq!(
        app.claim_workflow_step("运行-α").unwrap().as_deref(),
        Some("生成")
    );
    app.close().unwrap();
    path
}

#[test]
fn workflow_open_lock_reports_internal_and_does_not_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let db = seed_workflow(dir.path());
    let (blocker, _) = assetweft_core::sqlite::open_and_configure(&db).unwrap();
    blocker
        .execute_batch("PRAGMA journal_mode=DELETE; BEGIN EXCLUSIVE")
        .unwrap();
    let run = OsStr::new("运行-α");
    for action in ["status", "cancel"] {
        let mut args = vec![OsStr::new("--call-id"), OsStr::new("locked-open")];
        args.extend(workflow_args(action, db.as_os_str(), run));
        let (out, v) = invoke(&args);
        assert!(out.stderr.is_empty());
        assert_eq!(out.status.code(), Some(6), "{v}");
        assert_eq!(v["status"], "INTERNAL");
        assert_eq!(v["call_id"], "locked-open");
        assert_eq!(v["errors"][0]["code"], "INTERNAL");
        assert_eq!(v["errors"][0]["field_path"], "db");
        assert!(v["errors"][0]["message"].as_str().unwrap().contains("BUSY"));
        assert!(v["results"].is_null());
    }
    blocker.execute_batch("ROLLBACK").unwrap();
    let (out, v) = invoke(&workflow_args("status", db.as_os_str(), run));
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert!(
        !Application::open_read_only(&db)
            .unwrap()
            .workflow_cancelled("运行-α")
            .unwrap()
    );
    let (out, v) = invoke(&workflow_args("cancel", db.as_os_str(), run));
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert_eq!(v["results"]["cancellation_requested"], true);
}

#[test]
fn existing_database_open_lock_reports_internal_and_leaves_database_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let (db, app, contract, _, _) = candidate_write_fixture(dir.path());
    app.close().unwrap();
    let allowance = dir.path().join("allowance.json");
    fs::write(
        &allowance,
        json!({"id":"locked-budget","unit":"Token","capacity":1,
            "approval":"ExplicitBounded","source":"TEST_FIXTURE"})
        .to_string(),
    )
    .unwrap();
    let version = dir.path().join("version.json");
    fs::write(
        &version,
        catalog_write_wire("locked-version", "UNKNOWN", false).to_string(),
    )
    .unwrap();
    let commands = [
        candidate_write_args("initialize", &db, &contract),
        candidate_args("show", db.as_os_str(), OsStr::new("absent")),
        budget_args("approve", db.as_os_str(), allowance.as_os_str()),
        catalog_write_args("register-version", &db, &version),
        catalog_args("version", db.as_os_str(), OsStr::new("base")),
        vec![
            OsStr::new("catalog"),
            OsStr::new("feedback-get"),
            OsStr::new("--db"),
            db.as_os_str(),
            OsStr::new("--asset-id"),
            OsStr::new("character"),
            OsStr::new("--version-id"),
            OsStr::new("base"),
            OsStr::new("--feedback-id"),
            OsStr::new("absent"),
        ],
    ];
    let (blocker, _) = assetweft_core::sqlite::open_and_configure(&db).unwrap();
    blocker.execute_batch("PRAGMA journal_mode=DELETE").unwrap();
    let before = sha256(&fs::read(&db).unwrap());
    blocker.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let locked_results: Vec<_> = commands
        .iter()
        .map(|command| {
            let mut args = vec![OsStr::new("--call-id"), OsStr::new("locked-open")];
            args.extend(command);
            invoke(&args)
        })
        .collect();
    blocker.execute_batch("ROLLBACK").unwrap();
    assert_eq!(sha256(&fs::read(&db).unwrap()), before);
    for (out, v) in &locked_results {
        assert!(out.stderr.is_empty());
        assert_eq!(out.status.code(), Some(6), "{locked_results:?}");
        assert_eq!(v["status"], "INTERNAL");
        assert_eq!(v["call_id"], "locked-open");
        assert_eq!(v["errors"][0]["code"], "INTERNAL");
        assert_eq!(v["errors"][0]["field_path"], "db");
        assert!(v["errors"][0]["message"].as_str().unwrap().contains("BUSY"));
        assert!(v["results"].is_null());
    }
    for command in &commands {
        let (out, v) = invoke(command);
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert_eq!(v["status"], "PASS");
    }
}

#[test]
fn workflow_status_projects_persisted_state_deterministically() {
    let dir = tempfile::tempdir().unwrap();
    let db = seed_workflow(dir.path());
    let run = OsStr::new("运行-α");
    let mut args = vec![OsStr::new("--call-id"), OsStr::new("wf-状态")];
    args.extend(workflow_args("status", db.as_os_str(), run));
    let (out, v) = invoke(&args);
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert_eq!(v["command"], "workflow status");
    assert_eq!(v["call_id"], "wf-状态");
    assert_eq!(v["status"], "PASS");
    assert_eq!(v["errors"], json!([]));
    assert_eq!(v["results"]["run_id"], "运行-α");
    assert_eq!(v["results"]["cancellation_requested"], false);
    let steps = v["results"]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 3);
    let ids: Vec<_> = steps.iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["准备", "生成", "校验"]);
    assert_eq!(steps[0]["state"], "succeeded");
    assert_eq!(steps[0]["raw"], "raw:准备\nline2");
    assert_eq!(steps[0]["evidence"], "artifact:准备");
    assert_eq!(steps[1]["state"], "running");
    assert_eq!(steps[1]["raw"], Value::Null);
    assert_eq!(steps[1]["evidence"], Value::Null);
    assert_eq!(steps[2]["state"], "pending");
    assert_eq!(steps[2]["raw"], Value::Null);
    assert_eq!(steps[2]["evidence"], Value::Null);
    assert!(out.stderr.is_empty());
    let (again, _) = invoke(&args);
    assert_eq!(out.stdout, again.stdout);
    assert_eq!(out.status.code(), again.status.code());
    let app = Application::open(&db).unwrap();
    assert_eq!(
        app.workflow_steps("运行-α").unwrap()[0].state,
        State::Succeeded
    );
    assert!(!app.workflow_cancelled("运行-α").unwrap());
    drop(app);
    let mut app = Application::open(&db).unwrap();
    app.create_workflow("失败-运行", &[step("a", &[]), step("b", &["a"])])
        .unwrap();
    assert!(
        app.run_workflow_step("失败-运行", |_| Outcome::Failure {
            raw: "err:raw".into(),
            evidence: None,
        })
        .unwrap()
    );
    app.finish_workflow_step(
        "运行-α",
        "生成",
        &Outcome::Success {
            raw: "raw:生成".into(),
            evidence: None,
        },
    )
    .unwrap();
    app.close().unwrap();
    let (out2, v2) = invoke(&args);
    assert_eq!(out2.status.code(), Some(0));
    assert_eq!(v2["results"]["steps"][1]["state"], "succeeded");
    assert_eq!(v2["results"]["steps"][1]["raw"], "raw:生成");
    assert_eq!(v2["results"]["steps"][1]["evidence"], Value::Null);
    assert_eq!(v2["results"]["steps"][2]["state"], "pending");
    let mut failed = vec![OsStr::new("--call-id"), OsStr::new("wf-failed")];
    failed.extend(workflow_args(
        "status",
        db.as_os_str(),
        OsStr::new("失败-运行"),
    ));
    let (out3, v3) = invoke(&failed);
    assert_eq!(out3.status.code(), Some(0));
    assert_eq!(v3["results"]["steps"][0]["state"], "failed");
    assert_eq!(v3["results"]["steps"][0]["raw"], "err:raw");
    assert_eq!(v3["results"]["steps"][0]["evidence"], Value::Null);
    assert_eq!(v3["results"]["steps"][1]["state"], "pending");
}

#[test]
fn workflow_cancel_persists_request_and_keeps_results() {
    let dir = tempfile::tempdir().unwrap();
    let db = seed_workflow(dir.path());
    let args = workflow_args("cancel", db.as_os_str(), OsStr::new("运行-α"));
    let (out, v) = invoke(&args);
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert_eq!(v["command"], "workflow cancel");
    assert_eq!(v["status"], "PASS");
    assert_eq!(v["errors"], json!([]));
    assert_eq!(v["results"]["run_id"], "运行-α");
    assert_eq!(v["results"]["cancellation_requested"], true);
    assert_eq!(v["results"]["scope"], "persisted_cancellation_request_only");
    let rendered = v.to_string();
    for overclaim in ["refund", "stopped", "released", "terminated"] {
        assert!(!rendered.contains(overclaim), "{rendered}");
    }
    let (again, v_again) = invoke(&args);
    assert_eq!(out.stdout, again.stdout);
    assert_eq!(v_again["results"]["cancellation_requested"], true);
    let mut app = Application::open(&db).unwrap();
    assert!(app.workflow_cancelled("运行-α").unwrap());
    let steps = app.workflow_steps("运行-α").unwrap();
    assert_eq!(steps[0].state, State::Succeeded);
    assert_eq!(
        steps[0].outcome,
        Some(Outcome::Success {
            raw: "raw:准备\nline2".into(),
            evidence: Some("artifact:准备".into()),
        })
    );
    assert_eq!(steps[1].state, State::Running);
    assert_eq!(steps[2].state, State::Pending);
    assert_eq!(app.claim_workflow_step("运行-α").unwrap(), None);
    app.close().unwrap();
    let mut app = Application::open(&db).unwrap();
    assert_eq!(app.claim_workflow_step("运行-α").unwrap(), None);
    drop(app);
    let (out_status, v_status) = invoke(&workflow_args(
        "status",
        db.as_os_str(),
        OsStr::new("运行-α"),
    ));
    assert_eq!(out_status.status.code(), Some(0));
    assert_eq!(v_status["results"]["cancellation_requested"], true);
    assert_eq!(v_status["results"]["steps"][1]["state"], "running");
    assert_eq!(v_status["results"]["steps"][2]["state"], "pending");
}

#[test]
fn workflow_errors_are_truthful_and_never_create_the_db() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("不存在 库.sqlite");
    let run = OsStr::new("运行-α");
    let mut args = vec![OsStr::new("--call-id"), OsStr::new("missing-db")];
    args.extend(workflow_args("status", missing.as_os_str(), run));
    let (out, v) = invoke(&args);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(v["status"], "INVALID_INPUT");
    assert_eq!(v["call_id"], "missing-db");
    assert_eq!(v["errors"][0]["field_path"], "db");
    assert_eq!(v["results"], Value::Null);
    assert!(
        !missing.exists(),
        "CLI must not create the supplied db path"
    );
    let dirdb = dir.path().join("目录 db");
    fs::create_dir(&dirdb).unwrap();
    let cancel = workflow_args("cancel", dirdb.as_os_str(), run);
    let (out, v) = invoke(&cancel);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(v["errors"][0]["field_path"], "db");
    assert!(dirdb.is_dir());
    let db = seed_workflow(dir.path());
    let (out, v) = invoke(&workflow_args(
        "status",
        db.as_os_str(),
        OsStr::new("不存在-run"),
    ));
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(v["status"], "FAIL");
    assert!(
        v["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("unknown run")
    );
    let (out, _) = invoke(&workflow_args(
        "cancel",
        db.as_os_str(),
        OsStr::new("不存在-run"),
    ));
    assert_eq!(out.status.code(), Some(1));
    let (out, v) = invoke(&workflow_args("status", db.as_os_str(), OsStr::new("")));
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(v["status"], "INVALID_INPUT");
    assert_eq!(v["errors"][0]["field_path"], "run_id");
    let app = Application::open(&db).unwrap();
    assert!(!app.workflow_cancelled("运行-α").unwrap());
    assert_eq!(
        app.workflow_steps("运行-α").unwrap()[0].state,
        State::Succeeded
    );
    drop(app);
    for argv in [
        vec![OsStr::new("workflow"), OsStr::new("status")],
        vec![
            OsStr::new("workflow"),
            OsStr::new("status"),
            OsStr::new("--db"),
            db.as_os_str(),
        ],
        vec![
            OsStr::new("workflow"),
            OsStr::new("cancel"),
            OsStr::new("--db"),
            db.as_os_str(),
        ],
    ] {
        let (out, v) = invoke(&argv);
        assert_eq!(out.status.code(), Some(2), "{argv:?}");
        assert_eq!(v["status"], "INVALID_INPUT");
    }
    let empty = dir.path().join("空 库.sqlite");
    fs::write(&empty, b"").unwrap();
    let (out, v) = invoke(&workflow_args("status", empty.as_os_str(), run));
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(v["status"], "INPUT_INCOMPLETE");
    assert!(
        v["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("not initialized")
    );
    assert_eq!(fs::read(&empty).unwrap(), b"", "no auto-init/migration");
    for argv in [
        vec![
            OsStr::new("workflow"),
            OsStr::new("submit"),
            OsStr::new("--db"),
            db.as_os_str(),
            OsStr::new("--run-id"),
            run,
        ],
        vec![
            OsStr::new("workflow"),
            OsStr::new("run"),
            OsStr::new("--db"),
            db.as_os_str(),
            OsStr::new("--run-id"),
            run,
        ],
        vec![OsStr::new("submit")],
        vec![OsStr::new("create")],
    ] {
        let (out, v) = invoke(&argv);
        assert_eq!(out.status.code(), Some(2), "{argv:?}");
        assert_eq!(v["status"], "INVALID_INPUT");
    }
    let app = Application::open(&db).unwrap();
    assert_eq!(
        app.workflow_steps("运行-α").unwrap()[0].state,
        State::Succeeded
    );
}

#[cfg(any(unix, windows))]
#[test]
fn workflow_protocol_survives_broken_streams() {
    let dir = tempfile::tempdir().unwrap();
    let db = seed_workflow(dir.path());
    let mut args = vec![OsStr::new("--call-id"), OsStr::new("wf-id")];
    args.extend(workflow_args(
        "status",
        db.as_os_str(),
        OsStr::new("运行-α"),
    ));
    let ordinary = Command::new(env!("CARGO_BIN_EXE_assetweft"))
        .args(&args)
        .output()
        .unwrap();
    let broken_err = Command::new(env!("CARGO_BIN_EXE_assetweft"))
        .args(&args)
        .stderr(broken_output())
        .output()
        .unwrap();
    assert_eq!(broken_err.status.code(), ordinary.status.code());
    assert_eq!(broken_err.stdout, ordinary.stdout);
    let value: Value = serde_json::from_slice(&broken_err.stdout).unwrap();
    assert_eq!(value["call_id"], "wf-id");
    let mut missing = vec![OsStr::new("--call-id"), OsStr::new("wf-id")];
    let missing_db = dir.path().join("缺失 库.sqlite");
    missing.extend(workflow_args(
        "status",
        missing_db.as_os_str(),
        OsStr::new("运行-α"),
    ));
    for argv in [args, missing] {
        let status = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(&argv)
            .stdout(broken_output())
            .stderr(broken_output())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(6), "{argv:?}");
    }
}

fn library_init_args(db: &OsStr) -> Vec<&OsStr> {
    vec![
        OsStr::new("library"),
        OsStr::new("init"),
        OsStr::new("--db"),
        db,
    ]
}
fn create_args<'a>(db: &'a OsStr, run: &'a OsStr, steps: &'a OsStr) -> Vec<&'a OsStr> {
    let mut args = workflow_args("create", db, run);
    args.extend([OsStr::new("--steps"), steps]);
    args
}

#[cfg(unix)]
#[test]
fn workflow_create_rejects_steps_fifo_without_waiting_for_a_writer() {
    use std::{
        process::Stdio,
        time::{Duration, Instant},
    };

    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("library.sqlite");
    let (out, v) = invoke(&library_init_args(db.as_os_str()));
    assert!(out.status.success(), "{v}");
    let fifo = dir.path().join("steps.fifo");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );

    let mut child = Command::new(env!("CARGO_BIN_EXE_assetweft"))
        .args(create_args(
            db.as_os_str(),
            OsStr::new("fifo-run"),
            fifo.as_os_str(),
        ))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            child.wait().unwrap();
            panic!("workflow create blocked on a writerless FIFO for 5 seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let (out, v) = parse_output(child.wait_with_output().unwrap());
    assert_eq!(out.status.code(), Some(2), "{v}");
    assert_eq!(v["status"], "INVALID_INPUT");
    assert_eq!(v["errors"][0]["code"], "INVALID_INPUT");
    assert_eq!(v["errors"][0]["field_path"], "steps");
    assert!(v["results"].is_null());
    let (conn, _) = assetweft_core::sqlite::open_and_configure(&db).unwrap();
    let count: i64 = conn
        .query_row("SELECT count(*) FROM aw_workflow_runs", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn failed_historical_upgrade_preserves_library_after_cli_exit() {
    use assetweft_core::sqlite::open_and_configure;

    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("library.sqlite");
    let baseline = dir.path().join("before.sqlite");
    let bytes = b"historical mesh";
    let hash = sha256(bytes);
    let objects = dir.path().join("objects");
    fs::create_dir(&objects).unwrap();
    fs::write(objects.join(&hash), bytes).unwrap();
    let owner = dir.path().join(".aw-object-owner");
    fs::write(&owner, b"library.sqlite").unwrap();

    let (conn, _) = open_and_configure(&db).unwrap();
    conn.execute_batch(include_str!(
        "../../assetweft-core/src/library/001_initial.sql"
    ))
    .unwrap();
    conn.execute_batch(include_str!(
        "../../assetweft-core/src/library/002_objects.sql"
    ))
    .unwrap();
    conn.execute_batch(
        "CREATE TABLE aw_schema_version (singleton INTEGER PRIMARY KEY CHECK(singleton = 1), version INTEGER NOT NULL CHECK(version > 0));
         INSERT INTO aw_schema_version VALUES(1,2);
         INSERT INTO aw_assets VALUES('character'),('mesh');"
    ).unwrap();
    for (asset, version) in [("character", "character-v1"), ("mesh", "mesh-v1")] {
        conn.execute(
            "INSERT INTO aw_asset_versions VALUES(?1,?2,?3,'input','synthetic','UNKNOWN',0,'UNKNOWN','local','BIN','1.0.0','UNKNOWN')",
            [version, asset, &hash],
        ).unwrap();
    }
    conn.execute(
        "INSERT INTO aw_objects VALUES(?1,?2,0,1)",
        (&hash, bytes.len() as i64),
    )
    .unwrap();
    conn.execute(
        "INSERT INTO aw_managed_versions VALUES('mesh-v1',?1)",
        [&hash],
    )
    .unwrap();
    conn.execute_batch(
        "INSERT INTO aw_component_references VALUES('character-v1','mesh','mesh-v1');
         CREATE INDEX aw_feedback_version_order ON aw_assets(asset_id);",
    )
    .unwrap();
    conn.execute("VACUUM INTO ?1", [baseline.to_str().unwrap()])
        .unwrap();
    drop(conn);

    let args = library_init_args(db.as_os_str());
    let (failed, result) = invoke(&args);
    assert_eq!(failed.status.code(), Some(1), "{result}");
    assert!(
        result["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("index aw_feedback_version_order already exists"),
        "{result}"
    );

    let (conn, _) = open_and_configure(&db).unwrap();
    conn.execute("ATTACH ?1 AS original", [baseline.to_str().unwrap()])
        .unwrap();
    for table in [
        "sqlite_schema",
        "aw_schema_version",
        "aw_assets",
        "aw_asset_versions",
        "aw_component_references",
        "aw_objects",
        "aw_managed_versions",
    ] {
        let columns = if table == "sqlite_schema" {
            "type,name,tbl_name,sql"
        } else {
            "*"
        };
        for (left, right) in [("original", "main"), ("main", "original")] {
            let sql = format!(
                "SELECT EXISTS(SELECT {columns} FROM {left}.{table}
                EXCEPT SELECT {columns} FROM {right}.{table})"
            );
            let differs: bool = conn.query_row(&sql, [], |r| r.get(0)).unwrap();
            assert!(!differs, "{left}.{table} differs from {right}.{table}");
        }
    }
    assert_eq!(fs::read(&owner).unwrap(), b"library.sqlite");
    assert_eq!(fs::read(objects.join(&hash)).unwrap(), bytes);

    conn.execute_batch("DROP INDEX main.aw_feedback_version_order")
        .unwrap();
    drop(conn);
    let (retried, result) = invoke(&args);
    assert!(retried.status.success(), "{result}");
    let (conn, _) = open_and_configure(&db).unwrap();
    let version: i64 = conn
        .query_row("SELECT version FROM aw_schema_version", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(version, 6);
    let mut app = Application::open(&db).unwrap();
    assert_eq!(
        app.managed_object("mesh-v1").unwrap().unwrap().content_hash,
        hash
    );
    let refs = app.component_references("character-v1").unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].component_version_id, "mesh-v1");
}

#[test]
fn library_init_create_status_cancel_durable_chain() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("显式 库.sqlite");
    let steps = dir.path().join("步骤 α.json");
    let input = br#"[{"id":"later","dependencies":[" seed "]},{"id":" seed ","dependencies":[]}]"#;
    fs::write(&steps, input).unwrap();
    let mut init = library_init_args(db.as_os_str());
    init.extend([OsStr::new("--call-id"), OsStr::new("init-id")]);
    let (out, v) = invoke(&init);
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert_eq!(v["command"], "library init");
    assert_eq!(v["call_id"], "init-id");
    assert_eq!(v["status"], "PASS");
    assert_eq!(v["results"], json!({"scope":"initialized_library"}));
    Application::open(&db).unwrap().close().unwrap();
    let run = OsStr::new(" 运行 α ");
    let mut create = vec![OsStr::new("--call-id"), OsStr::new("create-id")];
    create.extend(create_args(db.as_os_str(), run, steps.as_os_str()));
    let (out, v) = invoke(&create);
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert_eq!(v["command"], "workflow create");
    assert_eq!(v["call_id"], "create-id");
    assert_eq!(
        v["results"],
        json!({"scope":"persisted_workflow_created","run_id":" 运行 α "})
    );
    let expected = json!([
        {"id":"later","state":"pending","raw":null,"evidence":null},
        {"id":" seed ","state":"pending","raw":null,"evidence":null}
    ]);
    let (out, v) = invoke(&workflow_args("status", db.as_os_str(), run));
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(v["results"]["steps"], expected);
    assert_eq!(v["results"]["cancellation_requested"], false);
    let mut app = Application::open(&db).unwrap();
    assert_eq!(app.workflow_steps(run.to_str().unwrap()).unwrap().len(), 2);
    assert_eq!(
        app.claim_workflow_step(run.to_str().unwrap()).unwrap(),
        Some(" seed ".into())
    );
    app.finish_workflow_step(
        run.to_str().unwrap(),
        " seed ",
        &Outcome::Success {
            raw: "exact result".into(),
            evidence: Some("fixture".into()),
        },
    )
    .unwrap();
    app.close().unwrap();
    let (out, _) = invoke(&workflow_args("cancel", db.as_os_str(), run));
    assert_eq!(out.status.code(), Some(0));
    let (_, before) = invoke(&workflow_args("status", db.as_os_str(), run));
    assert_eq!(before["results"]["cancellation_requested"], true);
    let (out, _) = invoke(&init);
    assert_eq!(out.status.code(), Some(0));
    let (_, after) = invoke(&workflow_args("status", db.as_os_str(), run));
    assert_eq!(
        before, after,
        "repeat init preserves workflow, outcome and cancellation"
    );
    let (out, v) = invoke(&create);
    assert_eq!(out.status.code(), Some(2), "duplicate SQL constraint: {v}");
    assert_eq!(v["status"], "INVALID_INPUT");
    assert_eq!(v["call_id"], "create-id");
    let (_, unchanged) = invoke(&workflow_args("status", db.as_os_str(), run));
    assert_eq!(unchanged, after);
    assert_eq!(fs::read(&steps).unwrap(), input);
}

#[test]
fn workflow_create_rejects_wire_and_core_graph_without_partial_runs() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("校验 库.sqlite");
    let steps = dir.path().join("步骤.json");
    assert_eq!(
        invoke(&library_init_args(db.as_os_str())).0.status.code(),
        Some(0)
    );
    let cases = [
        ("{", 2),
        ("{}", 2),
        (r#"[["a",[]]]"#, 2),
        (r#"[{"id":"a","id":"b","dependencies":[]}]"#, 2),
        ("[{}]", 2),
        (r#"[{"id":"a"}]"#, 2),
        (r#"[{"id":"a","dependencies":null}]"#, 2),
        (r#"[{"id":1,"dependencies":[]}]"#, 2),
        (r#"[{"id":"a","dependencies":[1]}]"#, 2),
        (r#"[{"id":"a","dependencies":[],"extra":true}]"#, 2),
        ("[]", 1),
        (
            r#"[{"id":"a","dependencies":[]},{"id":"a","dependencies":[]}]"#,
            1,
        ),
        (r#"[{"id":"a","dependencies":["absent"]}]"#, 1),
        (
            r#"[{"id":"a","dependencies":["b"]},{"id":"b","dependencies":["a"]}]"#,
            1,
        ),
        (
            r#"[{"id":"a","dependencies":[]},{"id":"b","dependencies":["a","a"]}]"#,
            1,
        ),
    ];
    for (index, (input, code)) in cases.iter().enumerate() {
        let run = format!("rejected-{index}");
        fs::write(&steps, input).unwrap();
        let mut args = vec![OsStr::new("--call-id"), OsStr::new("invalid-create")];
        args.extend(create_args(
            db.as_os_str(),
            OsStr::new(&run),
            steps.as_os_str(),
        ));
        let (out, v) = invoke(&args);
        assert_eq!(out.status.code(), Some(*code), "{input}: {v}");
        assert_eq!(v["command"], "workflow create");
        assert_eq!(v["call_id"], "invalid-create");
        assert_eq!(
            v["status"],
            if *code == 2 { "INVALID_INPUT" } else { "FAIL" }
        );
        assert_eq!(v["results"], Value::Null);
        let app = Application::open(&db).unwrap();
        assert!(
            app.workflow_steps(&run).is_err(),
            "failed creation left a run"
        );
        app.close().unwrap();
    }
    let valid = r#"[{"id":"a","dependencies":[]}]"#;
    fs::write(&steps, valid).unwrap();
    for run in [String::new(), "r".repeat(129), "bad\nrun".into()] {
        let mut args = vec![OsStr::new("--call-id"), OsStr::new("invalid-run")];
        args.extend(create_args(
            db.as_os_str(),
            OsStr::new(&run),
            steps.as_os_str(),
        ));
        let (out, v) = invoke(&args);
        assert_eq!(v["call_id"], "invalid-run");
        assert_eq!(out.status.code(), Some(1), "core owns run policy: {v}");
        assert_eq!(v["status"], "FAIL");
        assert!(
            Application::open(&db)
                .unwrap()
                .workflow_steps(&run)
                .is_err()
        );
    }
    let mut args = create_args(db.as_os_str(), OsStr::new("bounded"), steps.as_os_str());
    args.extend([OsStr::new("--max-bytes"), OsStr::new("1")]);
    let (out, v) = invoke(&args);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(v["errors"][0]["field_path"], "steps");
    assert!(
        Application::open(&db)
            .unwrap()
            .workflow_steps("bounded")
            .is_err()
    );
    let absent_steps = dir.path().join("missing steps.json");
    let (out, v) = invoke(&create_args(
        db.as_os_str(),
        OsStr::new("unreadable"),
        absent_steps.as_os_str(),
    ));
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(v["errors"][0]["field_path"], "steps");
    assert!(
        Application::open(&db)
            .unwrap()
            .workflow_steps("unreadable")
            .is_err()
    );
    fs::write(
        &steps,
        serde_json::to_vec(
            &(0..65)
                .map(|i| json!({"id":i.to_string(),"dependencies":[]}))
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    )
    .unwrap();
    let (out, v) = invoke(&create_args(
        db.as_os_str(),
        OsStr::new("too-many"),
        steps.as_os_str(),
    ));
    assert_eq!(out.status.code(), Some(1), "{v}");
    assert!(
        Application::open(&db)
            .unwrap()
            .workflow_steps("too-many")
            .is_err()
    );
}

#[test]
fn explicit_init_and_create_database_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let steps = dir.path().join("步骤.json");
    fs::write(&steps, r#"[{"id":"a","dependencies":[]}]"#).unwrap();
    let missing = dir.path().join("不存在.sqlite");
    let nonregular = dir.path().join("目录 db");
    fs::create_dir(&nonregular).unwrap();
    for db in [&missing, &nonregular] {
        let (out, v) = invoke(&create_args(
            db.as_os_str(),
            OsStr::new("new"),
            steps.as_os_str(),
        ));
        assert_eq!(out.status.code(), Some(2), "{v}");
        assert_eq!(v["errors"][0]["field_path"], "db");
    }
    assert!(!missing.exists());
    assert!(nonregular.is_dir());
    let empty = dir.path().join("空 db.sqlite");
    fs::write(&empty, []).unwrap();
    let (out, v) = invoke(&create_args(
        empty.as_os_str(),
        OsStr::new("new"),
        steps.as_os_str(),
    ));
    assert_eq!(out.status.code(), Some(3), "{v}");
    assert_eq!(v["status"], "INPUT_INCOMPLETE");
    assert_eq!(fs::read(&empty).unwrap(), b"");
    let corrupt = dir.path().join("损坏 db.sqlite");
    let bytes = b"not a SQLite database - keep original bytes";
    fs::write(&corrupt, bytes).unwrap();
    let parent_missing = dir.path().join("missing parent").join("db.sqlite");
    for db in [&corrupt, &parent_missing, &nonregular] {
        let (out, v) = invoke(&library_init_args(db.as_os_str()));
        assert_ne!(out.status.code(), Some(0), "{v}");
        assert_eq!(v["results"], Value::Null);
    }
    assert_eq!(fs::read(&corrupt).unwrap(), bytes);
    assert!(!parent_missing.parent().unwrap().exists());
    for input in ["", ":memory:", "file:never-create.sqlite"] {
        let (out, v) = invoke(&library_init_args(OsStr::new(input)));
        assert_eq!(out.status.code(), Some(2), "{v}");
    }
}

#[test]
fn cli_created_pending_cancellation_has_ready_control() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("取消 对照.sqlite");
    let steps = dir.path().join("独立步骤.json");
    fs::write(
        &steps,
        r#"[{"id":"ready","dependencies":[]},{"id":"also-ready","dependencies":[]}]"#,
    )
    .unwrap();
    assert!(
        invoke(&library_init_args(db.as_os_str()))
            .0
            .status
            .success()
    );
    for run in ["control", "target"] {
        assert!(
            invoke(&create_args(
                db.as_os_str(),
                OsStr::new(run),
                steps.as_os_str()
            ))
            .0
            .status
            .success()
        );
    }
    assert!(
        invoke(&workflow_args(
            "cancel",
            db.as_os_str(),
            OsStr::new("target")
        ))
        .0
        .status
        .success()
    );
    let mut app = Application::open(&db).unwrap();
    let before = app.workflow_steps("target").unwrap();
    assert!(
        before
            .iter()
            .all(|s| s.state == State::Pending && s.outcome.is_none())
    );
    assert!(app.workflow_cancelled("target").unwrap());
    assert!(!app.workflow_cancelled("control").unwrap());
    assert_eq!(
        app.claim_workflow_step("control").unwrap(),
        Some("ready".into())
    );
    assert_eq!(app.claim_workflow_step("target").unwrap(), None);
    assert_eq!(app.workflow_steps("target").unwrap(), before);
    app.close().unwrap();
    let mut app = Application::open(&db).unwrap();
    assert!(app.workflow_cancelled("target").unwrap());
    assert_eq!(app.claim_workflow_step("target").unwrap(), None);
    assert_eq!(app.workflow_steps("target").unwrap(), before);
}

#[test]
fn init_create_nested_help_and_required_arguments() {
    for command in [
        vec!["library"],
        vec!["library", "init"],
        vec!["workflow"],
        vec!["workflow", "create"],
    ] {
        let mut args = vec![OsStr::new("--call-id"), OsStr::new("nested-id")];
        args.extend(command.iter().map(OsStr::new));
        args.push(OsStr::new("--help"));
        let (out, v) = invoke(&args);
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert_eq!(v["status"], "HELP");
        assert_eq!(v["call_id"], "nested-id");
        args.pop();
        let (out, v) = invoke(&args);
        assert_eq!(out.status.code(), Some(2), "{v}");
        assert_eq!(v["call_id"], "nested-id");
    }
}

#[cfg(any(unix, windows))]
#[test]
fn init_create_broken_streams_do_not_replay_mutations() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("输出 db.sqlite");
    let steps = dir.path().join("步骤.json");
    fs::write(&steps, r#"[{"id":"a","dependencies":[]}]"#).unwrap();
    let mut init = vec![OsStr::new("--call-id"), OsStr::new("stream-id")];
    init.extend(library_init_args(db.as_os_str()));
    let out = Command::new(env!("CARGO_BIN_EXE_assetweft"))
        .args(&init)
        .stderr(broken_output())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["call_id"], "stream-id");
    assert_eq!(out.stdout.iter().filter(|b| **b == b'\n').count(), 1);
    let mut create = vec![OsStr::new("--call-id"), OsStr::new("stream-id")];
    create.extend(create_args(
        db.as_os_str(),
        OsStr::new("created"),
        steps.as_os_str(),
    ));
    let out = Command::new(env!("CARGO_BIN_EXE_assetweft"))
        .args(&create)
        .stderr(broken_output())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["call_id"], "stream-id");
    assert_eq!(out.stdout.iter().filter(|b| **b == b'\n').count(), 1);
    let out = Command::new(env!("CARGO_BIN_EXE_assetweft"))
        .args(&create)
        .stderr(broken_output())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["call_id"], "stream-id");
    let failed_db = dir.path().join("missing parent").join("db.sqlite");
    let mut failed_init = vec![OsStr::new("--call-id"), OsStr::new("failed-init")];
    failed_init.extend(library_init_args(failed_db.as_os_str()));
    let out = Command::new(env!("CARGO_BIN_EXE_assetweft"))
        .args(&failed_init)
        .stderr(broken_output())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["call_id"], "failed-init");
    let ambiguous_db = dir.path().join("stdout failed.sqlite");
    let ambiguous_init = library_init_args(ambiguous_db.as_os_str());
    let ambiguous_create = create_args(db.as_os_str(), OsStr::new("ambiguous"), steps.as_os_str());
    for argv in [&ambiguous_init, &ambiguous_create, &create, &failed_init] {
        let status = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(argv)
            .stdout(broken_output())
            .stderr(broken_output())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(6), "{argv:?}");
    }
    let mut initialized = Application::open(&ambiguous_db).unwrap();
    initialized
        .create_workflow("core-schema-probe", &[step("a", &[])])
        .unwrap();
    initialized.close().unwrap();
    let app = Application::open(&db).unwrap();
    let records = app.workflow_steps("ambiguous").unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].state, State::Pending);
    assert_eq!(records[0].outcome, None);
}

fn budget_args<'a>(action: &'a str, db: &'a OsStr, value: &'a OsStr) -> Vec<&'a OsStr> {
    vec![
        OsStr::new("budget"),
        OsStr::new(action),
        OsStr::new("--db"),
        db,
        OsStr::new(match action {
            "approve" => "--allowance",
            "reserve" => "--request",
            "balance" => "--budget-id",
            _ => "--intent-id",
        }),
        value,
    ]
}
fn budget_fixture(dir: &Path) -> (PathBuf, PathBuf, Value) {
    let db = dir.join("预算 本地.sqlite");
    let (out, v) = invoke(&library_init_args(db.as_os_str()));
    assert!(out.status.success(), "{v}");
    let path = dir.join("声明 α.json");
    let value = json!({"id":" 预算 α ","unit":"Token","capacity":100,
        "approval":"ExplicitBounded","source":" TEST_FIXTURE 本地声明 "});
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    (db, path, value)
}
fn declared_balance(db: &Path, id: &str) -> Value {
    let (out, v) = invoke(&budget_args("balance", db.as_os_str(), OsStr::new(id)));
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert_eq!(v["command"], "budget balance");
    assert_eq!(v["status"], "PASS");
    v["results"].clone()
}
#[test]
fn budget_declarations_reopen_exactly_and_never_reset_reservations() {
    let dir = tempfile::tempdir().unwrap();
    let (db, path, mut declaration) = budget_fixture(dir.path());
    for (index, (unit, capacity, approval)) in [
        (json!("Token"), 100_u64, "ExplicitBounded"),
        (json!("Task"), 0, "ExplicitBounded"),
        (json!("Second"), 0, "ZeroAdditionalCost"),
        (
            json!({"CurrencyMinor":{"currency":"USD","decimals":2}}),
            12345,
            "ExplicitBounded",
        ),
        (
            json!({"CurrencyMinor":{"currency":"JPY","decimals":0}}),
            0,
            "ZeroAdditionalCost",
        ),
        (json!("Token"), 9223372036854775807, "ExplicitBounded"),
    ]
    .into_iter()
    .enumerate()
    {
        declaration["id"] = json!(format!(" 预算 {index} "));
        declaration["unit"] = unit;
        declaration["capacity"] = json!(capacity);
        declaration["approval"] = json!(approval);
        let bytes = serde_json::to_vec(&declaration).unwrap();
        if index == 5 {
            assert!(
                std::str::from_utf8(&bytes)
                    .unwrap()
                    .contains("\"capacity\":9223372036854775807")
            );
        }
        fs::write(&path, &bytes).unwrap();
        let mut args = budget_args("approve", db.as_os_str(), path.as_os_str());
        args.extend([OsStr::new("--call-id"), OsStr::new("预算-call")]);
        let (first, v) = invoke(&args);
        assert_eq!(first.status.code(), Some(0), "{v}");
        assert_eq!(v["command"], "budget approve");
        assert_eq!(v["call_id"], "预算-call");
        assert_eq!(
            v["results"],
            json!({"scope":"stored_local_allowance_declaration","allowance":declaration})
        );
        let (again, _) = invoke(&args);
        assert_eq!(again.stdout, first.stdout);
        assert!(again.status.success());
        assert!(first.stderr.is_empty());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let id = declaration["id"].as_str().unwrap();
        assert_eq!(
            declared_balance(&db, id),
            json!({"scope":"local_budget_balance",
            "allowance":declaration,"reserved":0,"spent":0})
        );
        let app = Application::open(&db).unwrap();
        assert_eq!(
            serde_json::to_value(app.budget_balance(id).unwrap().allowance).unwrap(),
            declaration
        );
    }
    let id = " 预算 0 ";
    let mut app = Application::open(&db).unwrap();
    app.reserve_call(&assetweft_core::budget::Request {
        id: "local-reservation".into(),
        account: id.into(),
        input_fingerprint: "TEST_FIXTURE:input".into(),
        unit: assetweft_core::budget::Unit::Token,
        quote: Some(7),
        reservation: 7,
        source: "TEST_FIXTURE local quote".into(),
    })
    .unwrap();
    let original = app.budget_balance(id).unwrap();
    drop(app);
    let original_wire = serde_json::to_value(&original.allowance).unwrap();
    fs::write(&path, format!("{{\"source\":{},\"approval\":\"ExplicitBounded\",\"capacity\":100,\"unit\":\"Token\",\"id\":{}}}", original_wire["source"], json!(id))).unwrap();
    let (out, v) = invoke(&budget_args("approve", db.as_os_str(), path.as_os_str()));
    assert!(out.status.success(), "{v}");
    for (field, value) in [
        ("capacity", json!(101)),
        ("unit", json!("Task")),
        ("approval", json!("ZeroAdditionalCost")),
        ("source", json!("different local source")),
    ] {
        let mut conflict = original_wire.clone();
        conflict[field] = value;
        if field == "approval" {
            conflict["capacity"] = json!(0);
        }
        fs::write(&path, serde_json::to_vec(&conflict).unwrap()).unwrap();
        let (out, v) = invoke(&budget_args("approve", db.as_os_str(), path.as_os_str()));
        assert_eq!(out.status.code(), Some(2), "{v}");
        assert!(
            v["errors"][0]["message"]
                .as_str()
                .unwrap()
                .contains("allowance identity conflict"),
            "{v}"
        );
        let app = Application::open(&db).unwrap();
        assert_eq!(app.budget_balance(id).unwrap(), original);
    }
    assert_eq!(
        declared_balance(&db, id),
        json!({"scope":"local_budget_balance",
        "allowance":original_wire,"reserved":7,"spent":0})
    );
    let zero = Application::open(&db)
        .unwrap()
        .budget_balance(" 预算 1 ")
        .unwrap();
    let mut conflict = serde_json::to_value(&zero.allowance).unwrap();
    conflict["approval"] = json!("ZeroAdditionalCost");
    fs::write(&path, conflict.to_string()).unwrap();
    let (out, v) = invoke(&budget_args("approve", db.as_os_str(), path.as_os_str()));
    assert_eq!(out.status.code(), Some(2), "{v}");
    assert!(
        v["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("allowance identity conflict")
    );
    assert_eq!(
        Application::open(&db)
            .unwrap()
            .budget_balance(" 预算 1 ")
            .unwrap(),
        zero
    );
}

#[test]
fn budget_wire_rejects_bad_shapes_and_core_rejections_leave_no_partial_allowance() {
    let dir = tempfile::tempdir().unwrap();
    let (db, path, good) = budget_fixture(dir.path());
    let args = budget_args("approve", db.as_os_str(), path.as_os_str());
    let (out, v) = invoke(&args);
    assert!(out.status.success(), "{v}");
    let baseline = declared_balance(&db, good["id"].as_str().unwrap());
    let mut bad = vec!["{broken".to_string(),"null".into(),"[]".into(),
        r#"["x","Token",0,"ExplicitBounded","s"]"#.into(),
        r#"{"id":"x","id":"x","unit":"Token","capacity":0,"approval":"ExplicitBounded","source":"s"}"#.into()];
    for field in ["id", "unit", "capacity", "approval", "source"] {
        let mut value = good.clone();
        value.as_object_mut().unwrap().remove(field);
        bad.push(value.to_string());
        let mut value = good.clone();
        value[field] = Value::Null;
        bad.push(value.to_string());
    }
    for (field, value) in [
        ("extra", json!(true)),
        ("id", json!(4)),
        ("source", json!([])),
        ("approval", json!("explicitbounded")),
        ("capacity", json!(-1)),
        ("capacity", json!(1.5)),
        ("capacity", json!("10")),
        ("unit", json!("token")),
        ("unit", json!(["Token"])),
    ] {
        let mut v = good.clone();
        v[field] = value;
        bad.push(v.to_string());
    }
    for unit in [
        r#"{"CurrencyMinor":["USD",2]}"#,
        r#"{"CurrencyMinor":{}}"#,
        r#"{"CurrencyMinor":{"currency":"USD"}}"#,
        r#"{"CurrencyMinor":{"decimals":2}}"#,
        r#"{"CurrencyMinor":{"currency":"USD","decimals":2,"extra":0}}"#,
        r#"{"CurrencyMinor":{"currency":"USD","currency":"USD","decimals":2}}"#,
        r#"{"CurrencyMinor":{"currency":"USD","decimals":2,"decimals":2}}"#,
        r#"{"CurrencyMinor":{"currency":3,"decimals":2}}"#,
        r#"{"CurrencyMinor":{"currency":"USD","decimals":-1}}"#,
        r#"{"CurrencyMinor":{"currency":"USD","decimals":1.5}}"#,
        r#"{"CurrencyMinor":{"currency":"USD","decimals":256}}"#,
        r#"{"CurrencyMinor":{"currency":"USD","decimals":2},"Token":null}"#,
        r#"{"CurrencyMinor":{"currency":"USD","decimals":2},"CurrencyMinor":{"currency":"USD","decimals":2}}"#,
    ] {
        bad.push(format!(r#"{{"id":"shape-rejected","unit":{unit},"capacity":0,"approval":"ExplicitBounded","source":"s"}}"#));
    }
    bad.push(r#"{"id":"overflow","unit":"Token","capacity":18446744073709551616,"approval":"ExplicitBounded","source":"s"}"#.into());
    for raw in bad {
        fs::write(&path, &raw).unwrap();
        let (out, v) = invoke(&args);
        assert_eq!(out.status.code(), Some(2), "{raw}: {v}");
        assert_eq!(v["errors"][0]["field_path"], "allowance", "{raw}: {v}");
        assert_eq!(v["status"], "INVALID_INPUT");
        assert_eq!(v["results"], Value::Null);
        assert_eq!(
            declared_balance(&db, good["id"].as_str().unwrap()),
            baseline
        );
        let app = Application::open(&db).unwrap();
        assert!(app.budget_balance("shape-rejected").is_err());
        assert!(app.budget_balance("overflow").is_err());
    }
    for (field, value, message) in [
        (
            "capacity",
            json!(9223372036854775808_u64),
            "SQLite integer limit",
        ),
        (
            "unit",
            json!({"CurrencyMinor":{"currency":"usd","decimals":2}}),
            "currency scale",
        ),
        (
            "unit",
            json!({"CurrencyMinor":{"currency":"US","decimals":2}}),
            "currency scale",
        ),
        (
            "unit",
            json!({"CurrencyMinor":{"currency":"USD","decimals":10}}),
            "currency scale",
        ),
        (
            "approval",
            json!("ZeroAdditionalCost"),
            "zero cost capacity",
        ),
        ("source", json!(""), "bounded text"),
        ("source", json!("s".repeat(4097)), "bounded text"),
        ("source", json!("x\0y"), "bounded text"),
        ("id", json!(""), "bounded text"),
        ("id", json!("x".repeat(4097)), "bounded text"),
        ("id", json!("x\0y"), "bounded text"),
    ] {
        let mut v = good.clone();
        v["id"] = json!("core-rejected");
        v[field] = value;
        fs::write(&path, v.to_string()).unwrap();
        let (out, result) = invoke(&args);
        assert_eq!(out.status.code(), Some(2), "{result}");
        assert_eq!(result["errors"][0]["field_path"], "budget");
        assert!(
            result["errors"][0]["message"]
                .as_str()
                .unwrap()
                .contains(message),
            "{result}"
        );
        assert!(
            Application::open(&db)
                .unwrap()
                .budget_balance(v["id"].as_str().unwrap())
                .is_err()
        );
        assert_eq!(
            declared_balance(&db, good["id"].as_str().unwrap()),
            baseline
        );
    }
    fs::write(&path, good.to_string()).unwrap();
    let mut limited = args.clone();
    limited.extend([OsStr::new("--max-bytes"), OsStr::new("8")]);
    let (out, v) = invoke(&limited);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(v["errors"][0]["field_path"], "allowance");
    fs::remove_file(&path).unwrap();
    let (out, v) = invoke(&args);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(v["errors"][0]["field_path"], "allowance");
}

#[test]
fn budget_approve_rejects_null_tagged_unit_and_approval_without_persisting() {
    let dir = tempfile::tempdir().unwrap();
    let (db, path, good) = budget_fixture(dir.path());
    let args = budget_args("approve", db.as_os_str(), path.as_os_str());
    for (field, alias) in [
        ("unit", "Token"),
        ("unit", "Task"),
        ("unit", "Second"),
        ("approval", "ExplicitBounded"),
        ("approval", "ZeroAdditionalCost"),
    ] {
        let mut input = good.clone();
        input["capacity"] = json!(0);
        input[field] = json!({(alias): null});
        fs::write(&path, input.to_string()).unwrap();
        let (out, v) = invoke(&args);
        assert_eq!(out.status.code(), Some(2), "{field} {alias}: {v}");
        assert_eq!(v["status"], "INVALID_INPUT");
        assert_eq!(v["errors"][0]["code"], "INVALID_INPUT");
        assert_eq!(v["errors"][0]["field_path"], "allowance");
        assert!(v["results"].is_null());
        let (conn, _) = assetweft_core::sqlite::open_and_configure(&db).unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM aw_budget_accounts", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[test]
fn budget_reserve_rejects_null_tagged_unit_without_persisting() {
    let dir = tempfile::tempdir().unwrap();
    let (db, path, mut request) = intent_fixture(dir.path());
    request["unit"] = json!({"Token":null});
    fs::write(&path, request.to_string()).unwrap();
    let (out, v) = invoke(&budget_args("reserve", db.as_os_str(), path.as_os_str()));
    assert_eq!(out.status.code(), Some(2), "{v}");
    assert_eq!(v["status"], "INVALID_INPUT");
    assert_eq!(v["errors"][0]["code"], "INVALID_INPUT");
    assert_eq!(v["errors"][0]["field_path"], "request");
    assert!(v["results"].is_null());
    assert!(local_intent(&db, request["id"].as_str().unwrap()).is_null());
    assert_eq!(
        declared_balance(&db, request["account"].as_str().unwrap())["reserved"],
        0
    );
}

#[test]
fn budget_database_admission_and_missing_identity_are_truthful() {
    let dir = tempfile::tempdir().unwrap();
    let (db, path, good) = budget_fixture(dir.path());
    let missing = dir.path().join("missing.sqlite");
    let empty = dir.path().join("empty.sqlite");
    fs::write(&empty, b"").unwrap();
    for (p, code, field) in [
        (missing.as_path(), 2, "db"),
        (dir.path(), 2, "db"),
        (empty.as_path(), 3, "budget"),
    ] {
        for (action, value) in [
            ("approve", path.as_os_str()),
            ("balance", OsStr::new("missing")),
        ] {
            let (out, v) = invoke(&budget_args(action, p.as_os_str(), value));
            assert_eq!(out.status.code(), Some(code), "{v}");
            assert_eq!(v["errors"][0]["field_path"], field);
            assert_eq!(v["command"], format!("budget {action}"));
        }
    }
    assert!(!missing.exists());
    assert_eq!(fs::read(&empty).unwrap(), b"");
    for id in ["missing", ""] {
        let expected = Application::open(&db)
            .unwrap()
            .budget_balance(id)
            .unwrap_err()
            .to_string();
        let (out, v) = invoke(&budget_args("balance", db.as_os_str(), OsStr::new(id)));
        assert_eq!(out.status.code(), Some(1), "{v}");
        assert_eq!(v["status"], "FAIL");
        assert_eq!(v["errors"][0]["message"], expected);
    }
    let (out, v) = invoke(&budget_args("approve", db.as_os_str(), path.as_os_str()));
    assert!(out.status.success(), "{v}");
    assert_eq!(
        declared_balance(&db, good["id"].as_str().unwrap())["allowance"],
        good
    );
}

#[test]
fn budget_required_arguments_nested_help_and_call_id() {
    for args in [
        vec!["budget"],
        vec!["budget", "approve"],
        vec!["budget", "balance"],
        vec!["budget", "approve", "--db", "x"],
        vec!["budget", "balance", "--db", "x"],
        vec!["budget", "approve", "--allowance", "x"],
        vec!["budget", "balance", "--budget-id", "x"],
        vec!["budget", "reserve"],
        vec!["budget", "submit"],
        vec!["budget", "reconcile"],
    ] {
        let mut os = vec![OsStr::new("--call-id"), OsStr::new("arg-id")];
        os.extend(args.iter().map(OsStr::new));
        let (out, v) = invoke(&os);
        assert_eq!(out.status.code(), Some(2), "{v}");
        assert_eq!(v["call_id"], "arg-id");
        assert_eq!(v["command"], "arguments");
    }
    for action in [None, Some("approve"), Some("balance")] {
        let mut args = vec![OsStr::new("budget")];
        if let Some(a) = action {
            args.push(OsStr::new(a));
        }
        args.extend([
            OsStr::new("--help"),
            OsStr::new("--call-id"),
            OsStr::new("help-id"),
        ]);
        let (out, v) = invoke(&args);
        assert!(out.status.success(), "{v}");
        assert_eq!(v["status"], "HELP");
        assert_eq!(v["call_id"], "help-id");
        let text = v["results"]["text"].as_str().unwrap();
        if action.is_none() {
            assert!(text.contains("approve") && text.contains("balance"));
        } else {
            assert!(text.contains("--db"));
        }
    }
}

#[cfg(any(unix, windows))]
#[test]
fn budget_protocol_broken_streams_keep_committed_local_declaration() {
    let dir = tempfile::tempdir().unwrap();
    let (db, path, good) = budget_fixture(dir.path());
    for (action, value) in [
        ("approve", path.as_os_str()),
        ("balance", OsStr::new(" 预算 α ")),
        ("balance", OsStr::new("absent")),
    ] {
        let mut args = vec![OsStr::new("--call-id"), OsStr::new("budget-io")];
        args.extend(budget_args(action, db.as_os_str(), value));
        let (ordinary, v) = invoke(&args);
        assert_eq!(
            ordinary.status.code(),
            Some(if value == OsStr::new("absent") { 1 } else { 0 }),
            "{v}"
        );
        let failed = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(&args)
            .stderr(broken_output())
            .output()
            .unwrap();
        assert_eq!(failed.stdout, ordinary.stdout);
        assert_eq!(failed.status.code(), ordinary.status.code());
        let status = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(&args)
            .stdout(broken_output())
            .stderr(broken_output())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(6));
    }
    let mut committed = good.clone();
    committed["id"] = json!("committed-output-lost");
    fs::write(&path, committed.to_string()).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_assetweft"))
        .args(budget_args("approve", db.as_os_str(), path.as_os_str()))
        .stdout(broken_output())
        .stderr(broken_output())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(6));
    assert_eq!(
        declared_balance(&db, "committed-output-lost"),
        json!({"scope":"local_budget_balance","allowance":committed,"reserved":0,"spent":0})
    );
    fs::write(&path, b"{broken").unwrap();
    let args = budget_args("approve", db.as_os_str(), path.as_os_str());
    let (ordinary, v) = invoke(&args);
    assert_eq!(ordinary.status.code(), Some(2), "{v}");
    let failed = Command::new(env!("CARGO_BIN_EXE_assetweft"))
        .args(&args)
        .stderr(broken_output())
        .output()
        .unwrap();
    assert_eq!(failed.stdout, ordinary.stdout);
    assert_eq!(failed.status.code(), Some(2));
    let status = Command::new(env!("CARGO_BIN_EXE_assetweft"))
        .args(&args)
        .stdout(broken_output())
        .stderr(broken_output())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(6));
}

fn intent_fixture(dir: &Path) -> (PathBuf, PathBuf, Value) {
    let (db, path, allowance) = budget_fixture(dir);
    let (out, v) = invoke(&budget_args("approve", db.as_os_str(), path.as_os_str()));
    assert!(out.status.success(), "{v}");
    let request = json!({"id":" intent α ","account":allowance["id"],
        "input_fingerprint":" TEST_FIXTURE input α ","unit":"Token",
        "quote":null,"reservation":7,"source":" TEST_FIXTURE caller quote "});
    fs::write(&path, request.to_string()).unwrap();
    (db, path, request)
}
fn local_intent(db: &Path, id: &str) -> Value {
    let (out, v) = invoke(&budget_args("intent", db.as_os_str(), OsStr::new(id)));
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert_eq!(v["command"], "budget intent");
    assert_eq!(v["results"]["scope"], "local_call_intent");
    v["results"]["intent"].clone()
}
#[test]
fn budget_receipts_preserve_persisted_usage_without_writing() {
    use assetweft_core::budget::{Receipt, Request, Resolution, Unit, Usage};

    let dir = tempfile::tempdir().unwrap();
    let (db, _, request) = intent_fixture(dir.path());
    let mut request: Request = serde_json::from_value(request).unwrap();
    let id = request.id.clone();
    let other_id = "intent α";
    let empty_id = "no receipts yet";
    let early = Receipt {
        id: " a α ".into(),
        source: " TEST_FIXTURE unresolved ".into(),
        resolution: Resolution::Unresolved,
        usage: vec![Usage {
            unit: Unit::Second,
            amount: None,
            source: " TEST_FIXTURE unknown seconds ".into(),
        }],
    };
    let late = Receipt {
        id: " z α ".into(),
        source: " TEST_FIXTURE confirmed ".into(),
        resolution: Resolution::Confirmed { charge: 3 },
        usage: vec![
            Usage {
                unit: Unit::Token,
                amount: Some(3),
                source: " tokens ".into(),
            },
            Usage {
                unit: Unit::Task,
                amount: Some(0),
                source: " tasks ".into(),
            },
            Usage {
                unit: Unit::Second,
                amount: Some(8),
                source: " seconds ".into(),
            },
            Usage {
                unit: Unit::CurrencyMinor {
                    currency: "USD".into(),
                    decimals: 2,
                },
                amount: Some(17),
                source: " currency ".into(),
            },
        ],
    };
    let other = Receipt {
        id: early.id.clone(),
        source: " TEST_FIXTURE other intent ".into(),
        resolution: Resolution::AuthoritativeNoCharge,
        usage: vec![],
    };

    let mut app = Application::open(&db).unwrap();
    for call in [&id[..], other_id, empty_id] {
        request.id = call.into();
        app.reserve_call(&request).unwrap();
    }
    assert!(app.submit_test_fixture(&id, || {}).unwrap());
    app.reconcile_call(&id, &early).unwrap();
    app.reconcile_call(&id, &late).unwrap();
    assert!(app.submit_test_fixture(other_id, || {}).unwrap());
    app.reconcile_call(other_id, &other).unwrap();
    app.close().unwrap();

    let bytes = fs::read(&db).unwrap();
    let expected = json!([
        {"id":" a α ","source":" TEST_FIXTURE unresolved ","resolution":"Unresolved",
         "usage":[{"unit":"Second","amount":null,"source":" TEST_FIXTURE unknown seconds "}]},
        {"id":" z α ","source":" TEST_FIXTURE confirmed ","resolution":{"Confirmed":{"charge":3}},
         "usage":[{"unit":"Token","amount":3,"source":" tokens "},
                  {"unit":"Task","amount":0,"source":" tasks "},
                  {"unit":"Second","amount":8,"source":" seconds "},
                  {"unit":{"CurrencyMinor":{"currency":"USD","decimals":2}},
                   "amount":17,"source":" currency "}]}
    ]);
    for (call, receipts) in [
        (&id[..], expected),
        (other_id, json!([other])),
        (empty_id, json!([])),
        ("absent", json!([])),
        ("", json!([])),
    ] {
        let mut args = vec![OsStr::new("--call-id"), OsStr::new(other_id)];
        args.extend(budget_args("receipts", db.as_os_str(), OsStr::new(call)));
        let (out, v) = invoke(&args);
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert!(out.stderr.is_empty());
        assert_eq!(
            v,
            json!({"schema_version":1,"command":"budget receipts","call_id":other_id,
                   "status":"PASS","errors":[],
                   "results":{"scope":"local_call_receipts","receipts":receipts}})
        );
    }
    assert!(
        fs::read(&db).unwrap() == bytes,
        "receipt queries changed the database bytes"
    );
    let app = Application::open_read_only(&db).unwrap();
    assert_eq!(app.call_receipts(&id).unwrap(), vec![early, late]);
    assert_eq!(app.call_receipts(other_id).unwrap(), vec![other]);

    let missing = dir.path().join("missing.sqlite");
    let invalid = dir.path().join("invalid.sqlite");
    fs::write(&invalid, b"invalid database").unwrap();
    for path in [&missing, &invalid, dir.path()] {
        let (out, v) = invoke(&budget_args("receipts", path.as_os_str(), OsStr::new(&id)));
        assert_eq!(out.status.code(), Some(2), "{v}");
        assert_eq!(v["command"], "budget receipts");
        assert_eq!(v["status"], "INVALID_INPUT");
        assert_eq!(v["errors"][0]["field_path"], "db");
        assert_eq!(v["results"], Value::Null);
    }
    assert!(!missing.exists());
    assert_eq!(fs::read(&invalid).unwrap(), b"invalid database");
}

#[test]
fn intent_reserve_reopen_exact_identity_and_quote_semantics() {
    use assetweft_core::budget::{Allowance, Approval, Unit};
    let dir = tempfile::tempdir().unwrap();
    let (db, path, request) = intent_fixture(dir.path());
    let mut args = vec![OsStr::new("--call-id"), OsStr::new("correlation-only")];
    args.extend(budget_args("reserve", db.as_os_str(), path.as_os_str()));
    let bytes = fs::read(&path).unwrap();
    let (first, v) = invoke(&args);
    assert_eq!(first.status.code(), Some(0), "{v}");
    assert_eq!(v["command"], "budget reserve");
    assert_eq!(v["call_id"], "correlation-only");
    let expected = json!({"request":request,"state":"PREPARING","cancelled":false,"charge":null});
    assert_eq!(
        v["results"],
        json!({"scope":"local_call_intent","intent":expected})
    );
    assert_eq!(local_intent(&db, " intent α "), expected);
    assert_eq!(local_intent(&db, "correlation-only"), Value::Null);
    assert_eq!(local_intent(&db, ""), Value::Null);
    let (again, _) = invoke(&args);
    assert_eq!(again.status.code(), Some(0));
    assert_eq!(again.stdout, first.stdout);
    assert!(first.stderr.is_empty());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let app = Application::open(&db).unwrap();
    let before = app.budget_balance(" 预算 α ").unwrap();
    assert_eq!((before.reserved, before.spent), (7, 0));
    assert_eq!(
        serde_json::to_value(app.call_intent(" intent α ").unwrap().unwrap().request).unwrap(),
        request
    );
    drop(app);
    for (field, value) in [
        ("input_fingerprint", json!("different")),
        ("source", json!("different")),
        ("quote", json!(0)),
        ("reservation", json!(8)),
        ("unit", json!("Task")),
        ("account", json!("absent")),
    ] {
        let mut conflict = request.clone();
        conflict[field] = value;
        fs::write(&path, conflict.to_string()).unwrap();
        let (out, v) = invoke(&args);
        assert_eq!(out.status.code(), Some(2), "{v}");
        assert_eq!(v["errors"][0]["field_path"], "budget");
        assert_eq!(
            Application::open(&db)
                .unwrap()
                .budget_balance(" 预算 α ")
                .unwrap(),
            before
        );
        assert_eq!(local_intent(&db, " intent α "), expected);
    }
    let mut app = Application::open(&db).unwrap();
    app.approve_budget(&Allowance {
        id: "zero".into(),
        unit: Unit::Token,
        capacity: 0,
        approval: Approval::ZeroAdditionalCost,
        source: "TEST_FIXTURE".into(),
    })
    .unwrap();
    drop(app);
    for quote in [None, Some(Value::Null), Some(json!(0))] {
        let mut r = request.clone();
        r["id"] = json!("zero-intent");
        r["account"] = json!("zero");
        r["reservation"] = json!(0);
        if let Some(q) = quote.clone() {
            r["quote"] = q;
        } else {
            r.as_object_mut().unwrap().remove("quote");
        }
        fs::write(&path, r.to_string()).unwrap();
        let (out, v) = invoke(&args);
        if quote == Some(json!(0)) {
            assert_eq!(out.status.code(), Some(0), "{v}");
            assert_eq!(v["results"]["intent"]["request"]["quote"], 0);
        } else {
            assert_eq!(out.status.code(), Some(2), "{v}");
            assert_eq!(local_intent(&db, "zero-intent"), Value::Null);
        }
    }
    let mut r = request.clone();
    r["id"] = json!("omitted-quote");
    r.as_object_mut().unwrap().remove("quote");
    fs::write(&path, r.to_string()).unwrap();
    let (out, v) = invoke(&args);
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert_eq!(v["results"]["intent"]["request"]["quote"], Value::Null);
    for (index, unit, amount) in [
        (0, Unit::Task, 11),
        (1, Unit::Second, 13),
        (
            2,
            Unit::CurrencyMinor {
                currency: "USD".into(),
                decimals: 2,
            },
            12345,
        ),
        (3, Unit::Token, 9223372036854775807_u64),
    ] {
        let account = format!("exact-{index}");
        let mut app = Application::open(&db).unwrap();
        app.approve_budget(&Allowance {
            id: account.clone(),
            unit: unit.clone(),
            capacity: amount,
            approval: Approval::ExplicitBounded,
            source: "TEST_FIXTURE exact integers".into(),
        })
        .unwrap();
        drop(app);
        let mut r = request.clone();
        r["id"] = json!(account);
        r["account"] = json!(account);
        r["unit"] = serde_json::to_value(unit).unwrap();
        r["quote"] = json!(amount);
        r["reservation"] = json!(amount);
        fs::write(&path, r.to_string()).unwrap();
        let (out, v) = invoke(&args);
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert_eq!(v["results"]["intent"]["request"], r);
        assert_eq!(local_intent(&db, &account)["request"], r);
        let b = Application::open(&db)
            .unwrap()
            .budget_balance(&account)
            .unwrap();
        assert_eq!((b.reserved, b.spent), (amount, 0));
    }
}

#[test]
fn intent_cancel_projects_all_states_without_replay_or_refund() {
    use assetweft_core::budget::{Receipt, Request, Resolution};
    use std::cell::Cell;
    let dir = tempfile::tempdir().unwrap();
    let (db, path, request) = intent_fixture(dir.path());
    let (out, v) = invoke(&budget_args("reserve", db.as_os_str(), path.as_os_str()));
    assert_eq!(out.status.code(), Some(0), "{v}");
    let id = request["id"].as_str().unwrap();
    assert_eq!(local_intent(&db, id)["state"], "PREPARING");
    for _ in 0..2 {
        let (out, v) = invoke(&budget_args("cancel", db.as_os_str(), OsStr::new(id)));
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert_eq!(v["command"], "budget cancel");
        assert_eq!(
            v["results"],
            json!({"scope":"stored_local_call_cancellation_request","intent_id":id})
        );
    }
    assert_eq!(
        local_intent(&db, id),
        json!({"request":request,"state":"RELEASED","cancelled":true,"charge":0})
    );
    let (out, v) = invoke(&budget_args("reserve", db.as_os_str(), path.as_os_str()));
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert_eq!(v["results"]["intent"], local_intent(&db, id));
    let app = Application::open(&db).unwrap();
    assert_eq!(
        (
            app.budget_balance(" 预算 α ").unwrap().reserved,
            app.budget_balance(" 预算 α ").unwrap().spent
        ),
        (0, 0)
    );
    assert!(app.call_receipts(id).unwrap().is_empty());
    drop(app);
    let count = Cell::new(0);
    for (id, interrupted, terminal, charge) in [
        ("attempt α", true, "RELEASED", 0),
        ("unknown α", false, "CONFIRMED", 5),
    ] {
        let mut r = request.clone();
        r["id"] = json!(id);
        let typed: Request = serde_json::from_value(r.clone()).unwrap();
        let mut app = Application::open(&db).unwrap();
        app.reserve_call(&typed).unwrap();
        if interrupted {
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    app.submit_test_fixture(id, || {
                        count.set(count.get() + 1);
                        panic!("TEST_FIXTURE interruption");
                    })
                    .unwrap();
                }))
                .is_err()
            );
        } else {
            assert!(
                app.submit_test_fixture(id, || count.set(count.get() + 1))
                    .unwrap()
            );
        }
        drop(app);
        let state = if interrupted { "ATTEMPTING" } else { "UNKNOWN" };
        assert_eq!(
            local_intent(&db, id),
            json!({"request":r,"state":state,"cancelled":false,"charge":null})
        );
        for _ in 0..2 {
            let (out, v) = invoke(&budget_args("cancel", db.as_os_str(), OsStr::new(id)));
            assert_eq!(out.status.code(), Some(0), "{v}");
        }
        fs::write(&path, r.to_string()).unwrap();
        let (out, v) = invoke(&budget_args("reserve", db.as_os_str(), path.as_os_str()));
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert_eq!(
            v["results"]["intent"],
            json!({"request":r,"state":state,"cancelled":true,"charge":null})
        );
        let mut app = Application::open(&db).unwrap();
        assert_eq!(
            (
                app.budget_balance(" 预算 α ").unwrap().reserved,
                app.budget_balance(" 预算 α ").unwrap().spent
            ),
            (7, 0)
        );
        assert!(app.call_receipts(id).unwrap().is_empty());
        assert!(
            !app.submit_test_fixture(id, || count.set(count.get() + 1))
                .unwrap()
        );
        let receipt = Receipt {
            id: format!("receipt-{id}"),
            source: "TEST_FIXTURE explicit reconciliation".into(),
            resolution: if interrupted {
                Resolution::AuthoritativeNoCharge
            } else {
                Resolution::Confirmed { charge }
            },
            usage: vec![],
        };
        app.reconcile_call(id, &receipt).unwrap();
        drop(app);
        assert_eq!(
            local_intent(&db, id),
            json!({"request":r,"state":terminal,"cancelled":true,"charge":charge})
        );
        let (out, v) = invoke(&budget_args("cancel", db.as_os_str(), OsStr::new(id)));
        assert_eq!(out.status.code(), Some(0), "{v}");
        let app = Application::open(&db).unwrap();
        let b = app.budget_balance(" 预算 α ").unwrap();
        assert_eq!((b.reserved, b.spent), (0, charge));
        assert_eq!(app.call_receipts(id).unwrap(), vec![receipt]);
        drop(app);
        let (out, v) = invoke(&budget_args("reserve", db.as_os_str(), path.as_os_str()));
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert_eq!(v["results"]["intent"]["state"], terminal);
    }
    assert_eq!(count.get(), 2);
}

#[test]
fn intent_strict_wire_and_core_policy_failures_are_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let (db, path, good) = intent_fixture(dir.path());
    let args = budget_args("reserve", db.as_os_str(), path.as_os_str());
    let baseline = Application::open(&db)
        .unwrap()
        .budget_balance(" 预算 α ")
        .unwrap();
    let mut bad = vec![
        "{broken".to_string(),
        "null".into(),
        "[]".into(),
        r#"["i","a","f","Token",null,7,"s"]"#.into(),
    ];
    for field in [
        "id",
        "account",
        "input_fingerprint",
        "unit",
        "reservation",
        "source",
    ] {
        let mut v = good.clone();
        v.as_object_mut().unwrap().remove(field);
        bad.push(v.to_string());
        let mut v = good.clone();
        v[field] = Value::Null;
        bad.push(v.to_string());
    }
    for field in [
        "id",
        "account",
        "input_fingerprint",
        "unit",
        "quote",
        "reservation",
        "source",
    ] {
        let key = serde_json::to_string(field).unwrap();
        bad.push(format!(
            "{{{key}:{},{}",
            good[field],
            &good.to_string()[1..]
        ));
    }
    for (field, value) in [
        ("extra", json!(true)),
        ("id", json!(4)),
        ("account", json!([])),
        ("input_fingerprint", json!(true)),
        ("source", json!(3)),
        ("unit", json!("token")),
        ("quote", json!(-1)),
        ("quote", json!(1.5)),
        ("quote", json!("0")),
        ("reservation", json!(-1)),
        ("reservation", json!(1.5)),
        ("reservation", json!("7")),
        ("unit", json!({"CurrencyMinor":["USD",2]})),
        ("unit", json!({"CurrencyMinor":{"currency":"USD"}})),
        (
            "unit",
            json!({"CurrencyMinor":{"currency":"USD","decimals":2,"extra":0}}),
        ),
        ("unit", json!({"CurrencyMinor":{"currency":3,"decimals":2}})),
        (
            "unit",
            json!({"CurrencyMinor":{"currency":"USD","decimals":256}}),
        ),
    ] {
        let mut v = good.clone();
        v[field] = value;
        bad.push(v.to_string());
    }
    for unit in [
        r#"{"CurrencyMinor":{"currency":"USD","currency":"USD","decimals":2}}"#,
        r#"{"CurrencyMinor":{"currency":"USD","decimals":2,"decimals":2}}"#,
    ] {
        bad.push(good.to_string().replace("\"Token\"", unit));
    }
    bad.push(
        good.to_string()
            .replace("\"reservation\":7", "\"reservation\":18446744073709551616"),
    );
    for raw in bad {
        fs::write(&path, &raw).unwrap();
        let (out, v) = invoke(&args);
        assert_eq!(out.status.code(), Some(2), "{raw}: {v}");
        assert_eq!(v["errors"][0]["field_path"], "request", "{raw}: {v}");
        assert_eq!(v["results"], Value::Null);
        let app = Application::open(&db).unwrap();
        assert!(app.call_intent(" intent α ").unwrap().is_none());
        assert_eq!(app.budget_balance(" 预算 α ").unwrap(), baseline);
    }
    for (field, value, code) in [
        ("id", json!(""), 2),
        ("input_fingerprint", json!("x\0y"), 2),
        ("source", json!("s".repeat(4097)), 2),
        ("account", json!("absent"), 1),
        ("unit", json!("Task"), 2),
        (
            "unit",
            json!({"CurrencyMinor":{"currency":"usd","decimals":2}}),
            2,
        ),
        (
            "unit",
            json!({"CurrencyMinor":{"currency":"USD","decimals":10}}),
            2,
        ),
        ("quote", json!(8), 2),
        ("quote", json!(9223372036854775808_u64), 2),
        ("reservation", json!(9223372036854775808_u64), 2),
        ("reservation", json!(101), 2),
    ] {
        let mut v = good.clone();
        v[field] = value;
        fs::write(&path, v.to_string()).unwrap();
        let (out, result) = invoke(&args);
        assert_eq!(out.status.code(), Some(code), "{result}");
        assert_eq!(result["errors"][0]["field_path"], "budget");
        let app = Application::open(&db).unwrap();
        assert!(
            app.call_intent(v["id"].as_str().unwrap())
                .unwrap()
                .is_none()
        );
        assert_eq!(app.budget_balance(" 预算 α ").unwrap(), baseline);
    }
    fs::write(&path, good.to_string()).unwrap();
    let mut limited = args.clone();
    limited.extend([OsStr::new("--max-bytes"), OsStr::new("8")]);
    let (out, v) = invoke(&limited);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(v["errors"][0]["field_path"], "request");
    fs::remove_file(&path).unwrap();
    let (out, v) = invoke(&args);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(v["errors"][0]["field_path"], "request");
}

#[test]
fn intent_database_arguments_help_and_missing_ids() {
    let dir = tempfile::tempdir().unwrap();
    let (db, path, _) = intent_fixture(dir.path());
    let missing = dir.path().join("missing.sqlite");
    let empty = dir.path().join("empty.sqlite");
    fs::write(&empty, b"").unwrap();
    for (p, code, field) in [
        (missing.as_path(), 2, "db"),
        (dir.path(), 2, "db"),
        (empty.as_path(), 3, "budget"),
    ] {
        for (action, value) in [
            ("reserve", path.as_os_str()),
            ("intent", OsStr::new("absent")),
            ("cancel", OsStr::new("absent")),
        ] {
            let (out, v) = invoke(&budget_args(action, p.as_os_str(), value));
            assert_eq!(out.status.code(), Some(code), "{v}");
            assert_eq!(v["errors"][0]["field_path"], field);
            assert_eq!(v["command"], format!("budget {action}"));
        }
    }
    assert!(!missing.exists());
    assert_eq!(fs::read(&empty).unwrap(), b"");
    for id in ["absent", ""] {
        assert_eq!(local_intent(&db, id), Value::Null);
        let expected = Application::open(&db)
            .unwrap()
            .cancel_call(id)
            .unwrap_err()
            .to_string();
        let (out, v) = invoke(&budget_args("cancel", db.as_os_str(), OsStr::new(id)));
        assert_eq!(out.status.code(), Some(2), "{v}");
        assert_eq!(v["errors"][0]["message"], expected);
    }
    for (action, flag) in [
        ("reserve", "--request"),
        ("intent", "--intent-id"),
        ("cancel", "--intent-id"),
    ] {
        for tail in [vec![], vec!["--db", "x"], vec![flag, "x"]] {
            let mut args = vec![
                OsStr::new("--call-id"),
                OsStr::new("arg-id"),
                OsStr::new("budget"),
                OsStr::new(action),
            ];
            args.extend(tail.iter().map(OsStr::new));
            let (out, v) = invoke(&args);
            assert_eq!(out.status.code(), Some(2));
            assert_eq!(v["call_id"], "arg-id");
        }
        let (out, v) = invoke(&[
            OsStr::new("budget"),
            OsStr::new(action),
            OsStr::new("--help"),
            OsStr::new("--call-id"),
            OsStr::new("help-id"),
        ]);
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(v["call_id"], "help-id");
        assert!(v["results"]["text"].as_str().unwrap().contains(flag));
    }
}

#[cfg(any(unix, windows))]
#[test]
fn intent_mutating_stdout_failure_retains_commit_without_replay() {
    let dir = tempfile::tempdir().unwrap();
    let (db, path, request) = intent_fixture(dir.path());
    let status = Command::new(env!("CARGO_BIN_EXE_assetweft"))
        .args(budget_args("reserve", db.as_os_str(), path.as_os_str()))
        .stdout(broken_output())
        .stderr(broken_output())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(6));
    assert_eq!(
        local_intent(&db, " intent α "),
        json!({"request":request,"state":"PREPARING","cancelled":false,"charge":null})
    );
    let app = Application::open(&db).unwrap();
    let b = app.budget_balance(" 预算 α ").unwrap();
    assert_eq!((b.reserved, b.spent), (7, 0));
    assert!(app.call_receipts(" intent α ").unwrap().is_empty());
    drop(app);
    for (action, value) in [
        ("intent", OsStr::new(" intent α ")),
        ("cancel", OsStr::new(" intent α ")),
        ("cancel", OsStr::new("absent")),
    ] {
        let args = budget_args(action, db.as_os_str(), value);
        let (ordinary, _) = invoke(&args);
        let failed = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(&args)
            .stderr(broken_output())
            .output()
            .unwrap();
        assert_eq!(ordinary.stdout, failed.stdout);
        assert_eq!(ordinary.status.code(), failed.status.code());
    }
    assert_eq!(local_intent(&db, " intent α ")["state"], "RELEASED");
    assert_eq!(
        Application::open(&db)
            .unwrap()
            .budget_balance(" 预算 α ")
            .unwrap()
            .reserved,
        0
    );
}

fn candidate_invoke(args: &[&OsStr]) -> (Output, Value) {
    let utc = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let started = std::time::Instant::now();
    let (out, value) = invoke(args);
    let duration = started.elapsed().as_secs_f64();
    if let Some(directory) = std::env::var_os("AW033B_QUERY_RECEIPTS") {
        static SEQUENCE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let index = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let prefix = PathBuf::from(directory).join(format!("query-{index:04}"));
        fs::write(prefix.with_extension("stdout"), &out.stdout).unwrap();
        fs::write(prefix.with_extension("stderr"), &out.stderr).unwrap();
        fs::write(prefix.with_extension("json"), serde_json::to_vec(&json!({"utc_unix_seconds":utc,"duration_seconds":duration,"argv":args.iter().map(|a|a.to_string_lossy()).collect::<Vec<_>>(),"exit":out.status.code()})).unwrap()).unwrap();
    }
    (out, value)
}
fn candidate_fixture(
    dir: &Path,
    bad_proposed: bool,
) -> (PathBuf, Application, assetweft_core::candidate::Proposal) {
    use assetweft_core::{
        LockMode,
        candidate::{FixtureContract, Proposal},
        library::{AssetVersion, ComponentReference, ComponentRole, Presence},
    };
    let db = dir.join("候选 library.sqlite");
    let mut app = Application::open(&db).unwrap();
    app.initialize_library().unwrap();
    let glb = fs::read(fixture("mesh_only.glb")).unwrap();
    for id in ["mesh0", "mesh1", "mesh2", "base", "proposed", "other"] {
        let bad = bad_proposed && id == "mesh1";
        let bytes = if bad {
            b"invalid glb".as_slice()
        } else {
            glb.as_slice()
        };
        let version = AssetVersion {
            asset_id: if ["base", "proposed", "other"].contains(&id) {
                "character"
            } else {
                id
            }
            .into(),
            version_id: id.into(),
            content_hash: if bad {
                "c7c67ca4bc17258e6e674eb1769791d4d85333723b201d8fb1b84037c7c886bf"
            } else {
                "4658deaae78fb1ae6050baca75a8c098db6ccad311c74ba02097fceac01e45a5"
            }
            .into(),
            input_id: "fixture".into(),
            source: "TEST_FIXTURE".into(),
            authorization: "UNVERIFIED".into(),
            authorized: false,
            license_id: "UNKNOWN".into(),
            usage_scope: "TEST_ONLY".into(),
            source_format: "GLB".into(),
            contract_version: "1.0.0".into(),
            presence: Presence::Unknown,
        };
        app.commit_object(&version, &mut &bytes[..], bytes.len() as u64, 1024 * 1024)
            .unwrap();
    }
    for (owner, mesh) in [("base", "mesh0"), ("proposed", "mesh1"), ("other", "mesh2")] {
        app.add_component_reference(&ComponentReference {
            owner_version_id: owner.into(),
            role: ComponentRole::Mesh,
            component_version_id: mesh.into(),
        })
        .unwrap();
    }
    let contract = FixtureContract {
        contract_id: "TEST_FIXTURE_GLB_STRUCTURE_V1".into(),
        max_component_bytes: 1024 * 1024,
        required_quality: vec![],
    };
    app.initialize_candidate_current("base", &contract).unwrap();
    let proposal = Proposal {
        id: "candidate-1".into(),
        base_version: "base".into(),
        candidate_version: "proposed".into(),
        expected_revision: 0,
        editable: vec![ComponentRole::Mesh],
        lock_mode: LockMode::Byte,
        contract: Some(contract),
    };
    (db, app, proposal)
}
fn candidate_args<'a>(action: &'a str, db: &'a OsStr, id: &'a OsStr) -> Vec<&'a OsStr> {
    vec![
        OsStr::new("candidate"),
        OsStr::new(action),
        OsStr::new("--db"),
        db,
        OsStr::new("--candidate-id"),
        id,
    ]
}
fn candidate_query(db: &Path, action: &str, id: &str, expected: &Value) {
    let mut args = vec![OsStr::new("--call-id"), OsStr::new("correlation id 独立")];
    args.extend(candidate_args(action, db.as_os_str(), OsStr::new(id)));
    let (out, value) = candidate_invoke(&args);
    assert_eq!(out.status.code(), Some(0), "{value}");
    assert_eq!(value["status"], "PASS");
    assert_eq!(value["command"], format!("candidate {action}"));
    assert_eq!(value["call_id"], "correlation id 独立");
    assert_eq!(value["errors"], json!([]));
    assert!(out.stderr.is_empty());
    let (scope, key) = if action == "show" {
        ("stored_local_candidate_descriptor", "candidate")
    } else {
        ("stored_local_candidate_acceptance_receipt", "receipt")
    };
    assert_eq!(value["results"], json!({"scope": scope, key: expected}));
    let (again, _) = candidate_invoke(&args);
    assert_eq!(out.stdout, again.stdout);
}
#[test]
fn candidate_queries_preserve_descriptor_nulls_and_absence_without_attempts() {
    let dir = tempfile::tempdir().unwrap();
    let (db, app, mut proposal) = candidate_fixture(dir.path(), false);
    let mut app = app;
    proposal.contract = None;
    proposal.expected_revision = 17;
    let descriptor = app.propose_candidate(&proposal).unwrap();
    let current = app.candidate_current("character").unwrap();
    let value = serde_json::to_value(&descriptor).unwrap();
    assert_eq!(value["contract"], Value::Null);
    assert_eq!(value["expected_revision"], 17);
    assert_eq!(value["source"], "TEST_FIXTURE");
    assert!(
        value["not_run"]
            .as_array()
            .unwrap()
            .contains(&json!("QUALITY"))
    );
    app.close().unwrap();
    candidate_query(&db, "show", &proposal.id, &value);
    candidate_query(&db, "receipt", &proposal.id, &Value::Null);
    for action in ["show", "receipt"] {
        candidate_query(&db, action, "missing", &Value::Null);
    }
    let reopened = Application::open(&db).unwrap();
    assert_eq!(reopened.candidate(&proposal.id).unwrap(), Some(descriptor));
    assert_eq!(reopened.candidate_attempt(&proposal.id).unwrap(), None);
    assert_eq!(reopened.candidate_attempt("missing").unwrap(), None);
    assert_eq!(reopened.candidate("missing").unwrap(), None);
    assert_eq!(reopened.candidate_current("character").unwrap(), current);
}
#[test]
fn candidate_queries_preserve_all_terminal_receipts_and_historical_bytes() {
    use assetweft_core::candidate::Acceptance;
    for mode in ["NotRun", "NotRunReport", "Rejected", "Accepted", "Conflict"] {
        let dir = tempfile::tempdir().unwrap();
        let (db, mut app, mut proposal) = candidate_fixture(dir.path(), mode == "Rejected");
        if mode == "NotRun" {
            proposal.contract = None;
        }
        if mode == "NotRunReport" {
            proposal.contract.as_mut().unwrap().max_component_bytes = 512 * 1024;
        }
        if mode == "Conflict" {
            proposal.expected_revision = 1;
        }
        let descriptor = app.propose_candidate(&proposal).unwrap();
        let receipt = app.attempt_accept_candidate(&proposal.id).unwrap();
        assert!(matches!(
            (&receipt, mode),
            (Acceptance::NotRun { report: None, .. }, "NotRun")
                | (
                    Acceptance::NotRun {
                        report: Some(_),
                        ..
                    },
                    "NotRunReport"
                )
                | (Acceptance::Rejected { .. }, "Rejected")
                | (Acceptance::Accepted { .. }, "Accepted")
                | (Acceptance::Conflict { .. }, "Conflict")
        ));
        let mut records = vec![(proposal.id.clone(), descriptor, Some(receipt.clone()))];
        if mode == "Accepted" {
            let mut next = proposal.clone();
            next.id = "candidate-next".into();
            next.base_version = "proposed".into();
            next.candidate_version = "other".into();
            next.expected_revision = 1;
            let next_descriptor = app.propose_candidate(&next).unwrap();
            let next_receipt = app.attempt_accept_candidate(&next.id).unwrap();
            assert!(
                matches!(&next_receipt, Acceptance::Accepted { current, .. } if current.revision == 2)
            );
            records.push((next.id, next_descriptor, Some(next_receipt)));
        }
        let current = app.candidate_current("character").unwrap();
        let managed = app.managed_object("mesh0").unwrap().unwrap();
        app.close().unwrap();
        for (id, descriptor, receipt) in &records {
            candidate_query(&db, "show", id, &serde_json::to_value(descriptor).unwrap());
            candidate_query(&db, "receipt", id, &serde_json::to_value(receipt).unwrap());
        }
        let owned_object = dir.path().join("objects").join(&managed.content_hash);
        assert!(owned_object.is_file(), "{owned_object:?}");
        fs::remove_file(&owned_object).unwrap();
        for (id, descriptor, receipt) in &records {
            candidate_query(&db, "show", id, &serde_json::to_value(descriptor).unwrap());
            candidate_query(&db, "receipt", id, &serde_json::to_value(receipt).unwrap());
        }
        let reopened = Application::open(&db).unwrap();
        for (id, descriptor, receipt) in records {
            assert_eq!(reopened.candidate(&id).unwrap(), Some(descriptor));
            assert_eq!(reopened.candidate_attempt(&id).unwrap(), receipt);
        }
        assert_eq!(reopened.candidate_current("character").unwrap(), current);
        assert_eq!(reopened.candidate_attempt("missing").unwrap(), None);
        assert!(!owned_object.exists());
    }
}
#[test]
fn candidate_query_identity_database_and_grammar_errors_are_truthful() {
    let dir = tempfile::tempdir().unwrap();
    let (db, app, proposal) = candidate_fixture(dir.path(), false);
    for action in ["show", "receipt"] {
        for id in ["", " candidate-1", "candidate-1 ", "..", "invalid/id"] {
            let failure = if action == "show" {
                app.candidate(id).unwrap_err()
            } else {
                app.candidate_attempt(id).unwrap_err()
            };
            assert!(matches!(
                failure,
                assetweft_core::library::LibraryError::InvalidInput(_)
            ));
            let (out, value) =
                candidate_invoke(&candidate_args(action, db.as_os_str(), OsStr::new(id)));
            assert_eq!(out.status.code(), Some(2), "{value}");
            assert_eq!(value["status"], "INVALID_INPUT");
            assert_eq!(value["errors"][0]["message"], failure.to_string());
            assert_eq!(value["results"], Value::Null);
        }
        let missing = dir.path().join(format!("missing-{action}.sqlite"));
        let empty = dir.path().join(format!("empty-{action}.sqlite"));
        fs::write(&empty, []).unwrap();
        for (path, code, status) in [
            (missing.as_path(), 2, "INVALID_INPUT"),
            (dir.path(), 2, "INVALID_INPUT"),
            (empty.as_path(), 3, "INPUT_INCOMPLETE"),
        ] {
            let (out, value) = candidate_invoke(&candidate_args(
                action,
                path.as_os_str(),
                OsStr::new("candidate-1"),
            ));
            assert_eq!(out.status.code(), Some(code), "{value}");
            assert_eq!(value["status"], status);
        }
        assert!(!missing.exists());
        assert_eq!(fs::metadata(&empty).unwrap().len(), 0);
        let empty_app = Application::open(&empty).unwrap();
        assert!(matches!(
            empty_app.candidate("candidate-1"),
            Err(assetweft_core::library::LibraryError::Uninitialized)
        ));
        for tail in [
            vec![],
            vec![OsStr::new("--db"), db.as_os_str()],
            vec![OsStr::new("--candidate-id"), OsStr::new("candidate-1")],
            vec![
                OsStr::new("--db"),
                db.as_os_str(),
                OsStr::new("--candidate-id"),
                OsStr::new("candidate-1"),
                OsStr::new("--extra"),
            ],
        ] {
            let mut args = vec![
                OsStr::new("--call-id"),
                OsStr::new("syntax-id"),
                OsStr::new("candidate"),
                OsStr::new(action),
            ];
            args.extend(tail);
            let (out, value) = candidate_invoke(&args);
            assert_eq!(out.status.code(), Some(2));
            assert_eq!(value["call_id"], "syntax-id");
        }
        let (out, help) = candidate_invoke(&[
            OsStr::new("--call-id"),
            OsStr::new("help-id"),
            OsStr::new("candidate"),
            OsStr::new(action),
            OsStr::new("--help"),
        ]);
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(help["status"], "HELP");
        assert_eq!(help["call_id"], "help-id");
        let text = help["results"]["text"].as_str().unwrap();
        assert!(text.contains("stored") && text.contains("--candidate-id"));
    }
    for args in [
        vec![OsStr::new("candidate")],
        vec![OsStr::new("candidate"), OsStr::new("accept")],
    ] {
        assert_eq!(candidate_invoke(&args).0.status.code(), Some(2));
    }
    assert_eq!(app.candidate(&proposal.id).unwrap(), None);
    assert_eq!(app.candidate_attempt(&proposal.id).unwrap(), None);
}
#[cfg(any(unix, windows))]
#[test]
fn candidate_query_protocol_survives_broken_streams() {
    let dir = tempfile::tempdir().unwrap();
    let (db, app, proposal) = candidate_fixture(dir.path(), false);
    app.close().unwrap();
    for action in ["show", "receipt"] {
        let mut args = vec![OsStr::new("--call-id"), OsStr::new("query-id")];
        args.extend(candidate_args(
            action,
            db.as_os_str(),
            OsStr::new(&proposal.id),
        ));
        let ordinary = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(ordinary.status.code(), Some(0));
        let broken_err = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(&args)
            .stderr(broken_output())
            .output()
            .unwrap();
        assert_eq!(broken_err.status.code(), ordinary.status.code());
        assert_eq!(broken_err.stdout, ordinary.stdout);
        for suffix in [vec![], vec![OsStr::new("--extra")]] {
            let mut argv = args.clone();
            argv.extend(suffix);
            let status = Command::new(env!("CARGO_BIN_EXE_assetweft"))
                .args(&argv)
                .stdout(broken_output())
                .stderr(broken_output())
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(6));
        }
    }
    let reopened = Application::open(&db).unwrap();
    assert_eq!(reopened.candidate(&proposal.id).unwrap(), None);
    assert_eq!(reopened.candidate_attempt(&proposal.id).unwrap(), None);
    assert_eq!(
        reopened
            .candidate_current("character")
            .unwrap()
            .unwrap()
            .revision,
        0
    );
}

fn catalog_args<'a>(action: &'a str, db: &'a OsStr, id: &'a OsStr) -> Vec<&'a OsStr> {
    vec![
        OsStr::new("catalog"),
        OsStr::new(action),
        OsStr::new("--db"),
        db,
        OsStr::new(if action == "current" {
            "--asset-id"
        } else {
            "--version-id"
        }),
        id,
    ]
}
fn catalog_query(db: &Path, action: &str, id: &str, expected: Value) {
    let mut args = vec![
        OsStr::new("--call-id"),
        OsStr::new("catalog correlation 独立"),
    ];
    args.extend(catalog_args(action, db.as_os_str(), OsStr::new(id)));
    let (out, value) = candidate_invoke(&args);
    assert_eq!(out.status.code(), Some(0), "{value}");
    assert_eq!(
        value,
        json!({"schema_version":1,"command":format!("catalog {action}"),
        "call_id":"catalog correlation 独立","status":"PASS","errors":[],"results":expected})
    );
    assert!(out.stderr.is_empty());
    assert_eq!(out.stdout, candidate_invoke(&args).0.stdout);
}
fn catalog_version(
    id: &str,
    presence: assetweft_core::library::Presence,
) -> assetweft_core::library::AssetVersion {
    assetweft_core::library::AssetVersion {
        asset_id: "metadata-asset".into(),
        version_id: id.into(),
        content_hash: "a".repeat(64),
        input_id: "input 原样".into(),
        source: " declared source ".into(),
        authorization: "LOCAL DECLARATION ONLY".into(),
        authorized: true,
        license_id: "unverified-license".into(),
        usage_scope: "TEST ONLY".into(),
        source_format: "declared-format".into(),
        contract_version: "1.0.0".into(),
        presence,
    }
}
#[test]
fn catalog_version_projects_every_metadata_field_and_presence_without_bytes() {
    use assetweft_core::library::Presence;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("目录 metadata library.sqlite");
    let mut app = Application::open(&db).unwrap();
    app.initialize_library().unwrap();
    let rows = [
        catalog_version("元 数据-v1", Presence::Unknown),
        catalog_version("missing-bytes", Presence::Missing),
    ];
    for row in &rows {
        app.add_asset_version(row).unwrap();
    }
    app.close().unwrap();
    for (row, presence) in rows.iter().zip(["UNKNOWN", "MISSING"]) {
        catalog_query(
            &db,
            "version",
            &row.version_id,
            json!({"scope":"stored_local_asset_version_metadata","version":{
            "asset_id":"metadata-asset","version_id":row.version_id,"content_hash":"a".repeat(64),
            "input_id":"input 原样","source":" declared source ","authorization":"LOCAL DECLARATION ONLY",
            "authorized":true,"license_id":"unverified-license","usage_scope":"TEST ONLY",
            "source_format":"declared-format","contract_version":"1.0.0","presence":presence}}),
        );
    }
    catalog_query(
        &db,
        "version",
        "absent",
        json!({"scope":"stored_local_asset_version_metadata","version":null}),
    );
    let mut reopened = Application::open(&db).unwrap();
    for row in rows {
        assert_eq!(reopened.asset_version(&row.version_id).unwrap(), Some(row));
    }
    assert_eq!(reopened.asset_version("absent").unwrap(), None);
    assert!(!dir.path().join("objects").exists());
}
#[test]
fn catalog_references_preserve_all_roles_exact_versions_and_owner_order() {
    use assetweft_core::library::{ComponentReference, ComponentRole, Presence};
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("目录 references.sqlite");
    let mut app = Application::open(&db).unwrap();
    app.initialize_library().unwrap();
    let roles = [
        ComponentRole::Vfx,
        ComponentRole::Rig,
        ComponentRole::Motion,
        ComponentRole::Mesh,
        ComponentRole::Materials,
        ComponentRole::Dialogue,
        ComponentRole::Audio,
        ComponentRole::Mesh,
    ];
    app.add_asset_version(&catalog_version("owner", Presence::Unknown))
        .unwrap();
    for (i, role) in roles.into_iter().enumerate() {
        let id = format!("component-{i}");
        app.add_asset_version(&catalog_version(&id, Presence::Missing))
            .unwrap();
        app.add_component_reference(&ComponentReference {
            owner_version_id: "owner".into(),
            role,
            component_version_id: id,
        })
        .unwrap();
    }
    let before = app.component_references("owner").unwrap();
    assert_eq!(
        before
            .iter()
            .map(|r| r.component_version_id.as_str())
            .collect::<Vec<_>>(),
        [
            "component-6",
            "component-5",
            "component-4",
            "component-3",
            "component-7",
            "component-2",
            "component-1",
            "component-0"
        ]
    );
    app.close().unwrap();
    catalog_query(
        &db,
        "references",
        "owner",
        json!({"scope":"stored_local_exact_component_references","references":[
        {"owner_version_id":"owner","role":"audio","component_version_id":"component-6"},
        {"owner_version_id":"owner","role":"dialogue","component_version_id":"component-5"},
        {"owner_version_id":"owner","role":"materials","component_version_id":"component-4"},
        {"owner_version_id":"owner","role":"mesh","component_version_id":"component-3"},
        {"owner_version_id":"owner","role":"mesh","component_version_id":"component-7"},
        {"owner_version_id":"owner","role":"motion","component_version_id":"component-2"},
        {"owner_version_id":"owner","role":"rig","component_version_id":"component-1"},
        {"owner_version_id":"owner","role":"vfx","component_version_id":"component-0"}]}),
    );
    for id in ["absent", "component-0"] {
        catalog_query(
            &db,
            "references",
            id,
            json!({"scope":"stored_local_exact_component_references","references":[]}),
        );
    }
    let mut reopened = Application::open(&db).unwrap();
    assert_eq!(reopened.component_references("owner").unwrap(), before);
    assert_eq!(
        reopened.asset_version("owner").unwrap(),
        Some(catalog_version("owner", Presence::Unknown))
    );
    assert!(reopened.component_references("absent").unwrap().is_empty());
}
#[test]
fn catalog_current_preserves_real_stored_head_contract_after_cas_disappears() {
    use assetweft_core::candidate::Acceptance;
    let dir = tempfile::tempdir().unwrap();
    let (db, mut app, proposal) = candidate_fixture(dir.path(), false);
    let initial = app.candidate_current("character").unwrap();
    assert_eq!(initial.as_ref().unwrap().revision, 0);
    catalog_query(
        &db,
        "current",
        "character",
        json!({"scope":"stored_local_candidate_current","current":{
        "asset_id":"character","version_id":"base","revision":0,"contract":{
            "contract_id":"TEST_FIXTURE_GLB_STRUCTURE_V1","max_component_bytes":1048576,"required_quality":[]}}}),
    );
    let descriptor = app.propose_candidate(&proposal).unwrap();
    let receipt = app.attempt_accept_candidate(&proposal.id).unwrap();
    assert!(matches!(&receipt, Acceptance::Accepted { current, .. } if current.revision == 1));
    let head = app.candidate_current("character").unwrap();
    let version = app.asset_version("mesh0").unwrap();
    let refs = app.component_references("base").unwrap();
    let managed = app.managed_object("mesh0").unwrap().unwrap();
    app.close().unwrap();
    for remove in [false, true] {
        if remove {
            let owned = dir.path().join("objects").join(&managed.content_hash);
            assert!(owned.is_file());
            fs::remove_file(&owned).unwrap();
        }
        catalog_query(
            &db,
            "current",
            "character",
            json!({"scope":"stored_local_candidate_current","current":{
            "asset_id":"character","version_id":"proposed","revision":1,"contract":{
                "contract_id":"TEST_FIXTURE_GLB_STRUCTURE_V1","max_component_bytes":1048576,"required_quality":[]}}}),
        );
        catalog_query(
            &db,
            "current",
            "absent",
            json!({"scope":"stored_local_candidate_current","current":null}),
        );
        let mut reopened = Application::open(&db).unwrap();
        assert_eq!(reopened.candidate_current("character").unwrap(), head);
        assert_eq!(reopened.asset_version("mesh0").unwrap(), version);
        assert_eq!(reopened.component_references("base").unwrap(), refs);
        assert_eq!(
            reopened.candidate(&proposal.id).unwrap(),
            Some(descriptor.clone())
        );
        assert_eq!(
            reopened.candidate_attempt(&proposal.id).unwrap(),
            Some(receipt.clone())
        );
        reopened.close().unwrap();
    }
}
#[test]
fn catalog_identity_database_grammar_and_help_follow_existing_owner_policies() {
    use assetweft_core::library::LibraryError;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("目录 errors.sqlite");
    let mut app = Application::open(&db).unwrap();
    app.initialize_library().unwrap();
    for action in ["version", "references", "current"] {
        for id in ["", ".", "..", "invalid/id", " space 元 "] {
            let result = match action {
                "version" => app.asset_version(id).map(|v| v.is_none()),
                "references" => app.component_references(id).map(|v| v.is_empty()),
                _ => app.candidate_current(id).map(|v| v.is_none()),
            };
            let (out, value) =
                candidate_invoke(&catalog_args(action, db.as_os_str(), OsStr::new(id)));
            match result {
                Err(e @ LibraryError::InvalidInput(_)) => {
                    assert_eq!(out.status.code(), Some(2), "{value}");
                    assert_eq!(value["status"], "INVALID_INPUT");
                    assert_eq!(
                        value["errors"][0],
                        json!({"field_path":"catalog","code":"INVALID_INPUT","message":e.to_string()})
                    );
                    assert_eq!(value["results"], Value::Null);
                }
                Ok(true) => {
                    assert_eq!(out.status.code(), Some(0), "{value}");
                }
                other => panic!("unexpected fixture owner result: {other:?}"),
            }
        }
        let missing = dir.path().join(format!("missing-{action}.sqlite"));
        let empty = dir.path().join(format!("empty-{action}.sqlite"));
        fs::write(&empty, []).unwrap();
        for (path, code, status, field) in [
            (missing.as_path(), 2, "INVALID_INPUT", "db"),
            (dir.path(), 2, "INVALID_INPUT", "db"),
            (empty.as_path(), 3, "INPUT_INCOMPLETE", "catalog"),
        ] {
            let (out, value) =
                candidate_invoke(&catalog_args(action, path.as_os_str(), OsStr::new("valid")));
            assert_eq!(out.status.code(), Some(code), "{value}");
            assert_eq!(value["status"], status);
            assert_eq!(value["errors"][0]["field_path"], field);
            assert_eq!(value["results"], Value::Null);
        }
        assert!(!missing.exists());
        assert_eq!(fs::metadata(&empty).unwrap().len(), 0);
        let mut empty_app = Application::open(&empty).unwrap();
        assert!(matches!(
            empty_app.asset_version("valid"),
            Err(LibraryError::Uninitialized)
        ));
        let key = if action == "current" {
            "--asset-id"
        } else {
            "--version-id"
        };
        for tail in [
            vec![],
            vec![OsStr::new("--db"), db.as_os_str()],
            vec![OsStr::new(key), OsStr::new("valid")],
        ] {
            let mut args = vec![
                OsStr::new("--call-id"),
                OsStr::new("syntax-id"),
                OsStr::new("catalog"),
                OsStr::new(action),
            ];
            args.extend(tail);
            let (out, value) = candidate_invoke(&args);
            assert_eq!(out.status.code(), Some(2));
            assert_eq!(value["call_id"], "syntax-id");
        }
        for bad in ["--extra", "--max-bytes=0", "--max-bytes=16777217"] {
            let mut args = vec![OsStr::new("--call-id"), OsStr::new("syntax-id")];
            args.extend(catalog_args(action, db.as_os_str(), OsStr::new("valid")));
            args.push(OsStr::new(bad));
            let (out, value) = candidate_invoke(&args);
            assert_eq!(out.status.code(), Some(2));
            assert_eq!(value["call_id"], "syntax-id");
        }
        let (out, help) = candidate_invoke(&[
            OsStr::new("--call-id"),
            OsStr::new("help-id"),
            OsStr::new("catalog"),
            OsStr::new(action),
            OsStr::new("--help"),
        ]);
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(help["status"], "HELP");
        assert_eq!(help["call_id"], "help-id");
        let text = help["results"]["text"].as_str().unwrap();
        assert!(text.contains("stored") && text.contains(key));
    }
    for tail in [vec![], vec![OsStr::new("show")], vec![OsStr::new("write")]] {
        let mut args = vec![OsStr::new("catalog")];
        args.extend(tail);
        assert_eq!(candidate_invoke(&args).0.status.code(), Some(2));
    }
}
#[cfg(any(unix, windows))]
#[test]
fn catalog_query_protocol_preserves_broken_stream_exit_and_json() {
    let dir = tempfile::tempdir().unwrap();
    let (db, app, _) = candidate_fixture(dir.path(), false);
    app.close().unwrap();
    for (action, id) in [
        ("version", "base"),
        ("references", "base"),
        ("current", "character"),
    ] {
        for invalid in [false, true] {
            let mut args = vec![OsStr::new("--call-id"), OsStr::new("stream-id")];
            args.extend(catalog_args(action, db.as_os_str(), OsStr::new(id)));
            if invalid {
                args.push(OsStr::new("--extra"));
            }
            let ordinary = Command::new(env!("CARGO_BIN_EXE_assetweft"))
                .args(&args)
                .output()
                .unwrap();
            assert_eq!(ordinary.status.code(), Some(if invalid { 2 } else { 0 }));
            let broken_err = Command::new(env!("CARGO_BIN_EXE_assetweft"))
                .args(&args)
                .stderr(broken_output())
                .output()
                .unwrap();
            assert_eq!(broken_err.status.code(), ordinary.status.code());
            assert_eq!(broken_err.stdout, ordinary.stdout);
            let value: Value = serde_json::from_slice(&broken_err.stdout).unwrap();
            assert_eq!(value["call_id"], "stream-id");
            for both in [false, true] {
                let mut command = Command::new(env!("CARGO_BIN_EXE_assetweft"));
                command.args(&args).stdout(broken_output());
                if both {
                    command.stderr(broken_output());
                }
                assert_eq!(command.output().unwrap().status.code(), Some(6));
            }
        }
    }
    let mut reopened = Application::open(&db).unwrap();
    assert_eq!(
        reopened
            .candidate_current("character")
            .unwrap()
            .unwrap()
            .revision,
        0
    );
    assert_eq!(reopened.component_references("base").unwrap().len(), 1);
}

fn candidate_write_args<'a>(action: &'a str, db: &'a Path, input: &'a Path) -> Vec<&'a OsStr> {
    let mut args = vec![
        OsStr::new("candidate"),
        OsStr::new(action),
        OsStr::new("--db"),
        db.as_os_str(),
    ];
    if action == "initialize" {
        args.extend([
            OsStr::new("--version-id"),
            OsStr::new("headless-base"),
            OsStr::new("--contract"),
        ]);
    } else {
        args.push(OsStr::new("--proposal"));
    }
    args.push(input.as_os_str());
    args
}
fn candidate_write_fixture(dir: &Path) -> (PathBuf, Application, PathBuf, PathBuf, Value) {
    use assetweft_core::library::{ComponentReference, ComponentRole};
    let (db, mut app, proposal) = candidate_fixture(dir, false);
    let mut version = app.asset_version("base").unwrap().unwrap();
    version.asset_id = "headless".into();
    version.version_id = "headless-base".into();
    let bytes = fs::read(fixture("mesh_only.glb")).unwrap();
    app.commit_object(
        &version,
        &mut bytes.as_slice(),
        bytes.len() as u64,
        1024 * 1024,
    )
    .unwrap();
    app.add_component_reference(&ComponentReference {
        owner_version_id: "headless-base".into(),
        role: ComponentRole::Mesh,
        component_version_id: "mesh0".into(),
    })
    .unwrap();
    assert_eq!(app.candidate_current("headless").unwrap(), None);
    let contract = dir.join("合同 contract.json");
    fs::write(
        &contract,
        serde_json::to_vec(proposal.contract.as_ref().unwrap()).unwrap(),
    )
    .unwrap();
    let value = json!({"id":proposal.id,"base_version":proposal.base_version,"candidate_version":proposal.candidate_version,"expected_revision":proposal.expected_revision,"editable":["mesh"],"lock_mode":"BYTE","contract":proposal.contract});
    let path = dir.join("暂存 proposal.json");
    fs::write(&path, value.to_string()).unwrap();
    (db, app, contract, path, value)
}
#[test]
fn candidate_write_initialize_absent_head_exact_current_and_core_repeat() {
    let dir = tempfile::tempdir().unwrap();
    let (db, app, contract, _, _) = candidate_write_fixture(dir.path());
    assert_eq!(app.candidate_current("headless").unwrap(), None);
    let before = fs::read(&contract).unwrap();
    let mut args = candidate_write_args("initialize", &db, &contract);
    args.extend([OsStr::new("--call-id"), OsStr::new("init α")]);
    let (out, value) = candidate_invoke(&args);
    assert_eq!(
        out.status.code(),
        Some(0),
        "initialize must create absent fixture head: {value}"
    );
    assert_eq!(value["command"], "candidate initialize");
    assert_eq!(value["call_id"], "init α");
    assert_eq!(value["status"], "PASS");
    let mut reopened = Application::open(&db).unwrap();
    let current = reopened.candidate_current("headless").unwrap().unwrap();
    assert_eq!(current.version_id, "headless-base");
    assert_eq!(current.revision, 0);
    assert_eq!(
        value["results"],
        json!({"scope":"initialized_local_fixture_current","current":current})
    );
    let expected = reopened
        .initialize_candidate_current("headless-base", &current.contract)
        .unwrap();
    let (again, _) = candidate_invoke(&args);
    assert_eq!(again.stdout, out.stdout);
    assert_eq!(
        Application::open(&db)
            .unwrap()
            .candidate_current("headless")
            .unwrap(),
        Some(expected)
    );
    assert_eq!(fs::read(&contract).unwrap(), before);
}
#[test]
fn candidate_write_propose_exact_staging_without_acceptance_or_head_advance() {
    use assetweft_core::library::{ComponentReference, ComponentRole};
    let dir = tempfile::tempdir().unwrap();
    let (db, mut app, _, path, mut wire) = candidate_write_fixture(dir.path());
    let head = app.candidate_current("character").unwrap();
    let bytes = fs::read(fixture("mesh_only.glb")).unwrap();
    for owner in ["roles-base", "roles-proposed"] {
        let mut version = app.asset_version("base").unwrap().unwrap();
        version.asset_id = "roles-owner".into();
        version.version_id = owner.into();
        app.commit_object(
            &version,
            &mut bytes.as_slice(),
            bytes.len() as u64,
            1024 * 1024,
        )
        .unwrap();
        for role in [
            ComponentRole::Mesh,
            ComponentRole::Materials,
            ComponentRole::Rig,
            ComponentRole::Motion,
            ComponentRole::Vfx,
            ComponentRole::Audio,
            ComponentRole::Dialogue,
        ] {
            app.add_component_reference(&ComponentReference {
                owner_version_id: owner.into(),
                role,
                component_version_id: "mesh0".into(),
            })
            .unwrap();
        }
    }
    wire["base_version"] = json!("roles-base");
    wire["candidate_version"] = json!("roles-proposed");
    wire["editable"] = json!([
        "mesh",
        "materials",
        "rig",
        "motion",
        "vfx",
        "audio",
        "dialogue"
    ]);
    for (index, contract) in [Some(wire["contract"].clone()), Some(Value::Null), None]
        .into_iter()
        .enumerate()
    {
        wire["id"] = json!(format!("wire-{index}"));
        if let Some(contract) = contract {
            wire["contract"] = contract;
        } else {
            wire.as_object_mut().unwrap().remove("contract");
        }
        fs::write(&path, wire.to_string()).unwrap();
        let mut args = candidate_write_args("propose", &db, &path);
        args.extend([OsStr::new("--call-id"), OsStr::new("stage α")]);
        let (out, value) = candidate_invoke(&args);
        assert_eq!(
            out.status.code(),
            Some(0),
            "propose must stage existing managed byte fixture: {value}"
        );
        assert_eq!(value["status"], "PASS");
        assert_eq!(value["command"], "candidate propose");
        assert_eq!(value["call_id"], "stage α");
        let reopened = Application::open(&db).unwrap();
        let id = wire["id"].as_str().unwrap();
        let descriptor = reopened.candidate(id).unwrap().unwrap();
        assert_eq!(
            descriptor.editable,
            [
                "mesh",
                "materials",
                "rig",
                "motion",
                "vfx",
                "audio",
                "dialogue"
            ]
        );
        assert_eq!(descriptor.expected_revision, 0);
        assert_eq!(descriptor.source, "TEST_FIXTURE");
        assert_eq!(descriptor.scope, "MANAGED_COMPONENT_BYTE_STAGING_ONLY");
        assert_eq!(
            serde_json::to_value(&descriptor.contract).unwrap(),
            wire.get("contract").cloned().unwrap_or(Value::Null)
        );
        assert_eq!(
            value["results"],
            json!({"scope":"staged_local_byte_fixture_candidate","candidate":descriptor})
        );
        assert_eq!(candidate_invoke(&args).0.stdout, out.stdout);
        candidate_query(&db, "show", id, &serde_json::to_value(&descriptor).unwrap());
        candidate_query(&db, "receipt", id, &Value::Null);
        assert_eq!(reopened.candidate_attempt(id).unwrap(), None);
        assert_eq!(reopened.candidate_current("character").unwrap(), head);
        catalog_query(
            &db,
            "current",
            "character",
            json!({"scope":"stored_local_candidate_current","current":head}),
        );
    }
}
#[test]
fn candidate_write_strict_json_shapes_numbers_and_bounds_leave_no_mutations() {
    let dir = tempfile::tempdir().unwrap();
    let (db, app, contract, path, good) = candidate_write_fixture(dir.path());
    let head = app.candidate_current("character").unwrap();
    let contract_good = fs::read_to_string(&contract).unwrap();
    let mut bad_contracts = vec![
        "[]".to_owned(),
        "[\"TEST_FIXTURE_GLB_STRUCTURE_V1\",1024,[]]".into(),
        "null".into(),
        "{}".into(),
        "{broken".into(),
        format!("{contract_good} null"),
        contract_good.replacen('{', "{\"extra\":0,", 1),
        contract_good.replacen('{', "{\"contract_id\":\"duplicate\",", 1),
    ];
    for (key, value) in [
        ("max_component_bytes", json!(-1)),
        ("max_component_bytes", json!(1.0)),
        ("max_component_bytes", json!("1024")),
        ("max_component_bytes", json!(null)),
        ("required_quality", json!([1])),
        ("contract_id", json!(1)),
    ] {
        let mut c: Value = serde_json::from_str(&contract_good).unwrap();
        c[key] = value;
        bad_contracts.push(c.to_string());
    }
    bad_contracts.push("{\"contract_id\":\"x\",\"max_component_bytes\":18446744073709551616,\"required_quality\":[]}".into());
    for raw in &bad_contracts {
        fs::write(&contract, raw).unwrap();
        let (out, v) = candidate_invoke(&candidate_write_args("initialize", &db, &contract));
        assert_eq!(out.status.code(), Some(2), "{raw}: {v}");
        assert_eq!(v["command"], "candidate initialize");
        assert_eq!(v["errors"][0]["field_path"], "contract", "{v}");
        assert_eq!(app.candidate_current("headless").unwrap(), None);
    }
    let mut bad = vec![
        "[]".to_owned(),
        "[\"x\",\"base\",\"proposed\",0,[\"mesh\"],\"BYTE\",null]".into(),
        "null".into(),
        "{}".into(),
        "{broken".into(),
        format!("{good} {{}}"),
        good.to_string().replacen('{', "{\"extra\":0,", 1),
        good.to_string().replacen('{', "{\"id\":\"duplicate\",", 1),
    ];
    for key in [
        "id",
        "base_version",
        "candidate_version",
        "expected_revision",
        "editable",
        "lock_mode",
    ] {
        let mut p = good.clone();
        p.as_object_mut().unwrap().remove(key);
        bad.push(p.to_string());
    }
    for (key, value) in [
        ("id", json!(1)),
        ("expected_revision", json!(1.0)),
        ("expected_revision", json!("0")),
        ("expected_revision", json!(null)),
        ("editable", json!("mesh")),
        ("editable", json!(["Mesh"])),
        ("editable", json!([" mesh"])),
        ("editable", json!([1])),
        ("editable", json!([{ "mesh":null }])),
        ("lock_mode", json!("Byte")),
        ("lock_mode", json!("NOT_DEFINED")),
        ("lock_mode", json!(null)),
        ("lock_mode", json!({"BYTE":null})),
    ] {
        let mut p = good.clone();
        p[key] = value;
        bad.push(p.to_string());
    }
    bad.push(good.to_string().replace(
        "\"expected_revision\":0",
        "\"expected_revision\":9223372036854775808",
    ));
    for raw in bad_contracts.iter().filter(|raw| raw.as_str() != "null") {
        let mut p = good.clone();
        p.as_object_mut().unwrap().remove("contract");
        let prefix = p.to_string();
        bad.push(format!(
            "{},\"contract\":{raw}}}",
            &prefix[..prefix.len() - 1]
        ));
    }
    bad.push(
        good.to_string()
            .replace("\"contract\":{", "\"contract\":{\"max_component_bytes\":1,"),
    );
    bad.push(good.to_string().replace(
        "\"expected_revision\":0",
        "\"expected_revision\":-9223372036854775809",
    ));
    bad.push(good.to_string().replacen('{', "{\"contract\":null,", 1));
    for raw in &bad {
        fs::write(&path, raw).unwrap();
        let (out, v) = candidate_invoke(&candidate_write_args("propose", &db, &path));
        assert_eq!(out.status.code(), Some(2), "{raw}: {v}");
        assert_eq!(v["command"], "candidate propose");
        assert_eq!(v["errors"][0]["field_path"], "proposal", "{raw}: {v}");
        assert_eq!(app.candidate("candidate-1").unwrap(), None);
    }
    for (action, input, raw, field) in [
        ("initialize", &contract, contract_good, "contract"),
        ("propose", &path, good.to_string(), "proposal"),
    ] {
        fs::write(input, &raw).unwrap();
        let limit = (raw.len() - 1).to_string();
        let mut args = candidate_write_args(action, &db, input);
        args.extend([OsStr::new("--max-bytes"), OsStr::new(&limit)]);
        let (out, v) = candidate_invoke(&args);
        assert_eq!(out.status.code(), Some(2), "{v}");
        assert_eq!(v["errors"][0]["field_path"], field);
        let limit = raw.len().to_string();
        let mut args = candidate_write_args(action, &db, input);
        args.extend([OsStr::new("--max-bytes"), OsStr::new(&limit)]);
        assert_eq!(candidate_invoke(&args).0.status.code(), Some(0));
    }
    assert_eq!(app.candidate_current("character").unwrap(), head);
    assert_eq!(app.candidate_attempt("candidate-1").unwrap(), None);
}
#[test]
fn candidate_write_core_policy_is_preserved_verbatim_without_partial_changes() {
    use assetweft_core::{LockMode, candidate::FixtureContract};
    let dir = tempfile::tempdir().unwrap();
    let (db, mut app, contract, path, good) = candidate_write_fixture(dir.path());
    let (_, _, mut proposal) = candidate_fixture(
        &{
            let d = dir.path().join("comparison");
            fs::create_dir(&d).unwrap();
            d
        },
        false,
    );
    for (key, value) in [
        ("id", json!(" candidate-1")),
        ("expected_revision", json!(-1)),
        ("expected_revision", json!(i64::MAX)),
        ("editable", json!([])),
        ("editable", json!(["mesh", "mesh"])),
        ("lock_mode", json!("SEMANTIC")),
        ("base_version", json!("absent")),
    ] {
        let mut wire = good.clone();
        wire[key] = value.clone();
        proposal.id = wire["id"].as_str().unwrap().into();
        proposal.base_version = wire["base_version"].as_str().unwrap().into();
        proposal.expected_revision = wire["expected_revision"].as_i64().unwrap();
        proposal.editable = wire["editable"]
            .as_array()
            .unwrap()
            .iter()
            .map(|_| assetweft_core::library::ComponentRole::Mesh)
            .collect();
        proposal.lock_mode = if wire["lock_mode"] == "SEMANTIC" {
            LockMode::Semantic
        } else {
            LockMode::Byte
        };
        let failure = app.propose_candidate(&proposal).unwrap_err();
        fs::write(&path, wire.to_string()).unwrap();
        let (out, v) = candidate_invoke(&candidate_write_args("propose", &db, &path));
        assert_eq!(out.status.code(), Some(2), "{v}");
        assert_eq!(v["errors"][0]["message"], failure.to_string());
        assert_eq!(v["errors"][0]["field_path"], "candidate");
    }
    let c: FixtureContract = serde_json::from_slice(&fs::read(&contract).unwrap()).unwrap();
    for (version, change) in [
        (" headless-base", None),
        ("missing", None),
        (
            "headless-base",
            Some(json!({"contract_id":"other","max_component_bytes":1024,"required_quality":[]})),
        ),
        (
            "headless-base",
            Some(
                json!({"contract_id":"TEST_FIXTURE_GLB_STRUCTURE_V1","max_component_bytes":1,"required_quality":[]}),
            ),
        ),
        (
            "headless-base",
            Some(
                json!({"contract_id":"TEST_FIXTURE_GLB_STRUCTURE_V1","max_component_bytes":1024,"required_quality":["quality"]}),
            ),
        ),
    ] {
        let declared: FixtureContract = change
            .map(|v| serde_json::from_value(v).unwrap())
            .unwrap_or(c.clone());
        let error = app
            .initialize_candidate_current(version, &declared)
            .unwrap_err();
        fs::write(&contract, serde_json::to_vec(&declared).unwrap()).unwrap();
        let mut args = candidate_write_args("initialize", &db, &contract);
        args[5] = OsStr::new(version);
        let (out, v) = candidate_invoke(&args);
        assert_eq!(out.status.code(), Some(2), "{v}");
        assert_eq!(v["errors"][0]["message"], error.to_string());
        assert_eq!(v["errors"][0]["field_path"], "candidate");
    }
    assert_eq!(app.candidate_current("headless").unwrap(), None);
    assert_eq!(app.candidate("candidate-1").unwrap(), None);
}
#[test]
fn candidate_write_database_paths_help_grammar_and_input_precedence() {
    let dir = tempfile::tempdir().unwrap();
    let (db, app, contract, path, _) = candidate_write_fixture(dir.path());
    let missing = dir.path().join("absent.sqlite");
    let empty = dir.path().join("empty.sqlite");
    fs::write(&empty, []).unwrap();
    for (action, input, field) in [
        ("initialize", &contract, "contract"),
        ("propose", &path, "proposal"),
    ] {
        let (out, v) = candidate_invoke(&[
            OsStr::new("--call-id"),
            OsStr::new("help-id"),
            OsStr::new("candidate"),
            OsStr::new(action),
            OsStr::new("--help"),
        ]);
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(v["call_id"], "help-id");
        assert_eq!(v["status"], "HELP");
        assert!(v["results"].to_string().contains(&format!("--{field}")));
        for p in [&missing, &dir.path().to_path_buf()] {
            let (out, v) = candidate_invoke(&candidate_write_args(action, p, input));
            assert_eq!(out.status.code(), Some(2));
            assert_eq!(v["errors"][0]["field_path"], "db");
        }
        let (out, v) = candidate_invoke(&candidate_write_args(action, &empty, input));
        assert_eq!(out.status.code(), Some(3), "{v}");
        assert_eq!(fs::metadata(&empty).unwrap().len(), 0);
        for p in [&missing, &dir.path().to_path_buf()] {
            let (out, v) = candidate_invoke(&candidate_write_args(action, &db, p));
            assert_eq!(out.status.code(), Some(2));
            assert_eq!(v["errors"][0]["field_path"], field);
        }
        let mut args = candidate_write_args(action, &db, input);
        args.pop();
        assert_eq!(candidate_invoke(&args).0.status.code(), Some(2));
        let mut missing_db = candidate_write_args(action, &db, input);
        missing_db.drain(2..4);
        assert_eq!(candidate_invoke(&missing_db).0.status.code(), Some(2));
        if action == "initialize" {
            let mut missing_id = candidate_write_args(action, &db, input);
            missing_id.drain(4..6);
            assert_eq!(candidate_invoke(&missing_id).0.status.code(), Some(2));
        }
        for limit in ["0", "16777217"] {
            let mut args = candidate_write_args(action, &db, input);
            args.extend([OsStr::new("--max-bytes"), OsStr::new(limit)]);
            assert_eq!(candidate_invoke(&args).0.status.code(), Some(2));
        }
    }
    for action in ["attempt", "accept", "execute", "submit"] {
        assert_eq!(
            candidate_invoke(&[OsStr::new("candidate"), OsStr::new(action)])
                .0
                .status
                .code(),
            Some(2)
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = dir.path().join("contract-link");
        symlink(&contract, &link).unwrap();
        assert_eq!(
            candidate_invoke(&candidate_write_args("initialize", &db, &link))
                .0
                .status
                .code(),
            Some(0)
        );
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::ffi::OsStringExt;
            let os_path = dir
                .path()
                .join(std::ffi::OsString::from_vec(b"proposal-\xff.json".to_vec()));
            fs::copy(&path, &os_path).unwrap();
            assert_eq!(
                candidate_invoke(&candidate_write_args("propose", &db, &os_path))
                    .0
                    .status
                    .code(),
                Some(0)
            );
        }
    }
    assert!(!missing.exists());
    assert_eq!(app.candidate_attempt("candidate-1").unwrap(), None);
}
#[cfg(any(unix, windows))]
#[test]
fn candidate_write_broken_streams_preserve_durable_commits_and_errors() {
    let dir = tempfile::tempdir().unwrap();
    let (db, _, contract, path, _) = candidate_write_fixture(dir.path());
    for (action, input) in [("initialize", &contract), ("propose", &path)] {
        let args = candidate_write_args(action, &db, input);
        let status = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(&args)
            .stdout(broken_output())
            .stderr(broken_output())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(6));
    }
    let app = Application::open(&db).unwrap();
    assert_eq!(
        app.candidate_current("headless").unwrap().unwrap().revision,
        0
    );
    let staged = app.candidate("candidate-1").unwrap().unwrap();
    assert_eq!(app.candidate_attempt("candidate-1").unwrap(), None);
    candidate_query(
        &db,
        "show",
        "candidate-1",
        &serde_json::to_value(staged).unwrap(),
    );
    for (action, input) in [("initialize", &contract), ("propose", &path)] {
        let args = candidate_write_args(action, &db, input);
        let (normal, v) = candidate_invoke(&args);
        assert_eq!(normal.status.code(), Some(0), "{v}");
        let failed = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(&args)
            .stderr(broken_output())
            .output()
            .unwrap();
        assert_eq!(failed.status.code(), Some(0));
        assert_eq!(failed.stdout, normal.stdout);
        let failed = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(&args)
            .stdout(broken_output())
            .output()
            .unwrap();
        assert_eq!(failed.status.code(), Some(6));
        fs::write(input, b"{broken").unwrap();
        let (normal, v) = candidate_invoke(&args);
        assert_eq!(normal.status.code(), Some(2), "{v}");
        let failed = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(&args)
            .stderr(broken_output())
            .output()
            .unwrap();
        assert_eq!(failed.status.code(), Some(2));
        assert_eq!(failed.stdout, normal.stdout);
        let status = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(&args)
            .stdout(broken_output())
            .stderr(broken_output())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(6));
    }
    assert_eq!(app.candidate("not-written").unwrap(), None);
}

fn candidate_attempt_fixture(dir: &Path, mode: &str) -> (PathBuf, Application, String) {
    let (db, mut app, mut proposal) = candidate_fixture(dir, mode == "Rejected");
    match mode {
        "NoContract" => proposal.contract = None,
        "Quality" => proposal.contract.as_mut().unwrap().required_quality = vec!["QUALITY".into()],
        "Mismatch" => proposal.contract.as_mut().unwrap().max_component_bytes = 512 * 1024,
        _ => {}
    }
    app.propose_candidate(&proposal).unwrap();
    if mode == "Conflict" {
        let mut winner = proposal.clone();
        winner.id = "winner".into();
        winner.candidate_version = "other".into();
        app.propose_candidate(&winner).unwrap();
        assert!(matches!(
            app.attempt_accept_candidate(&winner.id).unwrap(),
            assetweft_core::candidate::Acceptance::Accepted { .. }
        ));
    }
    assert_eq!(app.candidate_attempt(&proposal.id).unwrap(), None);
    (db, app, proposal.id)
}
fn candidate_attempt_projection(mode: &str) -> (i32, &'static str, Option<&'static str>) {
    match mode {
        "Accepted" => (0, "PASS", None),
        "Rejected" => (1, "FAIL", Some("REJECTED")),
        "Conflict" => (1, "FAIL", Some("CONFLICT")),
        _ => (5, "UNSUPPORTED", Some("NOT_RUN")),
    }
}
#[test]
fn candidate_attempt_first_write_exact_receipts_and_historical_replay() {
    use assetweft_core::candidate::Acceptance;
    for mode in [
        "Accepted",
        "Rejected",
        "Conflict",
        "NoContract",
        "Quality",
        "Mismatch",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let control = tempfile::tempdir().unwrap();
        let (_, mut core, control_id) = candidate_attempt_fixture(control.path(), mode);
        let expected = core.attempt_accept_candidate(&control_id).unwrap();
        assert!(
            matches!((&expected, mode),
            (Acceptance::Accepted { current, .. }, "Accepted") if current.revision == 1)
                || matches!(
                    (&expected, mode),
                    (Acceptance::Rejected { .. }, "Rejected")
                        | (
                            Acceptance::Conflict {
                                current: assetweft_core::catalog::domain::Current {
                                    revision: 1,
                                    ..
                                },
                                ..
                            },
                            "Conflict"
                        )
                        | (
                            Acceptance::NotRun { report: None, .. },
                            "NoContract" | "Quality"
                        )
                        | (
                            Acceptance::NotRun {
                                report: Some(_),
                                ..
                            },
                            "Mismatch"
                        )
                )
        );
        let (db, app, id) = candidate_attempt_fixture(dir.path(), mode);
        let before = app.candidate_current("character").unwrap();
        app.close().unwrap();
        let mut args = candidate_args("attempt", db.as_os_str(), OsStr::new(&id));
        args.extend([
            OsStr::new("--call-id"),
            OsStr::new("attempt α"),
            OsStr::new("--max-bytes"),
            OsStr::new("1"),
        ]);
        let (out, value) = candidate_invoke(&args);
        let (code, status, error_code) = candidate_attempt_projection(mode);
        assert_eq!(
            out.status.code(),
            Some(code),
            "first attempt {mode}: {value}"
        );
        assert_eq!(value["command"], "candidate attempt");
        assert_eq!(value["call_id"], "attempt α");
        assert_eq!(value["status"], status);
        assert_eq!(
            value["results"],
            json!({"scope":"local_fixture_candidate_acceptance_attempt","receipt":expected})
        );
        if let Some(code) = error_code {
            assert_eq!(value["errors"][0]["code"], code);
            assert_eq!(value["errors"][0]["field_path"], "candidate");
        } else {
            assert_eq!(value["errors"], json!([]));
        }
        let mut reopened = Application::open(&db).unwrap();
        assert_eq!(
            reopened.candidate_attempt(&id).unwrap(),
            Some(expected.clone())
        );
        let after = reopened.candidate_current("character").unwrap();
        if let Acceptance::Accepted { current, .. } = &expected {
            assert_eq!(after.as_ref(), Some(current));
        } else {
            assert_eq!(after, before, "nonaccepted attempt must not advance {mode}");
        }
        let head = after.unwrap();
        let mut next = core.candidate(&control_id).unwrap().unwrap();
        next.id = "later-head".into();
        let proposal = assetweft_core::candidate::Proposal {
            id: next.id,
            base_version: head.version_id.clone(),
            candidate_version: if head.version_id == "other" {
                "proposed"
            } else {
                "other"
            }
            .into(),
            expected_revision: head.revision,
            editable: vec![assetweft_core::library::ComponentRole::Mesh],
            lock_mode: assetweft_core::LockMode::Byte,
            contract: Some(head.contract.clone()),
        };
        reopened.propose_candidate(&proposal).unwrap();
        assert!(matches!(
            reopened.attempt_accept_candidate(&proposal.id).unwrap(),
            Acceptance::Accepted { .. }
        ));
        let later = reopened.candidate_current("character").unwrap();
        let managed = reopened.managed_object("mesh0").unwrap().unwrap();
        reopened.close().unwrap();
        assert_eq!(candidate_invoke(&args).0.stdout, out.stdout);
        let owned = dir.path().join("objects").join(managed.content_hash);
        assert!(owned.is_file());
        fs::remove_file(&owned).unwrap();
        let (replay, _) = candidate_invoke(&args);
        assert_eq!(replay.status.code(), Some(code));
        assert_eq!(replay.stdout, out.stdout);
        let reopened = Application::open(&db).unwrap();
        assert_eq!(reopened.candidate_attempt(&id).unwrap(), Some(expected));
        assert_eq!(reopened.candidate_current("character").unwrap(), later);
        assert!(!owned.exists());
    }
}
#[test]
fn candidate_attempt_database_identity_grammar_and_call_id() {
    let dir = tempfile::tempdir().unwrap();
    let (db, mut app, id) = candidate_attempt_fixture(dir.path(), "Accepted");
    let before = app.candidate_current("character").unwrap();
    for invalid in [
        "missing",
        "",
        " candidate-1",
        "candidate-1 ",
        "..",
        "invalid/id",
    ] {
        let expected = app.attempt_accept_candidate(invalid).unwrap_err();
        let (out, value) = candidate_invoke(&candidate_args(
            "attempt",
            db.as_os_str(),
            OsStr::new(invalid),
        ));
        assert_eq!(out.status.code(), Some(2), "{value}");
        assert_eq!(value["status"], "INVALID_INPUT");
        assert_eq!(value["errors"][0]["message"], expected.to_string());
        assert_eq!(value["results"], Value::Null);
    }
    let missing = dir.path().join("missing.sqlite");
    let empty = dir.path().join("empty.sqlite");
    fs::write(&empty, []).unwrap();
    for (path, code, status) in [
        (missing.as_path(), 2, "INVALID_INPUT"),
        (dir.path(), 2, "INVALID_INPUT"),
        (empty.as_path(), 3, "INPUT_INCOMPLETE"),
    ] {
        let (out, value) = candidate_invoke(&candidate_args(
            "attempt",
            path.as_os_str(),
            OsStr::new(&id),
        ));
        assert_eq!(out.status.code(), Some(code), "{value}");
        assert_eq!(value["status"], status);
        assert_eq!(value["results"], Value::Null);
    }
    assert!(!missing.exists());
    assert_eq!(fs::metadata(&empty).unwrap().len(), 0);
    let valid = candidate_args("attempt", db.as_os_str(), OsStr::new(&id));
    for tail in [
        vec![OsStr::new("--extra")],
        vec![OsStr::new("--report"), OsStr::new("PASS")],
        vec![OsStr::new("--max-bytes"), OsStr::new("0")],
    ] {
        let mut args = vec![OsStr::new("--call-id"), OsStr::new("grammar α")];
        args.extend(valid.clone());
        args.extend(tail);
        let (out, value) = candidate_invoke(&args);
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(value["call_id"], "grammar α");
    }
    for args in [
        vec![OsStr::new("candidate"), OsStr::new("attempt")],
        valid[..4].to_vec(),
        vec![
            OsStr::new("candidate"),
            OsStr::new("attempt"),
            OsStr::new("--candidate-id"),
            OsStr::new(&id),
        ],
    ] {
        assert_eq!(candidate_invoke(&args).0.status.code(), Some(2));
    }
    let (_, help) = candidate_invoke(&[
        OsStr::new("candidate"),
        OsStr::new("attempt"),
        OsStr::new("--help"),
    ]);
    assert_eq!(help["status"], "HELP");
    assert!(
        help["results"]["text"]
            .as_str()
            .unwrap()
            .contains("--candidate-id")
    );
    assert_eq!(app.candidate_attempt(&id).unwrap(), None);
    assert_eq!(app.candidate_current("character").unwrap(), before);
}
#[cfg(any(unix, windows))]
#[test]
fn candidate_attempt_broken_streams_retain_first_commits_and_domain_outcomes() {
    use assetweft_core::candidate::Acceptance;
    for mode in [
        "Accepted",
        "Rejected",
        "Conflict",
        "NoContract",
        "Quality",
        "Mismatch",
    ] {
        for stream in ["stderr", "stdout", "both"] {
            let dir = tempfile::tempdir().unwrap();
            let control = tempfile::tempdir().unwrap();
            let (_, mut core, core_id) = candidate_attempt_fixture(control.path(), mode);
            let expected = core.attempt_accept_candidate(&core_id).unwrap();
            let (db, app, id) = candidate_attempt_fixture(dir.path(), mode);
            let before = app.candidate_current("character").unwrap();
            app.close().unwrap();
            let args = candidate_args("attempt", db.as_os_str(), OsStr::new(&id));
            let mut command = Command::new(env!("CARGO_BIN_EXE_assetweft"));
            command.args(&args);
            if stream != "stderr" {
                command.stdout(broken_output());
            }
            if stream != "stdout" {
                command.stderr(broken_output());
            }
            let output = command.output().unwrap();
            let (code, status, _) = candidate_attempt_projection(mode);
            assert_eq!(
                output.status.code(),
                Some(if stream == "stderr" { code } else { 6 }),
                "{mode}/{stream}"
            );
            if stream == "stderr" {
                let v: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(v["status"], status);
                assert_eq!(
                    v["results"],
                    json!({"scope":"local_fixture_candidate_acceptance_attempt","receipt":expected})
                );
            }
            let reopened = Application::open(&db).unwrap();
            assert_eq!(
                reopened.candidate_attempt(&id).unwrap(),
                Some(expected.clone())
            );
            let head = reopened.candidate_current("character").unwrap();
            if let Acceptance::Accepted { current, .. } = expected {
                assert_eq!(head, Some(current));
            } else {
                assert_eq!(head, before);
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let (db, app, id) = candidate_attempt_fixture(dir.path(), "Accepted");
    for invalid in [false, true] {
        let mut args = candidate_args("attempt", db.as_os_str(), OsStr::new("missing"));
        if invalid {
            args.push(OsStr::new("--extra"));
        }
        let ordinary = candidate_invoke(&args).0;
        assert_eq!(ordinary.status.code(), Some(2));
        let failed = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(&args)
            .stderr(broken_output())
            .output()
            .unwrap();
        assert_eq!(failed.status.code(), ordinary.status.code());
        assert_eq!(failed.stdout, ordinary.stdout);
        for both in [false, true] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_assetweft"));
            command.args(&args).stdout(broken_output());
            if both {
                command.stderr(broken_output());
            }
            assert_eq!(command.output().unwrap().status.code(), Some(6));
        }
    }
    assert_eq!(app.candidate_attempt(&id).unwrap(), None);
}
fn catalog_write_args<'a>(action: &'a str, db: &'a Path, input: &'a Path) -> Vec<&'a OsStr> {
    vec![
        OsStr::new("catalog"),
        OsStr::new(action),
        OsStr::new("--db"),
        db.as_os_str(),
        OsStr::new(if action == "register-version" {
            "--version"
        } else {
            "--reference"
        }),
        input.as_os_str(),
    ]
}
fn catalog_write_role(role: assetweft_core::library::ComponentRole) -> &'static str {
    use assetweft_core::library::ComponentRole;
    match role {
        ComponentRole::Mesh => "mesh",
        ComponentRole::Materials => "materials",
        ComponentRole::Rig => "rig",
        ComponentRole::Motion => "motion",
        ComponentRole::Vfx => "vfx",
        ComponentRole::Audio => "audio",
        ComponentRole::Dialogue => "dialogue",
    }
}
fn catalog_write_wire(id: &str, presence: &str, authorized: bool) -> Value {
    json!({"asset_id":"metadata-asset","version_id":id,"content_hash":"a".repeat(64),
        "input_id":"input 原样","source":" declared source ","authorization":"LOCAL DECLARATION ONLY",
        "authorized":authorized,"license_id":"unverified-license","usage_scope":"TEST ONLY",
        "source_format":"declared-format","contract_version":"1.0.0","presence":presence})
}
fn catalog_write_record(args: &[&OsStr], out: &Output) {
    if let Some(directory) = std::env::var_os("AW033B_WRITE_RECEIPTS") {
        static SEQUENCE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let index = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let prefix = PathBuf::from(directory).join(format!("write-{index:04}"));
        fs::write(prefix.with_extension("stdout"), &out.stdout).unwrap();
        fs::write(prefix.with_extension("stderr"), &out.stderr).unwrap();
        fs::write(prefix.with_extension("json"), serde_json::to_vec(&json!({"argv":args.iter().map(|a|a.to_string_lossy()).collect::<Vec<_>>(),"exit":out.status.code(),"stdout_utf8_bytes":out.stdout.len(),"stderr_utf8_bytes":out.stderr.len()})).unwrap()).unwrap();
    }
}
fn catalog_write_invoke(args: &[&OsStr]) -> (Output, Value) {
    let (out, value) = invoke(args);
    catalog_write_record(args, &out);
    (out, value)
}
fn catalog_write_error(args: &[&OsStr], failure: &assetweft_core::library::LibraryError) {
    use assetweft_core::library::LibraryError;
    let (code, status) = match failure {
        LibraryError::InvalidInput(_) | LibraryError::Constraint(_) => (2, "INVALID_INPUT"),
        LibraryError::Uninitialized => (3, "INPUT_INCOMPLETE"),
        LibraryError::FutureSchema(_) => (5, "UNSUPPORTED"),
        LibraryError::Busy(_) | LibraryError::Io(_) => (6, "INTERNAL"),
        LibraryError::Database(_) | LibraryError::UnknownValue(_) => (1, "FAIL"),
    };
    let (out, value) = catalog_write_invoke(args);
    assert_eq!(out.status.code(), Some(code), "{value}");
    assert_eq!(value["status"], status);
    assert_eq!(
        value["errors"],
        json!([{"field_path":"catalog","code":status,"message":failure.to_string()}])
    );
    assert_eq!(value["results"], Value::Null);
}
#[test]
fn catalog_write_versions_acknowledge_typed_declarations_and_persist_without_bytes() {
    use assetweft_core::library::Presence;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("目录 metadata.sqlite");
    let mut app = Application::open(&db).unwrap();
    app.initialize_library().unwrap();
    let path = dir.path().join("声明 version.json");
    for (id, presence, authorized) in [
        ("原样 v1", "UNKNOWN", true),
        ("missing-v2", "MISSING", false),
    ] {
        let wire = catalog_write_wire(id, presence, authorized);
        let bytes = serde_json::to_vec(&wire).unwrap();
        fs::write(&path, &bytes).unwrap();
        let mut args = catalog_write_args("register-version", &db, &path);
        args.extend([
            OsStr::new("--call-id"),
            OsStr::new("write α"),
            OsStr::new("--max-bytes"),
            OsStr::new("16777216"),
        ]);
        let (out, value) = catalog_write_invoke(&args);
        assert_eq!(out.status.code(), Some(0), "{value}");
        assert_eq!(
            value,
            json!({"schema_version":1,"command":"catalog register-version","call_id":"write α","status":"PASS","errors":[],"results":{"scope":"registered_local_asset_version_metadata","version":wire}})
        );
        assert!(out.stderr.is_empty());
        let mut row = catalog_version(
            id,
            if presence == "UNKNOWN" {
                Presence::Unknown
            } else {
                Presence::Missing
            },
        );
        row.authorized = authorized;
        let mut reopened = Application::open(&db).unwrap();
        assert_eq!(reopened.asset_version(id).unwrap(), Some(row.clone()));
        assert_eq!(reopened.managed_object(id).unwrap(), None);
        assert_eq!(reopened.candidate_current("metadata-asset").unwrap(), None);
        catalog_query(
            &db,
            "version",
            id,
            json!({"scope":"stored_local_asset_version_metadata","version":wire}),
        );
        let error = reopened.add_asset_version(&row).unwrap_err();
        catalog_write_error(&args, &error);
        row.content_hash = "b".repeat(64);
        let error = reopened.add_asset_version(&row).unwrap_err();
        let mut changed = wire.clone();
        changed["content_hash"] = json!(row.content_hash);
        fs::write(&path, changed.to_string()).unwrap();
        catalog_write_error(&args, &error);
        assert_eq!(
            reopened.asset_version(id).unwrap().unwrap().content_hash,
            "a".repeat(64)
        );
        fs::write(&path, &bytes).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    assert!(!dir.path().join("objects").exists());
}
#[test]
fn catalog_write_references_keep_all_roles_exact_targets_sort_and_core_failures() {
    use assetweft_core::library::{ComponentReference, ComponentRole, Presence};
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("引用 library.sqlite");
    let mut app = Application::open(&db).unwrap();
    app.initialize_library().unwrap();
    for id in ["owner", "target-old", "target-new"] {
        app.add_asset_version(&catalog_version(id, Presence::Unknown))
            .unwrap();
    }
    let path = dir.path().join("引用 reference.json");
    for role in [
        "vfx",
        "rig",
        "motion",
        "mesh",
        "materials",
        "dialogue",
        "audio",
    ] {
        let wire =
            json!({"owner_version_id":"owner","role":role,"component_version_id":"target-old"});
        fs::write(&path, wire.to_string()).unwrap();
        let args = catalog_write_args("add-reference", &db, &path);
        let (out, value) = catalog_write_invoke(&args);
        assert_eq!(out.status.code(), Some(0), "{value}");
        assert_eq!(
            value["results"],
            json!({"scope":"registered_local_exact_component_reference","reference":wire})
        );
        assert_eq!(value["status"], "PASS");
        assert_eq!(value["errors"], json!([]));
    }
    let before = app.component_references("owner").unwrap();
    assert_eq!(
        before
            .iter()
            .map(|r| catalog_write_role(r.role))
            .collect::<Vec<_>>(),
        [
            "audio",
            "dialogue",
            "materials",
            "mesh",
            "motion",
            "rig",
            "vfx"
        ]
    );
    assert!(
        before
            .iter()
            .all(|r| r.component_version_id == "target-old")
    );
    let wire =
        json!({"owner_version_id":"owner","role":"mesh","component_version_id":"target-new"});
    fs::write(&path, wire.to_string()).unwrap();
    assert_eq!(
        catalog_write_invoke(&catalog_write_args("add-reference", &db, &path))
            .0
            .status
            .code(),
        Some(0)
    );
    let saved = Application::open(&db)
        .unwrap()
        .component_references("owner")
        .unwrap();
    assert_eq!(saved[3].component_version_id, "target-new");
    assert_eq!(saved[4].component_version_id, "target-old");
    let projected = saved.iter().map(|r| json!({"owner_version_id":r.owner_version_id,"role":catalog_write_role(r.role),"component_version_id":r.component_version_id})).collect::<Vec<_>>();
    catalog_query(
        &db,
        "references",
        "owner",
        json!({"scope":"stored_local_exact_component_references","references":projected}),
    );
    for (owner, target) in [
        ("owner", "target-new"),
        ("absent", "target-old"),
        ("owner", "absent"),
        ("bad/id", "target-old"),
        ("owner", "bad/id"),
    ] {
        let reference = ComponentReference {
            owner_version_id: owner.into(),
            role: ComponentRole::Mesh,
            component_version_id: target.into(),
        };
        let error = app.add_component_reference(&reference).unwrap_err();
        fs::write(
            &path,
            json!({"owner_version_id":owner,"role":"mesh","component_version_id":target})
                .to_string(),
        )
        .unwrap();
        catalog_write_error(&catalog_write_args("add-reference", &db, &path), &error);
        assert_eq!(app.component_references("owner").unwrap(), saved);
    }
}
#[test]
fn catalog_write_strict_objects_all_required_fields_and_reader_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("strict.sqlite");
    let mut app = Application::open(&db).unwrap();
    app.initialize_library().unwrap();
    let path = dir.path().join("wire.json");
    for (action, field, good) in [
        (
            "register-version",
            "version",
            catalog_write_wire("strict", "UNKNOWN", false),
        ),
        (
            "add-reference",
            "reference",
            json!({"owner_version_id":"owner","role":"mesh","component_version_id":"target"}),
        ),
    ] {
        let mut bad = vec![
            "[]".into(),
            "null".into(),
            "{}".into(),
            "{broken".into(),
            format!("{good} null"),
            good.to_string().replacen('{', "{\"extra\":0,", 1),
            serde_json::to_string(&good.as_object().unwrap().values().collect::<Vec<_>>()).unwrap(),
        ];
        for key in good.as_object().unwrap().keys() {
            let mut missing = good.clone();
            missing.as_object_mut().unwrap().remove(key);
            bad.push(missing.to_string());
            let mut wrong = good.clone();
            wrong[key] = if key == "authorized" {
                json!("false")
            } else {
                json!(false)
            };
            bad.push(wrong.to_string());
            bad.push(
                good.to_string()
                    .replacen('{', &format!("{{\"{key}\":{},", good[key]), 1),
            );
        }
        for (key, wrong) in if action == "register-version" {
            vec![
                ("presence", json!("unknown")),
                ("presence", json!("PRESENT")),
                ("presence", json!({"UNKNOWN":null})),
                ("authorized", json!(1)),
                ("authorized", Value::Null),
            ]
        } else {
            vec![
                ("role", json!("Mesh")),
                ("role", json!("unknown")),
                ("role", json!({"mesh":null})),
                ("role", json!(1)),
            ]
        } {
            let mut value = good.clone();
            value[key] = wrong;
            bad.push(value.to_string());
        }
        for raw in bad {
            fs::write(&path, &raw).unwrap();
            let (out, value) = catalog_write_invoke(&catalog_write_args(action, &db, &path));
            assert_eq!(out.status.code(), Some(2), "{action} {raw}: {value}");
            assert_eq!(value["errors"][0]["field_path"], field, "{value}");
            assert_eq!(value["results"], Value::Null);
            if action == "register-version" {
                assert_eq!(app.asset_version("strict").unwrap(), None);
            }
            assert!(app.component_references("owner").unwrap().is_empty());
        }
        fs::write(&path, good.to_string()).unwrap();
        let size = fs::metadata(&path).unwrap().len().to_string();
        let mut args = catalog_write_args(action, &db, &path);
        args.extend([OsStr::new("--max-bytes"), OsStr::new("1")]);
        assert_eq!(catalog_write_invoke(&args).0.status.code(), Some(2));
        args[7] = OsStr::new(&size);
        let (out, value) = catalog_write_invoke(&args);
        assert_ne!(
            value["errors"][0]["field_path"], field,
            "exact byte limit must admit JSON: {value}"
        );
        assert_eq!(
            out.status.code(),
            Some(if action == "register-version" { 0 } else { 2 })
        );
        let mut exact_max = good.to_string().into_bytes();
        exact_max.resize(16777216, b' ');
        fs::write(&path, exact_max).unwrap();
        let (_, value) = catalog_write_invoke(&catalog_write_args(action, &db, &path));
        assert_eq!(
            value["errors"][0]["field_path"], "catalog",
            "16MiB JSON must reach Core: {value}"
        );
        let mut oversized = good.to_string().into_bytes();
        oversized.resize(16777217, b' ');
        fs::write(&path, oversized).unwrap();
        let (out, value) = catalog_write_invoke(&catalog_write_args(action, &db, &path));
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(value["errors"][0]["field_path"], field);
    }
    assert_eq!(
        app.asset_version("strict").unwrap().unwrap().version_id,
        "strict"
    );
    assert!(app.component_references("owner").unwrap().is_empty());
}
#[test]
fn catalog_write_domain_errors_and_frozen_references_preserve_facade_parity() {
    use assetweft_core::library::{ComponentReference, ComponentRole, Presence};
    let dir = tempfile::tempdir().unwrap();
    let (db, mut app, proposal) = candidate_fixture(dir.path(), false);
    let path = dir.path().join("wire.json");
    let head = app.candidate_current("character").unwrap();
    for (key, bad) in [
        ("asset_id", "bad/id"),
        ("version_id", ".."),
        ("input_id", ""),
        ("content_hash", "bad"),
        ("source", ""),
        ("authorization", ""),
        ("license_id", ""),
        ("usage_scope", ""),
        ("source_format", ""),
        ("contract_version", "future-version"),
    ] {
        let mut wire = catalog_write_wire("invalid", "UNKNOWN", false);
        wire[key] = json!(bad);
        let mut row = catalog_version("invalid", Presence::Unknown);
        row.authorized = false;
        match key {
            "asset_id" => row.asset_id = bad.into(),
            "version_id" => row.version_id = bad.into(),
            "input_id" => row.input_id = bad.into(),
            "content_hash" => row.content_hash = bad.into(),
            "source" => row.source = bad.into(),
            "authorization" => row.authorization = bad.into(),
            "license_id" => row.license_id = bad.into(),
            "usage_scope" => row.usage_scope = bad.into(),
            "source_format" => row.source_format = bad.into(),
            "contract_version" => row.contract_version = bad.into(),
            _ => unreachable!(),
        }
        let failure = app.add_asset_version(&row).unwrap_err();
        fs::write(&path, wire.to_string()).unwrap();
        catalog_write_error(
            &catalog_write_args("register-version", &db, &path),
            &failure,
        );
        assert_eq!(app.asset_version("invalid").unwrap(), None);
    }
    app.propose_candidate(&proposal).unwrap();
    for owner in ["base", "proposed"] {
        let r = ComponentReference {
            owner_version_id: owner.into(),
            role: ComponentRole::Audio,
            component_version_id: "mesh2".into(),
        };
        let before = app.component_references(owner).unwrap();
        let failure = app.add_component_reference(&r).unwrap_err();
        fs::write(
            &path,
            json!({"owner_version_id":owner,"role":"audio","component_version_id":"mesh2"})
                .to_string(),
        )
        .unwrap();
        catalog_write_error(&catalog_write_args("add-reference", &db, &path), &failure);
        assert_eq!(app.component_references(owner).unwrap(), before);
    }
    assert_eq!(app.candidate_current("character").unwrap(), head);
    assert_eq!(app.candidate_attempt(&proposal.id).unwrap(), None);
}
#[test]
fn catalog_write_database_grammar_paths_and_admission_precedence() {
    use assetweft_core::library::LibraryError;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("library.sqlite");
    let mut app = Application::open(&db).unwrap();
    app.initialize_library().unwrap();
    let path = dir.path().join("声明 file.json");
    let empty = dir.path().join("empty.sqlite");
    fs::write(&empty, []).unwrap();
    let missing = dir.path().join("absent.sqlite");
    let no_input = dir.path().join("absent.json");
    for (action, field, wire) in [
        (
            "register-version",
            "version",
            catalog_write_wire("path-version", "UNKNOWN", false),
        ),
        (
            "add-reference",
            "reference",
            json!({"owner_version_id":"owner","role":"mesh","component_version_id":"target"}),
        ),
    ] {
        fs::write(&path, wire.to_string()).unwrap();
        for bad_db in [&missing, dir.path(), Path::new("")] {
            let (out, value) = catalog_write_invoke(&catalog_write_args(action, bad_db, &no_input));
            assert_eq!(out.status.code(), Some(2));
            assert_eq!(
                value["errors"][0]["field_path"],
                if bad_db.as_os_str().is_empty() {
                    "arguments"
                } else {
                    "db"
                }
            );
        }
        catalog_write_error(
            &catalog_write_args(action, &empty, &path),
            &LibraryError::Uninitialized,
        );
        for bad_input in [&no_input, dir.path()] {
            let (out, value) = catalog_write_invoke(&catalog_write_args(action, &db, bad_input));
            assert_eq!(out.status.code(), Some(2));
            assert_eq!(value["errors"][0]["field_path"], field);
        }
        fs::write(&path, b"{broken").unwrap();
        let (out, value) = catalog_write_invoke(&catalog_write_args(action, &empty, &path));
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(value["errors"][0]["field_path"], field);
        let args = catalog_write_args(action, &db, &path);
        let (out, _) = catalog_write_invoke(&args[..4]);
        assert_eq!(out.status.code(), Some(2));
        for limit in ["0", "16777217", "no"] {
            let mut invalid = args.clone();
            invalid.extend([OsStr::new("--max-bytes"), OsStr::new(limit)]);
            assert_eq!(catalog_write_invoke(&invalid).0.status.code(), Some(2));
        }
        let mut extra = args.clone();
        extra.push(OsStr::new("--unknown"));
        assert_eq!(catalog_write_invoke(&extra).0.status.code(), Some(2));
        let (out, value) = catalog_write_invoke(&[
            OsStr::new("--call-id"),
            OsStr::new("help α"),
            OsStr::new("catalog"),
            OsStr::new(action),
            OsStr::new("--help"),
        ]);
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(value["call_id"], "help α");
        assert!(value.to_string().contains(&format!("--{field}")));
    }
    assert!(!missing.exists());
    assert!(matches!(
        Application::open(&empty).unwrap().asset_version("x"),
        Err(LibraryError::Uninitialized)
    ));
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let mut raw = dir.path().as_os_str().as_encoded_bytes().to_vec();
        raw.extend_from_slice(b"/wire-\xff.json");
        let unusual = PathBuf::from(std::ffi::OsString::from_vec(raw));
        let unusual_write = fs::write(
            &unusual,
            catalog_write_wire("os-path", "UNKNOWN", false).to_string(),
        );
        println!(
            "non-UTF8 input filename: {unusual_write:?}; an OS rejection is NOT_RUN for that path case"
        );
        if unusual_write.is_ok() {
            assert_eq!(
                catalog_write_invoke(&catalog_write_args("register-version", &db, &unusual))
                    .0
                    .status
                    .code(),
                Some(0)
            );
        }
    }
}
#[cfg(any(unix, windows))]
#[test]
fn catalog_write_first_commits_survive_broken_streams_without_duplicate_success() {
    use assetweft_core::library::Presence;
    for action in ["register-version", "add-reference"] {
        for mode in ["stderr", "stdout", "both"] {
            let dir = tempfile::tempdir().unwrap();
            let db = dir.path().join("streams.sqlite");
            let mut app = Application::open(&db).unwrap();
            app.initialize_library().unwrap();
            if action == "add-reference" {
                for id in ["owner", "target"] {
                    app.add_asset_version(&catalog_version(id, Presence::Unknown))
                        .unwrap();
                }
            }
            let wire = if action == "register-version" {
                catalog_write_wire("first", "UNKNOWN", false)
            } else {
                json!({"owner_version_id":"owner","role":"mesh","component_version_id":"target"})
            };
            let path = dir.path().join("wire.json");
            fs::write(&path, wire.to_string()).unwrap();
            let args = catalog_write_args(action, &db, &path);
            let mut command = Command::new(env!("CARGO_BIN_EXE_assetweft"));
            command.args(&args);
            if mode != "stderr" {
                command.stdout(broken_output());
            }
            if mode != "stdout" {
                command.stderr(broken_output());
            }
            let out = command.output().unwrap();
            catalog_write_record(&args, &out);
            assert_eq!(
                out.status.code(),
                Some(if mode == "stderr" { 0 } else { 6 })
            );
            if mode == "stderr" {
                let v: Value = serde_json::from_slice(&out.stdout).unwrap();
                assert_eq!(v["status"], "PASS");
            }
            let mut reopened = Application::open(&db).unwrap();
            if action == "register-version" {
                assert!(!reopened.asset_version("first").unwrap().unwrap().authorized);
                assert_eq!(reopened.managed_object("first").unwrap(), None);
            } else {
                assert_eq!(reopened.component_references("owner").unwrap().len(), 1);
            }
            let grammar_args = &args[..4];
            let (grammar, _) = catalog_write_invoke(grammar_args);
            assert_eq!(grammar.status.code(), Some(2));
            let failed = Command::new(env!("CARGO_BIN_EXE_assetweft"))
                .args(grammar_args)
                .stderr(broken_output())
                .output()
                .unwrap();
            catalog_write_record(grammar_args, &failed);
            assert_eq!(failed.status.code(), Some(2));
            assert_eq!(failed.stdout, grammar.stdout);
            let failed = Command::new(env!("CARGO_BIN_EXE_assetweft"))
                .args(grammar_args)
                .stdout(broken_output())
                .stderr(broken_output())
                .output()
                .unwrap();
            catalog_write_record(grammar_args, &failed);
            assert_eq!(failed.status.code(), Some(6));
            let (normal, value) = catalog_write_invoke(&args);
            assert_eq!(
                normal.status.code(),
                Some(2),
                "repeat must retain core uniqueness failure: {value}"
            );
            for raw in [wire.to_string(), "{broken".into()] {
                fs::write(&path, raw).unwrap();
                let (normal, _) = catalog_write_invoke(&args);
                let failed = Command::new(env!("CARGO_BIN_EXE_assetweft"))
                    .args(&args)
                    .stderr(broken_output())
                    .output()
                    .unwrap();
                catalog_write_record(&args, &failed);
                assert_eq!(failed.status.code(), normal.status.code());
                assert_eq!(failed.stdout, normal.stdout);
                for both in [false, true] {
                    let mut command = Command::new(env!("CARGO_BIN_EXE_assetweft"));
                    command.args(&args).stdout(broken_output());
                    if both {
                        command.stderr(broken_output());
                    }
                    let failed = command.output().unwrap();
                    catalog_write_record(&args, &failed);
                    assert_eq!(failed.status.code(), Some(6));
                }
            }
        }
    }
}

fn object_args<'a>(
    db: &'a Path,
    version: &'a Path,
    file: &'a Path,
    len: &'a str,
) -> Vec<&'a OsStr> {
    vec![
        OsStr::new("catalog"),
        OsStr::new("commit-object"),
        OsStr::new("--db"),
        db.as_os_str(),
        OsStr::new("--version"),
        version.as_os_str(),
        OsStr::new("--file"),
        file.as_os_str(),
        OsStr::new("--expected-len"),
        OsStr::new(len),
    ]
}
fn object_wire(id: &str) -> Value {
    let mut wire = catalog_write_wire(id, "UNKNOWN", false);
    wire["content_hash"] =
        json!("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    wire
}

fn library_backup_test_root(path: &Path) -> PathBuf {
    #[cfg(windows)]
    return path.to_path_buf();
    #[cfg(not(windows))]
    return path.canonicalize().unwrap();
}

#[test]
fn library_backup_restore_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let root = library_backup_test_root(dir.path());
    let db = root.join("library.sqlite");
    let fixture = root.join("version.json");
    let file = root.join("bytes");
    let backup = root.join("backup");
    let restored = root.join("restored");
    let metadata = catalog_write_wire("metadata", "MISSING", false);
    let success = |args: &[&OsStr]| {
        let (out, value) = invoke(args);
        assert_eq!(out.status.code(), Some(0), "{value}");
        assert!(out.stderr.is_empty());
        assert_eq!(value["status"], "PASS");
        assert_eq!(value["errors"], json!([]));
        value
    };
    let feedback = |action, db: &Path, extra: &[&str]| {
        let mut args = vec!["catalog", action, "--db"]
            .into_iter()
            .map(OsStr::new)
            .collect::<Vec<_>>();
        args.push(db.as_os_str());
        args.extend(["--asset-id", "metadata-asset", "--version-id", "managed"].map(OsStr::new));
        args.extend(extra.iter().map(OsStr::new));
        success(&args)
    };
    success(&library_init_args(db.as_os_str()));
    fs::write(&file, b"abc").unwrap();
    fs::write(&fixture, object_wire("managed").to_string()).unwrap();
    success(&object_args(&db, &fixture, &file, "3"));
    fs::write(&fixture, metadata.to_string()).unwrap();
    success(&catalog_write_args("register-version", &db, &fixture));
    let appended = feedback(
        "feedback-add",
        &db,
        &[
            "--feedback-id",
            "review-1",
            "--decision",
            "ACCEPT",
            "--reason",
            "原样",
        ],
    );

    let saved = success(&[
        OsStr::new("library"),
        OsStr::new("backup"),
        OsStr::new("--db"),
        db.as_os_str(),
        OsStr::new("--out"),
        backup.as_os_str(),
        OsStr::new("--max-bytes"),
        OsStr::new("1"),
    ]);
    assert_eq!(saved["command"], "library backup");
    assert_eq!(saved["results"]["scope"], "backed_up_local_library");
    assert_eq!(saved["results"]["out_dir"], backup.display().to_string());
    let manifest: Value =
        serde_json::from_slice(&fs::read(backup.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(saved["results"]["manifest"], manifest);
    assert!(manifest["database"]["bytes"].as_u64().unwrap() > 1);
    let hash = object_wire("managed")["content_hash"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        fs::read(backup.join("objects").join(&hash)).unwrap(),
        b"abc"
    );

    let restore = |dest: &Path| {
        invoke(&[
            OsStr::new("library"),
            OsStr::new("restore"),
            OsStr::new("--from"),
            backup.as_os_str(),
            OsStr::new("--dest"),
            dest.as_os_str(),
            OsStr::new("--max-bytes"),
            OsStr::new("1"),
        ])
    };
    let (out, result) = restore(&restored);
    assert_eq!(out.status.code(), Some(0), "{result}");
    assert!(out.stderr.is_empty());
    assert_eq!(result["status"], "PASS");
    assert_eq!(result["errors"], json!([]));
    assert_eq!(result["command"], "library restore");
    assert_eq!(
        result["results"],
        json!({"scope":"restored_local_library",
        "dest_dir":restored.display().to_string(),"manifest":manifest})
    );
    assert_eq!(
        fs::read(restored.join("objects").join(&hash)).unwrap(),
        b"abc"
    );

    let restored_db = restored.join("library.sqlite");
    for (action, id) in [
        ("version", "managed"),
        ("version", "metadata"),
        ("file-availability", "managed"),
    ] {
        let original = success(&catalog_args(action, db.as_os_str(), OsStr::new(id)));
        assert_eq!(
            success(&catalog_args(
                action,
                restored_db.as_os_str(),
                OsStr::new(id)
            )),
            original
        );
        if action == "file-availability" {
            assert_eq!(original["results"]["availability"], "AVAILABLE");
        }
    }
    let latest = feedback("feedback-latest", &restored_db, &[]);
    assert_eq!(latest, feedback("feedback-latest", &db, &[]));
    assert_eq!(
        latest["results"]["feedback"],
        appended["results"]["feedback"]
    );

    let existing = root.join("existing-restore");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("library.sqlite"), b"original DB").unwrap();
    let corrupt = root.join("corrupt-restore");
    for dest in [&existing, &corrupt] {
        if dest == &corrupt {
            fs::write(backup.join("objects").join(&hash), b"bad").unwrap();
        }
        let (out, value) = restore(dest);
        assert_eq!(out.status.code(), Some(2), "{value}");
        assert_eq!(value["status"], "INVALID_INPUT");
        assert_eq!(value["errors"][0]["field_path"], "library");
        assert_eq!(value["results"], Value::Null);
        assert!(!dest.join("manifest.json").exists());
    }
    assert_eq!(
        fs::read(existing.join("library.sqlite")).unwrap(),
        b"original DB"
    );
    assert_eq!(fs::read_dir(existing).unwrap().count(), 1);
}

#[test]
fn library_backup_rejects_missing_uninitialized_and_existing_destination() {
    let dir = tempfile::tempdir().unwrap();
    let root = library_backup_test_root(dir.path());
    let db = root.join("library.sqlite");
    let backup = root.join("backup");
    let run = |code, status| {
        let (out, value) = invoke(&[
            OsStr::new("library"),
            OsStr::new("backup"),
            OsStr::new("--db"),
            db.as_os_str(),
            OsStr::new("--out"),
            backup.as_os_str(),
        ]);
        assert_eq!(out.status.code(), Some(code), "{value}");
        assert_eq!(value["command"], "library backup");
        assert_eq!(value["status"], status);
        assert_eq!(value["errors"][0]["code"], status);
        assert_eq!(value["results"], Value::Null);
    };
    run(2, "INVALID_INPUT");
    assert!(!db.exists());
    fs::write(&db, b"").unwrap();
    run(3, "INPUT_INCOMPLETE");
    assert!(!backup.exists());
    assert_eq!(
        invoke(&library_init_args(db.as_os_str())).0.status.code(),
        Some(0)
    );
    fs::create_dir(&backup).unwrap();
    fs::write(backup.join("manifest.json"), b"original manifest").unwrap();
    run(2, "INVALID_INPUT");
    assert_eq!(
        fs::read(backup.join("manifest.json")).unwrap(),
        b"original manifest"
    );
    assert_eq!(fs::read_dir(backup).unwrap().count(), 1);
}

#[test]
fn object_commit_roundtrip_and_duplicate_preserve_original() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("library.sqlite");
    Application::open(&db)
        .unwrap()
        .initialize_library()
        .unwrap();
    let version = dir.path().join("version.json");
    let file = dir.path().join("bytes");
    fs::write(&file, b"abc").unwrap();
    for id in ["first", "second"] {
        fs::write(&version, object_wire(id).to_string()).unwrap();
        let mut args = object_args(&db, &version, &file, "3");
        args.extend([OsStr::new("--call-id"), OsStr::new("object α")]);
        let (out, value) = catalog_write_invoke(&args);
        assert_eq!(out.status.code(), Some(0), "{value}");
        assert!(out.stderr.is_empty());
        assert_eq!(out.stdout.iter().filter(|b| **b == b'\n').count(), 1);
        assert_eq!(
            value,
            json!({"schema_version":1,"command":"catalog commit-object","call_id":"object α","status":"PASS","errors":[],"results":{"scope":"committed_local_managed_object","version_id":id,"content_hash":object_wire(id)["content_hash"],"byte_length":3}})
        );
        let mut app = Application::open(&db).unwrap();
        let row = app.asset_version(id).unwrap().unwrap();
        assert!(!row.authorized);
        assert_eq!(row.source, " declared source ");
        let stored = app.managed_object(id).unwrap().unwrap();
        assert_eq!(stored.content_hash, row.content_hash);
        assert_eq!(stored.byte_length, 3);
        assert_eq!(
            fs::read(dir.path().join("objects").join(&stored.content_hash)).unwrap(),
            b"abc"
        );
        let (duplicate, _) = catalog_write_invoke(&args);
        assert_eq!(duplicate.status.code(), Some(2));
        assert_eq!(
            Application::open(&db).unwrap().asset_version(id).unwrap(),
            Some(row)
        );
    }
}
#[test]
fn object_commit_rejects_inputs_without_registration() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("library.sqlite");
    Application::open(&db)
        .unwrap()
        .initialize_library()
        .unwrap();
    let version = dir.path().join("version.json");
    let file = dir.path().join("bytes");
    fs::write(&file, b"abc").unwrap();
    for (case, len, cap, code) in [
        ("hash", "3", "16777216", 1),
        ("short", "2", "16777216", 2),
        ("long", "4", "16777216", 2),
        ("json-cap", "3", "2", 2),
        ("array", "3", "16777216", 2),
        ("presence", "3", "16777216", 2),
        ("unknown", "3", "16777216", 2),
    ] {
        let mut wire = object_wire(case);
        if case == "hash" {
            wire["content_hash"] = json!("a".repeat(64));
        }
        if case == "array" {
            wire = json!([]);
        }
        if case == "presence" {
            wire["presence"] = json!(["UNKNOWN"]);
        }
        if case == "unknown" {
            wire["extra"] = json!(true);
        }
        fs::write(&version, wire.to_string()).unwrap();
        let mut args = object_args(&db, &version, &file, len);
        args.extend([OsStr::new("--max-bytes"), OsStr::new(cap)]);
        let (out, value) = catalog_write_invoke(&args);
        assert_eq!(out.status.code(), Some(code), "{case}: {value}");
        if case == "json-cap" {
            assert_eq!(value["errors"][0]["field_path"], "version", "{value}");
        }
        assert_eq!(
            Application::open(&db).unwrap().asset_version(case).unwrap(),
            None
        );
        assert_eq!(
            Application::open(&db)
                .unwrap()
                .managed_object(case)
                .unwrap(),
            None
        );
    }
    fs::write(&version, object_wire("admission").to_string()).unwrap();
    let missing = dir.path().join("missing");
    for source in [&missing, dir.path()] {
        assert_eq!(
            catalog_write_invoke(&object_args(&db, &version, source, "3"))
                .0
                .status
                .code(),
            Some(2)
        );
    }
    assert_eq!(
        catalog_write_invoke(&object_args(&missing, &version, &file, "3"))
            .0
            .status
            .code(),
        Some(2)
    );
    assert!(!missing.exists());
    let empty = dir.path().join("empty.sqlite");
    fs::write(&empty, []).unwrap();
    assert_eq!(
        catalog_write_invoke(&object_args(&empty, &version, &file, "3"))
            .0
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        Application::open(&db)
            .unwrap()
            .asset_version("admission")
            .unwrap(),
        None
    );
}

#[test]
fn object_commit_enforces_source_cap_after_admitting_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("library.sqlite");
    Application::open(&db)
        .unwrap()
        .initialize_library()
        .unwrap();
    let version = dir.path().join("version.json");
    let file = dir.path().join("bytes");
    let bytes = [b'a'; 1024];
    let hash = "2edc986847e209b4016e141a6dc8716d3207350f416969382d431539bf292e4a";
    let id = "source-cap";
    let mut wire = object_wire(id);
    wire["content_hash"] = json!(hash);
    let metadata = wire.to_string();
    assert!(metadata.len() < bytes.len());
    fs::write(&version, &metadata).unwrap();
    fs::write(&file, bytes).unwrap();

    let expected_len = bytes.len().to_string();
    let cap = metadata.len().to_string();
    let mut args = object_args(&db, &version, &file, &expected_len);
    args.extend([OsStr::new("--max-bytes"), OsStr::new(&cap)]);
    let (out, value) = catalog_write_invoke(&args);
    assert_eq!(out.status.code(), Some(2), "{value}");
    assert_eq!(value["status"], "INVALID_INPUT");
    assert_eq!(value["errors"][0]["field_path"], "catalog", "{value}");
    {
        let mut app = Application::open(&db).unwrap();
        assert_eq!(app.asset_version(id).unwrap(), None);
        assert_eq!(app.managed_object(id).unwrap(), None);
    }

    *args.last_mut().unwrap() = OsStr::new(&expected_len);
    let (out, value) = catalog_write_invoke(&args);
    assert_eq!(out.status.code(), Some(0), "{value}");
    assert_eq!(value["status"], "PASS");
    assert_eq!(value["results"]["version_id"], id);
    assert_eq!(value["results"]["content_hash"], hash);
    assert_eq!(value["results"]["byte_length"], bytes.len());
    let mut app = Application::open(&db).unwrap();
    assert_eq!(app.asset_version(id).unwrap().unwrap().content_hash, hash);
    let object = app.managed_object(id).unwrap().unwrap();
    assert_eq!(object.content_hash, hash);
    assert_eq!(object.byte_length, bytes.len() as u64);
}
#[test]
fn object_commit_first_write_survives_broken_stream() {
    for stdout in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("library.sqlite");
        Application::open(&db)
            .unwrap()
            .initialize_library()
            .unwrap();
        let version = dir.path().join("version.json");
        let file = dir.path().join("bytes");
        fs::write(&version, object_wire("first").to_string()).unwrap();
        fs::write(&file, b"abc").unwrap();
        let args = object_args(&db, &version, &file, "3");
        let mut command = Command::new(env!("CARGO_BIN_EXE_assetweft"));
        command.args(&args);
        if stdout {
            command.stdout(broken_output());
        } else {
            command.stderr(broken_output());
        }
        let out = command.output().unwrap();
        catalog_write_record(&args, &out);
        assert_eq!(out.status.code(), Some(if stdout { 6 } else { 0 }));
        let mut app = Application::open(&db).unwrap();
        assert!(app.asset_version("first").unwrap().is_some());
        assert_eq!(app.managed_object("first").unwrap().unwrap().byte_length, 3);
    }
}

fn progress_call(db: &Path, run: &str, action: &str, extra: &[&OsStr]) -> (Output, Value) {
    let mut args = workflow_args(action, db.as_os_str(), OsStr::new(run));
    args.extend_from_slice(extra);
    args.extend([OsStr::new("--call-id"), OsStr::new("progress α")]);
    catalog_write_invoke(&args)
}
fn progress_setup(dir: &Path, run: &str) -> PathBuf {
    let db = dir.join("progress.sqlite");
    assert_eq!(
        invoke(&[
            OsStr::new("library"),
            OsStr::new("init"),
            OsStr::new("--db"),
            db.as_os_str()
        ])
        .0
        .status
        .code(),
        Some(0)
    );
    let steps = dir.join("steps.json");
    fs::write(&steps, r#"[{"id":" a α ","dependencies":[]},{"id":"b","dependencies":[" a α "]},{"id":"c","dependencies":[]}]"#).unwrap();
    assert_eq!(
        progress_call(
            &db,
            run,
            "create",
            &[OsStr::new("--steps"), steps.as_os_str()]
        )
        .0
        .status
        .code(),
        Some(0)
    );
    db
}
fn progress_finish(db: &Path, run: &str, step: &str, path: &Path) -> (Output, Value) {
    progress_call(
        db,
        run,
        "finish",
        &[
            OsStr::new("--step-id"),
            OsStr::new(step),
            OsStr::new("--outcome"),
            path.as_os_str(),
        ],
    )
}
#[test]
fn progress_restart_order_and_exact_outcomes() {
    let dir = tempfile::tempdir().unwrap();
    let run = " run α ";
    let db = progress_setup(dir.path(), run);
    let path = dir.path().join("outcome.json");
    for (id, wire) in [
        (" a α ", json!({"state":"succeeded","raw":"\n 原样 \n"})),
        ("b", json!({"state":"succeeded","raw":"","evidence":null})),
        ("c", json!({"state":"succeeded","raw":"尾","evidence":""})),
    ] {
        let (out, v) = progress_call(&db, run, "claim", &[]);
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert_eq!(
            v["results"],
            json!({"scope":"persisted_workflow_step_claim","run_id":run,"step_id":id})
        );
        assert_eq!(v["call_id"], "progress α");
        assert_eq!(out.stdout.iter().filter(|b| **b == b'\n').count(), 1);
        assert_eq!(
            progress_call(&db, run, "claim", &[]).1["results"]["step_id"],
            Value::Null
        );
        let (_, status) = progress_call(&db, run, "status", &[]);
        assert!(
            status["results"]["steps"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["id"] == id && s["state"] == "running")
        );
        fs::write(&path, wire.to_string()).unwrap();
        let (out, ack) = progress_finish(&db, run, id, &path);
        assert_eq!(out.status.code(), Some(0), "{ack}");
        assert_eq!(ack["command"], "workflow finish");
        assert_eq!(ack["results"]["scope"], "persisted_workflow_step_outcome");
        assert_eq!(
            ack["results"]["outcome_source"],
            "caller_supplied_not_executed_or_verified"
        );
        assert_eq!(ack["results"]["outcome"]["raw"], wire["raw"]);
        assert_eq!(ack["results"]["outcome"]["evidence"], wire["evidence"]);
        let before = progress_call(&db, run, "status", &[]).1;
        assert_eq!(
            progress_finish(&db, run, id, &path).0.status.code(),
            Some(1)
        );
        assert_eq!(progress_call(&db, run, "status", &[]).1, before);
    }
    assert_eq!(
        progress_call(&db, run, "claim", &[]).1["results"]["step_id"],
        Value::Null
    );
}
#[test]
fn progress_failure_cancel_and_state_errors() {
    for cancel in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let db = progress_setup(dir.path(), "r");
        let path = dir.path().join("outcome.json");
        fs::write(
            &path,
            r#"{"state":"failed","raw":" actual failure ","evidence":"证据\n指针"}"#,
        )
        .unwrap();
        for (run, step) in [("missing", " a α "), ("r", "missing"), ("r", "b")] {
            assert_eq!(
                progress_finish(&db, run, step, &path).0.status.code(),
                Some(1)
            );
        }
        assert_eq!(
            progress_call(&db, "missing", "claim", &[]).0.status.code(),
            Some(1)
        );
        assert_eq!(
            progress_call(&db, "r", "claim", &[]).0.status.code(),
            Some(0)
        );
        if cancel {
            assert_eq!(
                progress_call(&db, "r", "cancel", &[]).0.status.code(),
                Some(0)
            );
        }
        assert_eq!(
            progress_finish(&db, "r", " a α ", &path).0.status.code(),
            Some(0)
        );
        let (_, v) = progress_call(&db, "r", "status", &[]);
        assert_eq!(v["results"]["steps"][0]["state"], "failed");
        assert_eq!(v["results"]["steps"][0]["evidence"], "证据\n指针");
        let next = progress_call(&db, "r", "claim", &[]).1;
        assert_eq!(
            next["results"]["step_id"],
            if cancel { Value::Null } else { json!("c") }
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let db = progress_setup(dir.path(), "cancel-only");
    assert_eq!(
        progress_call(&db, "cancel-only", "cancel", &[])
            .0
            .status
            .code(),
        Some(0)
    );
    assert_eq!(
        progress_call(&db, "cancel-only", "claim", &[]).1["results"]["step_id"],
        Value::Null
    );
    assert_eq!(
        Application::open(&db)
            .unwrap()
            .workflow_steps("cancel-only")
            .unwrap()[0]
            .state,
        State::Pending
    );
}
#[test]
fn progress_strict_wire_and_core_byte_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let db = progress_setup(dir.path(), "r");
    assert_eq!(
        progress_call(&db, "r", "claim", &[]).0.status.code(),
        Some(0)
    );
    let path = dir.path().join("outcome.json");
    for wire in [
        "[]",
        "{}",
        r#"{"raw":""}"#,
        r#"{"state":"succeeded"}"#,
        r#"{"state":"succeeded","raw":"","x":0}"#,
        r#"{"state":"succeeded","state":"failed","raw":""}"#,
        r#"{"state":"succeeded","raw":"","raw":"x"}"#,
        r#"{"state":"succeeded","raw":"","evidence":null,"evidence":""}"#,
        r#"{"state":{},"raw":""}"#,
        r#"{"state":{"Success":{}},"raw":""}"#,
        r#"{"state":"Success","raw":""}"#,
        r#"{"state":"succeeded ","raw":""}"#,
        r#"{"state":null,"raw":""}"#,
        r#"{"state":"failed","raw":1}"#,
        r#"{"state":"failed","raw":null}"#,
        r#"{"state":"failed","raw":"","evidence":3}"#,
    ] {
        fs::write(&path, wire).unwrap();
        assert_eq!(
            progress_finish(&db, "r", " a α ", &path).0.status.code(),
            Some(2),
            "{wire}"
        );
        assert_eq!(
            Application::open(&db).unwrap().workflow_steps("r").unwrap()[0].state,
            State::Running
        );
    }
    for (raw, evidence, code) in [
        ("é".repeat(524289), "".to_owned(), 1),
        ("".to_owned(), "é".repeat(2049), 1),
        ("é".repeat(524288), "é".repeat(2048), 0),
    ] {
        fs::write(
            &path,
            json!({"state":"succeeded","raw":raw,"evidence":evidence}).to_string(),
        )
        .unwrap();
        assert_eq!(
            progress_finish(&db, "r", " a α ", &path).0.status.code(),
            Some(code)
        );
        if code == 1 {
            assert_eq!(
                Application::open(&db).unwrap().workflow_steps("r").unwrap()[0].state,
                State::Running
            );
        } else {
            assert_eq!(
                Application::open(&db).unwrap().workflow_steps("r").unwrap()[0].outcome,
                Some(Outcome::Success {
                    raw,
                    evidence: Some(evidence)
                })
            );
        }
    }
    assert_eq!(
        progress_call(
            &db,
            "r",
            "finish",
            &[
                OsStr::new("--step-id"),
                OsStr::new("b"),
                OsStr::new("--outcome"),
                path.as_os_str(),
                OsStr::new("--max-bytes"),
                OsStr::new("2")
            ]
        )
        .0
        .status
        .code(),
        Some(2)
    );
}
#[test]
fn progress_first_mutations_survive_broken_streams() {
    for stdout in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let db = progress_setup(dir.path(), "r");
        let path = dir.path().join("outcome.json");
        fs::write(
            &path,
            r#"{"state":"succeeded","raw":"actual","evidence":"unverified"}"#,
        )
        .unwrap();
        for action in ["claim", "finish"] {
            let expected = if action == "claim" {
                State::Pending
            } else {
                State::Running
            };
            assert_eq!(
                Application::open(&db).unwrap().workflow_steps("r").unwrap()[0].state,
                expected
            );
            let mut args = workflow_args(action, db.as_os_str(), OsStr::new("r"));
            if action == "finish" {
                args.extend([
                    OsStr::new("--step-id"),
                    OsStr::new(" a α "),
                    OsStr::new("--outcome"),
                    path.as_os_str(),
                ]);
            }
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_assetweft"));
            cmd.args(&args);
            if stdout {
                cmd.stdout(broken_output());
            } else {
                cmd.stderr(broken_output());
            }
            let out = cmd.output().unwrap();
            catalog_write_record(&args, &out);
            assert_eq!(out.status.code(), Some(if stdout { 6 } else { 0 }));
            let row = Application::open(&db)
                .unwrap()
                .workflow_steps("r")
                .unwrap()
                .remove(0);
            assert_eq!(
                row.state,
                if action == "claim" {
                    State::Running
                } else {
                    State::Succeeded
                }
            );
            if action == "finish" {
                assert_eq!(
                    row.outcome,
                    Some(Outcome::Success {
                        raw: "actual".into(),
                        evidence: Some("unverified".into())
                    })
                );
            }
        }
    }
}

#[test]
fn catalog_feedback_roundtrip_preserves_exact_versions_and_original() {
    use assetweft_core::history::HistoryQuery;

    let dir = tempfile::tempdir().unwrap();
    let (db, mut app, _) = candidate_fixture(dir.path(), false);
    let head = app.candidate_current("character").unwrap().unwrap();
    let original_version = app.asset_version("base").unwrap().unwrap();
    let original = dir
        .path()
        .join("objects")
        .join(&original_version.content_hash);
    let bytes = fs::read(&original).unwrap();
    let long_id = "v".repeat(257);
    let mut long_version = original_version.clone();
    long_version.version_id = long_id.clone();
    app.add_asset_version(&long_version).unwrap();
    app.close().unwrap();

    let run = |action: &str, db: &Path, extra: &[&str]| {
        let mut args = vec![
            OsStr::new("--call-id"),
            OsStr::new("feedback α"),
            OsStr::new("catalog"),
            OsStr::new(action),
            OsStr::new("--db"),
            db.as_os_str(),
        ];
        args.extend(extra.iter().map(OsStr::new));
        catalog_write_invoke(&args)
    };
    let success = |action: &str, extra: &[&str], scope: &str, record: &Value| {
        let (out, value) = run(action, &db, extra);
        assert_eq!(out.status.code(), Some(0), "{value}");
        assert!(out.stderr.is_empty());
        assert_eq!(
            value,
            json!({
                "schema_version":1,"command":format!("catalog {action}"),"call_id":"feedback α",
                "status":"PASS","errors":[],"results":{"scope":scope,"feedback":record}
            })
        );
    };
    let latest = |extra: &[&str], record: &Value| {
        success(
            "feedback-latest",
            extra,
            "stored_local_version_feedback",
            record,
        );
    };
    let asset = ["--asset-id", "character"];
    let base = ["--asset-id", "character", "--version-id", "base"];
    let proposed = ["--asset-id", "character", "--version-id", "proposed"];
    latest(&asset, &Value::Null);
    let first = json!({"sequence":1,"feedback_id":"first","asset_id":"character",
        "version_id":"base","decision":"REJECT","reason":"原样 reason"});
    let second = json!({"sequence":2,"feedback_id":"second","asset_id":"character",
        "version_id":"proposed","decision":"ACCEPT","reason":null});
    fn add<'a>(
        target: &[&'a str],
        id: &'a str,
        decision: &'a str,
        reason: &[&'a str],
    ) -> Vec<&'a str> {
        let mut args = target.to_vec();
        args.extend(["--feedback-id", id, "--decision", decision]);
        args.extend_from_slice(reason);
        args
    }
    success(
        "feedback-add",
        &add(&base, "first", "REJECT", &["--reason", "原样 reason"]),
        "appended_local_version_feedback",
        &first,
    );
    latest(&proposed, &Value::Null);
    success(
        "feedback-add",
        &add(&proposed, "second", "ACCEPT", &[]),
        "appended_local_version_feedback",
        &second,
    );
    let mismatch = ["--asset-id", "character", "--version-id", "mesh0"];
    let long = ["--asset-id", "character", "--version-id", long_id.as_str()];
    for (args, message) in [
        (
            add(&mismatch, "wrong", "ACCEPT", &[]),
            "feedback version/asset mismatch",
        ),
        (
            add(&proposed, "first", "ACCEPT", &[]),
            "UNIQUE constraint failed: aw_feedback.feedback_id",
        ),
        (add(&base, &long_id, "ACCEPT", &[]), "feedback identity"),
        (
            add(&base, "blank", "ACCEPT", &["--reason", " "]),
            "feedback reason",
        ),
    ] {
        let (out, value) = run("feedback-add", &db, &args);
        assert_eq!(out.status.code(), Some(2), "{value}");
        assert_eq!(value["status"], "INVALID_INPUT");
        assert_eq!(value["results"], Value::Null);
        assert_eq!(value["errors"].as_array().unwrap().len(), 1);
        let error = &value["errors"][0];
        assert_eq!(error["field_path"], "catalog");
        assert_eq!(error["code"], "INVALID_INPUT");
        assert!(
            error["message"].as_str().unwrap().contains(message),
            "{value}"
        );
    }
    latest(&long, &Value::Null);
    latest(&mismatch, &Value::Null);
    latest(&asset, &second);
    latest(&base, &first);
    latest(&proposed, &second);
    let third = json!({"sequence":3,"feedback_id":"long","asset_id":"character",
        "version_id":long_id,"decision":"ACCEPT","reason":null});
    success(
        "feedback-add",
        &add(&long, "long", "ACCEPT", &[]),
        "appended_local_version_feedback",
        &third,
    );
    latest(&long, &third);
    latest(&asset, &third);

    let missing = dir.path().join("missing.sqlite");
    for (action, args) in [
        ("feedback-latest", asset.to_vec()),
        ("feedback-add", add(&base, "missing", "ACCEPT", &[])),
    ] {
        let (out, value) = run(action, &missing, &args);
        assert_eq!(out.status.code(), Some(2), "{value}");
        assert_eq!(value["errors"][0]["field_path"], "db");
        assert!(!missing.exists());
    }

    let mut app = Application::open(&db).unwrap();
    for (asset_id, count) in [("character", 3), ("mesh0", 0)] {
        let history = app
            .feedback_history(HistoryQuery {
                asset_id,
                version_id: None,
                before_sequence: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(history.len(), count);
    }
    assert_eq!(app.candidate_current("character").unwrap(), Some(head));
    assert_eq!(app.asset_version("base").unwrap(), Some(original_version));
    assert_eq!(app.asset_version(&long_id).unwrap(), Some(long_version));
    assert_eq!(fs::read(original).unwrap(), bytes);
}

#[cfg(any(unix, windows))]
#[test]
fn feedback_get_recovers_exact_id_after_lost_append_receipt() {
    for committed in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let (db, app, _) = candidate_fixture(dir.path(), false);
        app.close().unwrap();

        let blocker = (!committed).then(|| {
            let conn = assetweft_core::sqlite::open_and_configure(&db).unwrap().0;
            conn.execute_batch("BEGIN IMMEDIATE").unwrap();
            conn
        });
        let lost = Command::new(env!("CARGO_BIN_EXE_assetweft"))
            .args(["catalog", "feedback-add", "--db"])
            .arg(&db)
            .args([
                "--asset-id",
                "character",
                "--version-id",
                "base",
                "--feedback-id",
                "A",
                "--decision",
                "REJECT",
                "--reason",
                "原样 A",
            ])
            .stdout(broken_output())
            .output()
            .unwrap();
        assert_eq!(lost.status.code(), Some(6), "{lost:?}");
        assert!(lost.stdout.is_empty());
        drop(blocker);

        let run = |action: &str, asset: &str, version: &str, extra: &[&str]| {
            let mut args = vec![
                OsStr::new("catalog"),
                OsStr::new(action),
                OsStr::new("--db"),
                db.as_os_str(),
            ];
            args.extend(["--asset-id", asset, "--version-id", version].map(OsStr::new));
            args.extend(extra.iter().map(OsStr::new));
            let (out, v) = invoke(&args);
            assert_eq!(out.status.code(), Some(0), "{v}");
            assert!(out.stderr.is_empty(), "{out:?}");
            assert_eq!(v["command"], format!("catalog {action}"));
            assert_eq!(v["status"], "PASS");
            assert_eq!(v["errors"], json!([]));
            v["results"].clone()
        };
        let second = run(
            "feedback-add",
            "character",
            "base",
            &["--feedback-id", "B", "--decision", "ACCEPT"],
        );
        assert_eq!(second["feedback"]["feedback_id"], "B");
        assert_eq!(
            run("feedback-latest", "character", "base", &[])["feedback"],
            second["feedback"]
        );

        let exact = run("feedback-get", "character", "base", &["--feedback-id", "A"]);
        assert_eq!(exact["scope"], "stored_local_version_feedback");
        let expected = if committed {
            json!({"sequence":1,"feedback_id":"A","asset_id":"character",
                "version_id":"base","decision":"REJECT","reason":"原样 A"})
        } else {
            Value::Null
        };
        assert_eq!(exact["feedback"], expected);
        for (asset, version) in [("character", "proposed"), ("mesh0", "base")] {
            assert_eq!(
                run("feedback-get", asset, version, &["--feedback-id", "A"])["feedback"],
                Value::Null
            );
        }
    }
}

fn export_fixture(parent: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let root = parent.join("library");
    fs::create_dir(&root).unwrap();
    let db = root.join("library.sqlite");
    Application::open(&db)
        .unwrap()
        .initialize_library()
        .unwrap();
    let file = parent.join("original");
    let version = parent.join("version.json");
    fs::write(&file, b"abc").unwrap();
    for id in ["first", "second"] {
        fs::write(&version, object_wire(id).to_string()).unwrap();
        let (out, value) = catalog_write_invoke(&object_args(&db, &version, &file, "3"));
        assert_eq!(out.status.code(), Some(0), "{value}");
    }
    let cas = root
        .join("objects")
        .join(object_wire("first")["content_hash"].as_str().unwrap());
    (db, file, cas)
}

fn export_call(db: &Path, action: &str, id: &str) -> Value {
    let (out, v) = catalog_write_invoke(&[
        OsStr::new("catalog"),
        OsStr::new(action),
        OsStr::new("--db"),
        db.as_os_str(),
        OsStr::new("--version-id"),
        OsStr::new(id),
    ]);
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert_eq!(v["command"], format!("catalog {action}"));
    assert_eq!(v["results"]["version_id"], id);
    v["results"].clone()
}

#[test]
fn catalog_export_roundtrip_preserves_library() {
    use assetweft_core::{
        history::{Decision, Feedback},
        library::{ComponentReference, ComponentRole},
    };
    let dir = tempfile::tempdir().unwrap();
    let (db, file, cas) = export_fixture(dir.path());
    let root = db.parent().unwrap();
    let mut app = Application::open(&db).unwrap();
    let version = app.asset_version("first").unwrap().unwrap();
    let reference = ComponentReference {
        owner_version_id: "first".into(),
        role: ComponentRole::Mesh,
        component_version_id: "second".into(),
    };
    app.add_component_reference(&reference).unwrap();
    let feedback = app
        .append_feedback(&Feedback {
            feedback_id: "review".into(),
            asset_id: version.asset_id.clone(),
            version_id: version.version_id.clone(),
            decision: Decision::Reject,
            reason: Some("keep history".into()),
        })
        .unwrap();
    app.close().unwrap();
    let database_before = fs::read(&db).unwrap();
    let export = |id| {
        root.join(
            export_call(&db, "materialize-export", id)["relative_path"]
                .as_str()
                .unwrap(),
        )
    };
    let evict = || {
        export_call(&db, "evict-export", "first")["removed"]
            .as_bool()
            .unwrap()
    };
    assert!(!evict());
    let target = export("first");
    assert_eq!(target.parent(), Some(root.join("exports").as_path()));
    assert_eq!(export("first"), target);
    let other = export("second");
    assert_ne!(target, other);
    let unknown = root.join("exports/user-notes");
    fs::write(&unknown, b"keep").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"abc");
    assert!(evict());
    assert!(!target.try_exists().unwrap());
    assert!(!evict());
    for keep in [&other, &file, &cas] {
        assert_eq!(fs::read(keep).unwrap(), b"abc");
    }
    assert_eq!(fs::read(&unknown).unwrap(), b"keep");
    assert_eq!(export("first"), target);
    assert_eq!(fs::read(&target).unwrap(), fs::read(&cas).unwrap());
    assert_eq!(fs::read(&db).unwrap(), database_before);
    let mut app = Application::open(&db).unwrap();
    assert_eq!(app.asset_version("first").unwrap(), Some(version.clone()));
    assert_eq!(app.component_references("first").unwrap(), [reference]);
    assert_eq!(
        app.latest_feedback(&version.asset_id, Some("first"))
            .unwrap(),
        Some(feedback)
    );
    assert_eq!(
        app.managed_object("first").unwrap().unwrap().content_hash,
        version.content_hash
    );
}

#[test]
fn catalog_export_refuses_changed_unowned_and_unrecoverable_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let (db, source, cas) = export_fixture(dir.path());
    let root = db.parent().unwrap();
    let mut app = Application::open(&db).unwrap();
    let target = root.join(app.materialize_export("first").unwrap());
    let refuse = |app: &Application| {
        assert!(app.evict_export("first").is_err());
        assert!(app.materialize_export("first").is_err());
    };
    fs::write(&target, b"xyz").unwrap();
    refuse(&app);
    assert_eq!(fs::read(&target).unwrap(), b"xyz");
    for keep in [&source, &cas] {
        assert_eq!(fs::read(keep).unwrap(), b"abc");
    }
    fs::write(&target, b"abc").unwrap();
    for corrupt in [true, false] {
        if corrupt {
            fs::write(&cas, b"bad").unwrap();
        } else {
            fs::remove_file(&cas).unwrap();
        }
        refuse(&app);
        assert_eq!(fs::read(&target).unwrap(), b"abc");
    }
    fs::write(&cas, b"abc").unwrap();
    let lock = root.join(".aw-storage.lock");
    fs::write(&lock, b"in use").unwrap();
    refuse(&app);
    assert_eq!(fs::read(&lock).unwrap(), b"in use");
    fs::remove_file(lock).unwrap();

    let moved = root.join("retained-export");
    fs::rename(&target, &moved).unwrap();
    fs::hard_link(&source, &target).unwrap();
    refuse(&app);
    for keep in [&moved, &target, &source, &cas] {
        assert_eq!(fs::read(keep).unwrap(), b"abc");
    }
    let mut metadata = app.asset_version("first").unwrap().unwrap();
    metadata.version_id = "metadata-only".into();
    app.add_asset_version(&metadata).unwrap();
    assert!(app.materialize_export("metadata-only").is_err());
    assert!(app.evict_export("metadata-only").is_err());
}

#[cfg(unix)]
#[test]
fn catalog_export_refuses_redirected_parents() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let (db, source, cas) = export_fixture(dir.path());
    let root = db.parent().unwrap();
    let app = Application::open(&db).unwrap();
    let relative = app.materialize_export("first").unwrap();
    for name in ["exports", "objects"] {
        let inside = root.join(name);
        let outside = dir.path().join("moved");
        fs::rename(&inside, &outside).unwrap();
        symlink(&outside, &inside).unwrap();
        assert!(app.evict_export("first").is_err());
        assert!(app.materialize_export("second").is_err());
        fs::remove_file(&inside).unwrap();
        fs::rename(outside, inside).unwrap();
    }
    let alias = dir.path().join("alias.sqlite");
    symlink(&db, &alias).unwrap();
    assert!(
        Application::open(&alias)
            .unwrap()
            .evict_export("first")
            .is_err()
    );
    assert_eq!(fs::read(root.join(&relative)).unwrap(), b"abc");
    assert_eq!(fs::read(cas).unwrap(), fs::read(source).unwrap());
}

#[test]
fn catalog_export_partial_publication_never_adopts_or_sweeps() {
    let dir = tempfile::tempdir().unwrap();
    let (db, source, cas) = export_fixture(dir.path());
    let root = db.parent().unwrap();
    let app = Application::open(&db).unwrap();
    let target = root.join(app.materialize_export("first").unwrap());
    let witness = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".aw-export-")
        })
        .unwrap();

    let retained = root.join("retained");
    fs::rename(&witness, &retained).unwrap();
    assert!(app.evict_export("first").is_err());
    assert!(app.materialize_export("first").is_err());
    assert_eq!(fs::read(&target).unwrap(), b"abc");
    fs::rename(retained, &witness).unwrap();

    fs::remove_file(&target).unwrap();
    assert!(!app.evict_export("first").unwrap());
    assert_eq!(root.join(app.materialize_export("first").unwrap()), target);
    assert!(app.evict_export("first").unwrap());
    assert!(!target.try_exists().unwrap());
    for keep in [witness, source, cas] {
        assert_eq!(fs::read(keep).unwrap(), b"abc");
    }
}

#[test]
fn catalog_file_availability_observes_exact_local_bytes_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("library.sqlite");
    let query = |id: &str| {
        invoke(&catalog_args(
            "file-availability",
            db.as_os_str(),
            OsStr::new(id),
        ))
    };
    let observe = |id: &str, expected: &str| {
        catalog_query(
            &db,
            "file-availability",
            id,
            json!({"scope":"local_managed_version_bytes","version_id":id,"availability":expected}),
        );
    };
    let (out, value) = query("absent");
    assert_eq!(out.status.code(), Some(2), "{value}");
    assert_eq!(value["command"], "catalog file-availability");
    assert_eq!(value["errors"][0]["field_path"], "db");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);

    fs::write(&db, []).unwrap();
    let (out, value) = query("absent");
    assert_eq!(out.status.code(), Some(3), "{value}");
    assert!(fs::read(&db).unwrap().is_empty());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);

    let (out, value) = invoke(&[
        OsStr::new("library"),
        OsStr::new("init"),
        OsStr::new("--db"),
        db.as_os_str(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{value}");
    let source_fixture = fixture("mesh_only.glb");
    let bytes = fs::read(&source_fixture).unwrap();
    let original = dir.path().join("original.glb");
    fs::write(&original, &bytes).unwrap();
    let version = dir.path().join("version.json");
    let mut wire = catalog_write_wire("metadata", "MISSING", false);
    let hash = "4658deaae78fb1ae6050baca75a8c098db6ccad311c74ba02097fceac01e45a5";
    wire["content_hash"] = json!(hash);
    wire["source_format"] = json!("GLB");
    fs::write(&version, wire.to_string()).unwrap();
    let (out, value) = invoke(&catalog_write_args("register-version", &db, &version));
    assert_eq!(out.status.code(), Some(0), "{value}");
    let owner = dir.path().join(".aw-object-owner");
    let objects = dir.path().join("objects");
    observe("metadata", "METADATA_ONLY");
    observe("absent", "UNKNOWN_VERSION");
    assert!(!owner.exists());
    assert!(!objects.exists());

    wire["version_id"] = json!("managed");
    wire["presence"] = json!("UNKNOWN");
    fs::write(&version, wire.to_string()).unwrap();
    let (out, value) = invoke(&object_args(
        &db,
        &version,
        &original,
        &bytes.len().to_string(),
    ));
    assert_eq!(out.status.code(), Some(0), "{value}");
    let db_bytes = fs::read(&db).unwrap();
    let owner_bytes = fs::read(&owner).unwrap();
    let cas = objects.join(hash);
    observe("managed", "AVAILABLE");
    observe("metadata", "METADATA_ONLY");
    assert_eq!(fs::read(&cas).unwrap(), bytes);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let permissions = fs::metadata(&cas).unwrap().permissions();
        fs::set_permissions(&cas, fs::Permissions::from_mode(0o000)).unwrap();
        let read_probe = fs::File::open(&cas);
        let (out, value) = query("managed");
        fs::set_permissions(&cas, permissions).unwrap();
        if let Err(error) = read_probe {
            assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
            assert_eq!(out.status.code(), Some(6), "{value}");
            assert!(value["results"].is_null());
            assert!(
                value["errors"][0]["message"]
                    .as_str()
                    .unwrap()
                    .contains("PermissionDenied")
            );
        }
    }

    let mut corrupted = bytes.clone();
    corrupted[0] ^= 1;
    fs::write(&cas, &corrupted).unwrap();
    let (out, value) = query("managed");
    assert_eq!(out.status.code(), Some(1), "{value}");
    assert_eq!(value["errors"][0]["message"], "Corrupt");
    assert!(value["results"].is_null());
    assert_eq!(fs::read(&cas).unwrap(), corrupted);

    fs::remove_file(&cas).unwrap();
    observe("managed", "MISSING");
    fs::create_dir(&cas).unwrap();
    let (out, value) = query("managed");
    assert_eq!(out.status.code(), Some(1), "{value}");
    assert!(value["results"].is_null());
    fs::remove_dir(&cas).unwrap();
    fs::remove_dir(&objects).unwrap();
    observe("managed", "MISSING");
    assert!(!objects.exists());

    fs::remove_file(&owner).unwrap();
    let (out, value) = query("managed");
    assert_eq!(out.status.code(), Some(6), "{value}");
    assert!(value["results"].is_null());
    assert!(
        value["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("NotFound")
    );
    assert!(!owner.exists());
    fs::write(&owner, &owner_bytes).unwrap();

    catalog_query(
        &db,
        "version",
        "managed",
        json!({"scope":"stored_local_asset_version_metadata","version":wire}),
    );
    assert_eq!(fs::read(&db).unwrap(), db_bytes);
    assert_eq!(fs::read(&owner).unwrap(), owner_bytes);
    assert_eq!(fs::read(&original).unwrap(), bytes);
    assert_eq!(fs::read(&source_fixture).unwrap(), bytes);
    assert!(!dir.path().join(".aw-storage.lock").exists());
}

#[test]
fn catalog_search_finds_literal_version_matches_without_writes() {
    use assetweft_core::library::Presence;

    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("library.sqlite");
    let search = |query: &str| {
        let query_arg = format!("--query={query}");
        invoke(&[
            OsStr::new("--call-id"),
            OsStr::new("discovery"),
            OsStr::new("catalog"),
            OsStr::new("search"),
            OsStr::new("--db"),
            db.as_os_str(),
            OsStr::new(&query_arg),
        ])
    };
    let (out, result) = search("mesh");
    assert_eq!(result["command"], "catalog search");
    assert_eq!(out.status.code(), Some(2), "{result}");
    assert_eq!(result["errors"][0]["field_path"], "db");
    assert!(result["results"].is_null());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);

    fs::write(&db, []).unwrap();
    let (out, result) = search("mesh");
    assert_eq!(out.status.code(), Some(3), "{result}");
    assert!(result["results"].is_null());
    assert!(fs::read(&db).unwrap().is_empty());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    let mut app = Application::open(&db).unwrap();
    app.initialize_library().unwrap();

    let original = dir.path().join("original.glb");
    let bytes = fs::read(fixture("mesh_only.glb")).unwrap();
    fs::write(&original, &bytes).unwrap();
    let mut managed = catalog_version("part-雪%_' v2", Presence::Unknown);
    managed.asset_id = "a-mesh".into();
    managed.source_format = "GLB".into();
    managed.content_hash =
        "4658deaae78fb1ae6050baca75a8c098db6ccad311c74ba02097fceac01e45a5".into();
    app.commit_object(
        &managed,
        &mut bytes.as_slice(),
        bytes.len() as u64,
        bytes.len() as u64,
    )
    .unwrap();
    for (asset, id, format) in [
        ("a-mesh", "mesh-part雪%_' v1", "OBJ"),
        ("z-prop", "mesh-雪%_' v1", " WAV "),
        ("z-prop", "mesh-雪xx' v1", "PNG"),
    ] {
        let mut row = catalog_version(id, Presence::Missing);
        row.asset_id = asset.into();
        row.source_format = format.into();
        app.add_asset_version(&row).unwrap();
    }
    for n in (0..101).rev() {
        let mut row = catalog_version(&format!("batch-{n:03}"), Presence::Unknown);
        row.asset_id = "another".into();
        app.add_asset_version(&row).unwrap();
    }
    app.close().unwrap();
    let db_bytes = fs::read(&db).unwrap();
    let cas = dir.path().join("objects").join(&managed.content_hash);
    let entries = || {
        let mut paths = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        paths.sort();
        paths
    };
    let initial_entries = entries();
    let hit = |asset: &str, version: &str, format: &str| json!({"asset_id":asset,"version_id":version,"source_format":format});
    let first = hit("a-mesh", "mesh-part雪%_' v1", "OBJ");
    let second = hit("a-mesh", &managed.version_id, "GLB");
    let third = hit("z-prop", "mesh-雪%_' v1", " WAV ");
    let fourth = hit("z-prop", "mesh-雪xx' v1", "PNG");
    let check = |query: &str, matches: Value, truncated: bool| {
        let (out, result) = search(query);
        assert_eq!(out.status.code(), Some(0), "{result}");
        assert_eq!(
            result,
            json!({
                "schema_version":1,"command":"catalog search","call_id":"discovery",
                "status":"PASS","errors":[],"results":{
                    "scope":"stored_local_asset_version_search","matches":matches,"truncated":truncated
                }
            })
        );
        assert!(out.stderr.is_empty());
        result
    };
    check("mesh", json!([first, second, third, fourth]), false);
    let literal = check("-雪%_' ", json!([second, third]), false);
    check("MESH", json!([]), false);
    check("雪%_'  ", json!([]), false);
    check(
        "batch",
        json!(
            (0..100)
                .map(|n| { hit("another", &format!("batch-{n:03}"), "declared-format") })
                .collect::<Vec<_>>()
        ),
        true,
    );
    check(
        "batch-0",
        json!(
            (0..100)
                .map(|n| { hit("another", &format!("batch-{n:03}"), "declared-format") })
                .collect::<Vec<_>>()
        ),
        false,
    );
    let (out, result) = search("");
    assert_eq!(out.status.code(), Some(2), "{result}");
    assert!(result["results"].is_null());

    let id = literal["results"]["matches"][0]["version_id"]
        .as_str()
        .unwrap();
    let (out, result) = invoke(&catalog_args("version", db.as_os_str(), OsStr::new(id)));
    assert_eq!(out.status.code(), Some(0), "{result}");
    assert_eq!(result["results"]["version"]["source_format"], "GLB");
    catalog_query(
        &db,
        "file-availability",
        id,
        json!({"scope":"local_managed_version_bytes","version_id":id,"availability":"AVAILABLE"}),
    );
    assert_eq!(fs::read(&db).unwrap(), db_bytes);
    assert_eq!(fs::read(&original).unwrap(), bytes);
    assert_eq!(fs::read(&cas).unwrap(), bytes);
    assert_eq!(entries(), initial_entries);
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn add_preview_source(db: &Path, id: &str, bytes: &[u8], format: &str) -> std::path::PathBuf {
    let root = db.parent().unwrap();
    let file = root.join(format!("{id}.source"));
    fs::write(&file, bytes).unwrap();
    let hash = sha256(bytes);
    let mut wire = object_wire(id);
    wire["content_hash"] = json!(hash);
    wire["source_format"] = json!(format);
    let version = root.join("version.json");
    fs::write(&version, wire.to_string()).unwrap();
    let (out, value) =
        catalog_write_invoke(&object_args(db, &version, &file, &bytes.len().to_string()));
    assert!(out.status.success(), "{value}");
    root.join("objects").join(hash)
}

#[test]
fn catalog_preview_decodes_exact_versions_and_preserves_library() {
    use png::{BitDepth, ColorType, Decoder, Encoder};
    use std::io::Cursor;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let db = root.join("library.sqlite");
    Application::open(&db)
        .unwrap()
        .initialize_library()
        .unwrap();
    let add = |id: &str, bytes: &[u8], format: &str| add_preview_source(&db, id, bytes, format);
    let make = |blue, animated, gamma| {
        let mut bytes = Vec::new();
        let mut encoder = Encoder::new(&mut bytes, 256, 64);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        if let Some(gamma) = gamma {
            encoder.set_source_gamma(png::ScaledFloat::from_scaled(gamma));
        }
        if animated {
            encoder.set_animated(1, 0).unwrap();
        }
        let mut writer = encoder.write_header().unwrap();
        let mut pixels = Vec::new();
        for y in 0..64 {
            for x in 0..256 {
                pixels.extend_from_slice(&[x as u8, y, blue, x as u8]);
            }
        }
        writer.write_image_data(&pixels).unwrap();
        writer.finish().unwrap();
        bytes
    };
    let query = |id: &str| invoke(&catalog_args("preview", db.as_os_str(), OsStr::new(id)));
    let fail = |id: &str| {
        let (out, result) = query(id);
        assert!(!out.status.success(), "{id} {result}");
        assert!(result["results"].is_null());
    };
    let expect_error = |id, exit, status, diagnostic: &str| {
        let (out, result) = query(id);
        assert_eq!(out.status.code(), Some(exit), "{id} {result}");
        assert_eq!(result["status"], status);
        assert_eq!(result["errors"][0]["code"], status);
        assert_eq!(result["errors"][0]["field_path"], "catalog");
        assert!(
            result["errors"][0]["message"]
                .as_str()
                .unwrap()
                .contains(diagnostic),
            "{id} {result}"
        );
        assert!(result["results"].is_null());
    };
    let first = make(27, false, None);
    let cas = add("first", &first, "PNG");
    add("second", &make(91, false, Some(100_000)), "PNG");
    add("animated", &make(0, true, None), "PNG");
    add("short", &first[..first.len() - 6], "PNG");
    let mut corrupt = first.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    add("crc", &corrupt, "PNG");
    add("unsupported", &first, "GLB");
    add("fake", b"not an image", "PNG");
    let mut wide = Vec::new();
    let mut encoder = Encoder::new(&mut wide, 4097, 1);
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&vec![0; 4097 * 4]).unwrap();
    writer.finish().unwrap();
    add("wide", &wide, "PNG");
    let db_before = fs::read(&db).unwrap();
    expect_error("unknown", 2, "INVALID_INPUT", "unknown version");
    fs::remove_file(&cas).unwrap();
    expect_error(
        "first",
        3,
        "INPUT_INCOMPLETE",
        "managed source file is missing",
    );
    assert!(!cas.exists());
    assert!(!root.join("previews").exists());
    fs::write(&cas, &first).unwrap();
    for id in ["animated", "short", "crc", "unsupported", "fake", "wide"] {
        fail(id);
    }
    assert!(!root.join("previews").exists());
    let preview_dir = root.join("previews");
    fs::create_dir(&preview_dir).unwrap();
    let legacy = preview_dir.join(format!("{}.png", sha256(b"second")));
    fs::write(&legacy, b"legacy or user-owned").unwrap();
    let mut targets = Vec::new();
    for (id, blue) in [("first", 27), ("second", 91)] {
        let result = export_call(&db, "preview", id);
        let target = root.join(result["relative_path"].as_str().unwrap());
        assert_eq!(target.parent(), Some(root.join("previews").as_path()));
        assert_eq!(
            target,
            preview_dir.join(format!("{}.v2.png", sha256(id.as_bytes())))
        );
        let encoded = fs::read(&target).unwrap();
        let mut reader = Decoder::new(Cursor::new(&encoded)).read_info().unwrap();
        assert_eq!(
            reader.info().gama_chunk.map(|v| v.into_scaled()),
            (id == "second").then_some(100_000)
        );
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!((info.width, info.height), (128, 32));
        for y in 0..32 {
            for x in 0..128 {
                let i = (y * 128 + x) * 4;
                assert_eq!(
                    pixels[i..i + 4],
                    [(x * 2) as u8, (y * 2) as u8, blue, (x * 2) as u8]
                );
            }
        }
        assert_eq!(export_call(&db, "preview", id), result);
        targets.push(target);
    }
    let thumbnail = fs::read(&targets[0]).unwrap();
    let other_thumbnail = fs::read(&targets[1]).unwrap();
    assert_eq!(fs::read(legacy).unwrap(), b"legacy or user-owned");
    fs::write(&targets[0], b"user file").unwrap();
    expect_error("first", 1, "FAIL", "Corrupt");
    fs::remove_file(&cas).unwrap();
    expect_error(
        "first",
        3,
        "INPUT_INCOMPLETE",
        "managed source file is missing",
    );
    assert!(!cas.exists());
    assert_eq!(fs::read(&targets[0]).unwrap(), b"user file");
    fs::remove_file(&targets[0]).unwrap();
    fs::write(&cas, &corrupt).unwrap();
    expect_error("first", 1, "FAIL", "Corrupt");
    assert_eq!(fs::read(&cas).unwrap(), corrupt);
    assert!(!targets[0].exists());

    fs::remove_file(&cas).unwrap();
    let owner = root.join(".aw-object-owner");
    let owner_bytes = fs::read(&owner).unwrap();
    fs::remove_file(&owner).unwrap();
    expect_error("first", 6, "INTERNAL", "NotFound");
    assert!(!targets[0].exists());
    assert!(!owner.exists());
    fs::write(&owner, owner_bytes).unwrap();

    let objects = cas.parent().unwrap();
    let displaced = root.join("displaced-objects");
    fs::rename(objects, &displaced).unwrap();
    expect_error("first", 6, "INTERNAL", "NotFound");
    assert!(!objects.exists());
    fs::rename(&displaced, objects).unwrap();

    fs::write(&cas, &first).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = fs::metadata(&cas).unwrap().permissions();
        fs::set_permissions(&cas, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::File::open(&cas).is_err_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied) {
            expect_error("first", 6, "INTERNAL", "PermissionDenied");
        }
        fs::set_permissions(&cas, permissions).unwrap();
        assert!(!targets[0].exists());
    }
    export_call(&db, "preview", "first");
    assert_eq!(fs::read(&targets[0]).unwrap(), thumbnail);
    assert_eq!(fs::read(&targets[1]).unwrap(), other_thumbnail);
    assert_eq!(fs::read(&cas).unwrap(), first);
    assert_eq!(fs::read(root.join("first.source")).unwrap(), first);
    assert_eq!(fs::read(db).unwrap(), db_before);
}

#[test]
fn catalog_preview_wav_uses_complete_frames_of_the_requested_version() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("library.sqlite");
    Application::open(&db)
        .unwrap()
        .initialize_library()
        .unwrap();
    let wave = |channels: u16, samples: &[i16]| {
        let size = (samples.len() * 2) as u32;
        let mut bytes = b"RIFF".to_vec();
        bytes.extend((36 + size).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(channels.to_le_bytes());
        bytes.extend(8000u32.to_le_bytes());
        bytes.extend((16000 * u32::from(channels)).to_le_bytes());
        bytes.extend((2 * channels).to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(size.to_le_bytes());
        bytes.extend(samples.iter().flat_map(|s| s.to_le_bytes()));
        bytes
    };
    let source = wave(1, &[-107, 17, 85]);
    let stereo = (0..24000)
        .flat_map(|i| {
            [
                if i % 20 < 10 { 8000 } else { -8000 },
                if i % 40 < 20 { 5000 } else { -5000 },
            ]
        })
        .collect::<Vec<_>>();
    let other = wave(2, &stereo);
    let cas = add_preview_source(&db, "one", &source, "WAV");
    add_preview_source(&db, "two", &other, "wav");
    for offset in [4, 16, 20, 22, 26, 28, 32, 34, 36, 40] {
        let mut invalid = source.clone();
        invalid[offset] ^= 0x10;
        let id = format!("invalid-{offset}");
        add_preview_source(&db, &id, &invalid, "WAV");
        let (out, result) = invoke(&catalog_args("preview", db.as_os_str(), OsStr::new(&id)));
        assert_eq!(out.status.code(), Some(2), "{result}");
        assert!(result["results"].is_null());
    }
    assert!(!dir.path().join("previews").exists());
    let before = fs::read(&db).unwrap();
    for (id, expected) in [
        ("two", wave(2, &stereo[..16000])),
        ("one", wave(1, &[-107])),
    ] {
        let result = export_call(&db, "preview", id);
        assert_eq!(result["scope"], "local_version_wav_preview");
        let path = dir.path().join(result["relative_path"].as_str().unwrap());
        assert_eq!(
            path,
            dir.path()
                .join("previews")
                .join(format!("{}.v1.wav", sha256(id.as_bytes())))
        );
        assert_eq!(fs::read(&path).unwrap(), expected);
        assert_eq!(export_call(&db, "preview", id), result);
    }
    assert_eq!(fs::read(cas).unwrap(), source);
    assert_eq!(fs::read(dir.path().join("one.source")).unwrap(), source);
    assert_eq!(fs::read(db).unwrap(), before);
}

#[test]
fn catalog_preview_glb_draws_transformed_geometry_for_each_version() {
    let root = tempfile::tempdir().unwrap();
    let db = root.path().join("library.sqlite");
    Application::open(&db)
        .unwrap()
        .initialize_library()
        .unwrap();
    let original = include_bytes!("../../../examples/godot-static/triangle.glb");
    let mut changed = original.to_vec();
    let json_len = u32::from_le_bytes(original[12..16].try_into().unwrap()) as usize;
    let x = 28 + json_len + 24;
    changed[x..x + 4].copy_from_slice(&1.0f32.to_le_bytes());
    let a = add_preview_source(&db, "left", original, "GLB");
    let b = add_preview_source(&db, "right", &changed, "glb");
    let mut invalid = original.to_vec();
    invalid[x + 12..x + 14].copy_from_slice(&9u16.to_le_bytes());
    add_preview_source(&db, "bad-index", &invalid, "GLB");
    let mut unsupported = original.to_vec();
    let scene = original
        .windows(10)
        .position(|w| w == br#""scene":0,"#)
        .unwrap();
    unsupported[scene..scene + 10].fill(b' ');
    add_preview_source(&db, "no-scene", &unsupported, "GLB");
    let db_before = fs::read(&db).unwrap();
    for id in ["bad-index", "no-scene"] {
        let (out, value) = invoke(&catalog_args("preview", db.as_os_str(), OsStr::new(id)));
        assert_eq!(out.status.code(), Some(2), "{value}");
        assert!(value["results"].is_null());
        assert!(!root.path().join("previews").exists());
    }
    for (id, bytes, cas) in [("left", original.as_slice(), a), ("right", &changed[..], b)] {
        let value = export_call(&db, "preview", id);
        assert_eq!(value["scope"], "local_version_png_preview");
        let target = root.path().join(value["relative_path"].as_str().unwrap());
        assert_eq!(
            target,
            root.path()
                .join("previews")
                .join(format!("{}.glb-v1.png", sha256(id.as_bytes())))
        );
        let image = fs::read(&target).unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(&image))
            .read_info()
            .unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!(
            (info.width, info.height, info.color_type),
            (128, 128, png::ColorType::Rgba)
        );
        for y in 0..128 {
            for x in 0..128 {
                let u = 224 + 3 * (2 * x + 1 - 128);
                let v = 2 * (240 - 2 * y - 1);
                let inside = (0..=448).contains(&u)
                    && (0..=448).contains(&v)
                    && if id == "left" { u + v <= 448 } else { u >= v };
                let i = ((y * 128 + x) * 4) as usize;
                assert_eq!(
                    pixels[i..i + 4],
                    if inside { [0, 255, 0, 255] } else { [0; 4] },
                    "{id} {x},{y}"
                );
            }
        }
        assert_eq!(export_call(&db, "preview", id), value);
        fs::write(&target, b"conflict").unwrap();
        let (out, _) = invoke(&catalog_args("preview", db.as_os_str(), OsStr::new(id)));
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(fs::read(target).unwrap(), b"conflict");
        assert_eq!(fs::read(cas).unwrap(), bytes);
        assert_eq!(
            fs::read(root.path().join(format!("{id}.source"))).unwrap(),
            bytes
        );
    }
    assert_eq!(fs::read(db).unwrap(), db_before);
}

#[test]
fn rig_bend_rejects_other_bytes_before_launch_or_staging() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("new-evidence");
    let (process, reply) = invoke(&[
        OsStr::new("rig-bend"),
        fixture("mesh_only.glb").as_os_str(),
        OsStr::new("--blender"),
        dir.path().join("absent-blender").as_os_str(),
        OsStr::new("--out"),
        out.as_os_str(),
    ]);
    assert_eq!(process.status.code(), Some(1));
    assert_eq!(reply["status"], "NOT_RUN");
    assert!(
        reply["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("SHA-256")
    );
    assert!(!out.exists());
}

#[test]
fn motion_strike_writes_only_a_new_supported_glb() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let input =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rigged-simple/RiggedSimple.glb");
    let output = dir.path().join("strike.glb");
    let run = |file: &Path, out: &Path| {
        invoke(&[
            OsStr::new("motion-strike"),
            file.as_os_str(),
            OsStr::new("--out"),
            out.as_os_str(),
        ])
    };
    let (p, r) = run(&input, &output);
    assert!(p.status.success(), "{r}");
    assert_eq!(r["results"]["clip"], "AssetWeft_InPlaceStrike");
    let bytes = fs::read(&output).unwrap();
    let hash: String = Sha256::digest(&bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect();
    assert_eq!(r["results"]["output_sha256"], hash);
    assert_eq!(
        hash,
        "20f7e1596107d9b875ea6b276c1d79aeb5c98c3a9ca3ec9054bc2cd57d4eb90b"
    );
    let (p, r) = invoke(&[
        OsStr::new("validate"),
        output.as_os_str(),
        OsStr::new("--role"),
        OsStr::new("animated-character"),
    ]);
    assert!(p.status.success(), "{r}");
    fs::write(&output, b"existing").unwrap();
    assert!(!run(&input, &output).0.status.success());
    assert_eq!(fs::read(&output).unwrap(), b"existing");
    let rejected = dir.path().join("rejected.glb");
    assert_eq!(run(&output, &rejected).1["status"], "NOT_RUN");
    assert!(!rejected.exists());
}

#[test]
fn motion_idle_checks_chain_structure_before_source_admission() {
    let dir = tempfile::tempdir().unwrap();
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rigged-simple/RiggedSimple.glb");
    let output = dir.path().join("clips.glb");
    let run = |input: &Path| {
        invoke(&[
            OsStr::new("motion-strike"),
            input.as_os_str(),
            OsStr::new("--with-idle"),
            OsStr::new("--out"),
            output.as_os_str(),
        ])
    };
    let (p, r) = run(&source);
    assert!(p.status.success(), "{r}");
    assert_eq!(r["results"]["constraint"], "fixed-root distal sway");
    assert_eq!(r["results"]["idle_clip"], "AssetWeft_Idle");
    fs::remove_file(&output).unwrap();

    let bytes = fs::read(&source).unwrap();
    for (old, new, reason) in [
        ("[3,4]", "[4,3]", "topology matched"),
        (
            "\"skeleton\":3,\"joints\":[3,4]",
            "\"skeleton\":1,\"joints\":[1,4]",
            "direct parent",
        ),
    ] {
        let mut changed = bytes.clone();
        let at = changed
            .windows(old.len())
            .position(|s| s == old.as_bytes())
            .unwrap();
        changed[at..at + old.len()].copy_from_slice(new.as_bytes());
        let other = dir.path().join("other.glb");
        fs::write(&other, changed).unwrap();
        let (p, r) = run(&other);
        assert_eq!(p.status.code(), Some(1));
        assert_eq!(r["status"], "NOT_RUN");
        assert!(
            r["errors"][0]["message"].as_str().unwrap().contains(reason),
            "{r}"
        );
        assert!(!output.exists());
    }
}

#[test]
fn motion_stance_uses_declared_role_and_sampled_world_contact() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("motion.json");
    let run = |samples: &Value, role: &str, code, status| {
        let bytes = serde_json::to_vec(samples).unwrap();
        fs::write(&path, &bytes).unwrap();
        let (process, reply) = invoke(&[
            OsStr::new("motion-stance"),
            path.as_os_str(),
            OsStr::new("--role"),
            OsStr::new(role),
        ]);
        assert_eq!(process.status.code(), Some(code), "{reply}");
        assert_eq!(reply["status"], status, "{reply}");
        assert_eq!(fs::read(&path).unwrap(), bytes);
        reply["results"].clone()
    };

    let animation = |planted| {
        let root = [0.5, 0.0, 0.125, 0.0, 0.5];
        let foot: Vec<_> = root
            .iter()
            .enumerate()
            .map(|(frame, x)| {
                let local_x = if planted && (1..=3).contains(&frame) {
                    -x
                } else {
                    0.0
                };
                [x + local_x, 0.0, 0.0]
            })
            .collect();
        json!({"stance":{"contact":"left_sole","frames":[1,3]},
            "tracks":[{"marker":"hips","world_y_up_m":root.map(|x| [x,1.0,0.0])},
                      {"marker":"left_sole","world_y_up_m":foot}]})
    };
    let slip = animation(false);
    let grounded = run(&slip, "grounded-humanoid", 1, "FAIL");
    assert_eq!(grounded["max_slip_m"], 0.125);
    assert_eq!(grounded["samples_evaluated"], 3);
    let hover = run(&slip, "hovering", 5, "INAPPLICABLE");
    assert_eq!(hover["input_sha256"], grounded["input_sha256"]);
    assert!(
        hover["input_sha256"]
            .as_str()
            .is_some_and(|s| s.len() == 64)
    );
    assert_eq!(hover["samples_evaluated"], 0);
    assert!(hover["reason"].as_str().unwrap().contains("hover"));
    let planted = animation(true);
    assert_eq!(
        run(&planted, "grounded-humanoid", 0, "PASS")["max_slip_m"],
        0.0
    );

    let mut z_axis = planted.clone();
    z_axis["tracks"][1]["world_y_up_m"][2] = json!([0, 1, 0.01]);
    run(&z_axis, "grounded-humanoid", 0, "PASS");
    z_axis["tracks"][1]["world_y_up_m"][2] = json!([0, 1, 0.011]);
    run(&z_axis, "grounded-humanoid", 1, "FAIL");

    let mut undeclared = planted.clone();
    undeclared.as_object_mut().unwrap().remove("stance");
    run(&undeclared, "grounded-humanoid", 3, "NOT_RUN");
    run(&undeclared, "hovering", 5, "INAPPLICABLE");
    let mut ambiguous = planted.clone();
    ambiguous["tracks"][0]["marker"] = "left_sole".into();
    run(&ambiguous, "grounded-humanoid", 3, "NOT_RUN");
    let mut wrong_space = planted.clone();
    wrong_space["tracks"][1]
        .as_object_mut()
        .unwrap()
        .remove("world_y_up_m");
    wrong_space["tracks"][1]["local_m"] = json!([[0, 0, 0]]);
    run(&wrong_space, "grounded-humanoid", 2, "INVALID_INPUT");

    let mut missing = planted.clone();
    missing["stance"]["contact"] = "missing_foot".into();
    run(&missing, "grounded-humanoid", 3, "NOT_RUN");
    let mut gap = planted.clone();
    gap["tracks"][1]["world_y_up_m"][2] = Value::Null;
    run(&gap, "grounded-humanoid", 3, "NOT_RUN");
    let mut out_of_range = planted;
    out_of_range["stance"]["frames"] = json!([0, 8]);
    run(&out_of_range, "grounded-humanoid", 3, "NOT_RUN");
}

#[path = "candidate/rigged_simple.rs"]
mod rigged_simple;

#[test]
fn catalog_list_reports_bounded_versions() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("list.sqlite");
    let list = |limit: Option<&str>| {
        let mut args = vec![
            OsStr::new("catalog"),
            OsStr::new("list"),
            OsStr::new("--db"),
            db.as_os_str(),
        ];
        if let Some(limit) = limit {
            args.extend([OsStr::new("--limit"), OsStr::new(limit)]);
        }
        invoke(&args)
    };
    let mut app = Application::open(&db).unwrap();
    let (out, wire) = list(None);
    assert_eq!(out.status.code(), Some(3), "{wire}");
    assert_eq!(wire["status"], "INPUT_INCOMPLETE");
    app.initialize_library().unwrap();
    let (out, wire) = list(None);
    assert!(out.status.success(), "{wire}");
    assert_eq!(wire["results"], json!({"matches":[],"truncated":false}));
    for id in ["z", "a"] {
        app.add_asset_version(&catalog_version(
            id,
            assetweft_core::library::Presence::Unknown,
        ))
        .unwrap();
    }
    let (out, wire) = list(None);
    assert!(out.status.success(), "{wire}");
    assert_eq!(wire["results"]["matches"][0]["version_id"], "a");
    assert_eq!(wire["results"]["matches"][1]["version_id"], "z");
    let (out, wire) = list(Some("1"));
    assert!(out.status.success(), "{wire}");
    assert_eq!(wire["results"]["matches"].as_array().unwrap().len(), 1);
    assert_eq!(wire["results"]["truncated"], true);
    for limit in ["0", "101", "99999", "invalid"] {
        let (out, wire) = list(Some(limit));
        assert_eq!(out.status.code(), Some(3), "{wire}");
        assert_eq!(wire["status"], "INPUT_INCOMPLETE");
    }
}

#[test]
fn action_presentation_inspect_wire_contract() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("action.json");
    let args = [
        "--call-id".as_ref(),
        "action-1".as_ref(),
        "action-presentation-inspect".as_ref(),
        path.as_os_str(),
    ];
    let run = |args: &[&OsStr], code, status| {
        let (out, response) = invoke(args);
        assert_eq!(response["command"], "action-presentation-inspect");
        assert_eq!(response["call_id"], "action-1");
        assert_eq!(response["status"], status, "{response}");
        assert_eq!(out.status.code(), Some(code));
        (out, response)
    };
    let spec = json!({
        "clip_duration_ms": 300,
        "phases": {"charge_ms": [0, 100], "release_ms": [100, 200], "recovery_ms": [200, 300]},
        "bindings": [
            {"id": "glow", "kind": "vfx", "phase": "charge", "trigger_ms": 0, "duration_ms": 100,
             "cancel_cleanup_policy": "stop_and_clear", "event_source": "fixed_timeline", "claims_gameplay_hit": false},
            {"id": "impact", "kind": "sfx", "phase": "release", "trigger_ms": 150, "duration_ms": 30,
             "cancel_cleanup_policy": "stop_and_clear", "event_source": "authoritative_hit", "claims_gameplay_hit": true},
            {"id": "tail", "kind": "sfx", "phase": "recovery", "trigger_ms": 200, "duration_ms": 100,
             "cancel_cleanup_policy": "fade_out", "event_source": "fixed_timeline", "claims_gameplay_hit": false}
        ]
    });
    run(&args, 2, "INVALID_INPUT");
    fs::write(&path, spec.to_string()).unwrap();
    let (out, pass) = run(&args, 0, "PASS");
    assert_eq!(out.stdout, run(&args, 0, "PASS").0.stdout);
    assert_eq!(pass["results"]["engine_playback_status"], "NOT_RUN");
    assert_eq!(
        pass["results"]["presentation_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    for (pointer, value, reason) in [
        ("/phases/release_ms/0", json!(99), "InvalidPhaseBounds"),
        ("/phases/release_ms/0", json!(101), "InvalidPhaseBounds"),
        ("/bindings/0/trigger_ms", json!(100), "TriggerOutsidePhase"),
        (
            "/bindings/0/cancel_cleanup_policy",
            json!("persist_leak"),
            "UncleanCancelCleanup",
        ),
        (
            "/bindings/1/event_source",
            json!("fixed_timeline"),
            "HitEventConflation",
        ),
    ] {
        let mut invalid = spec.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        fs::write(&path, invalid.to_string()).unwrap();
        let (_, fail) = run(&args, 1, "FAIL");
        assert_eq!(fail["errors"][0]["code"], reason);
    }
    fs::write(&path, b"{").unwrap();
    run(&args, 2, "INVALID_INPUT");
    fs::write(&path, spec.to_string()).unwrap();
    for limit in ["8", "0", "bad"] {
        run(
            &[&args[..], &["--max-bytes".as_ref(), limit.as_ref()]].concat(),
            2,
            "INVALID_INPUT",
        );
    }
}

#[test]
fn vfx_inspect_wire_contract() {
    let dir = tempfile::tempdir().unwrap();
    let spec_path = dir.path().join("vfx.json");
    let flipbook_path = dir.path().join("atlas.png");
    let mut spec = json!({
        "vfx_mode": "parametric",
        "authoring_state": "native_editable",
        "requires_native_editable": true,
        "timing": {"fps": 20, "frame_count": 2, "duration_ms": 100, "loop_cycle": true, "blend_mode": "alpha"},
        "parameters": [{"name": "intensity", "value": 0.5, "min": 0, "max": 1}]
    });

    let args = [
        OsStr::new("--call-id"),
        OsStr::new("vfx-1"),
        OsStr::new("vfx-inspect"),
        spec_path.as_os_str(),
        OsStr::new("--flipbook"),
        flipbook_path.as_os_str(),
    ];
    let run = |args: &[&OsStr], code, status| {
        let (out, response) = invoke(args);
        assert_eq!(response["command"], "vfx-inspect");
        assert_eq!(response["call_id"], "vfx-1");
        assert_eq!(response["status"], status, "{response}");
        assert_eq!(out.status.code(), Some(code));
        (out, response["results"].clone())
    };

    fs::write(&spec_path, spec.to_string()).unwrap();
    let (out, parametric) = run(&args[..4], 0, "PASS");
    assert_eq!(out.stdout, invoke(&args[..4]).0.stdout);
    assert_eq!(parametric["authoring_state"], "native_editable");
    assert_eq!(parametric["visual_engine_status"], "NOT_RUN");
    assert!(parametric["flipbook_sha256"].is_null());

    for (pointer, value) in [
        ("/authoring_state", json!("baked")),
        ("/timing/duration_ms", json!(102)),
        ("/parameters/0/value", json!(2)),
    ] {
        let mut invalid = spec.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        fs::write(&spec_path, invalid.to_string()).unwrap();
        let (_, report) = run(&args[..4], 1, "FAIL");
        assert_eq!(report["authoring_state"], invalid["authoring_state"]);
    }

    spec["vfx_mode"] = json!("flipbook");
    spec["flipbook"] = json!({
        "frame_width": 1, "frame_height": 1, "columns": 2, "rows": 1,
        "require_transparency": true
    });
    fs::write(&spec_path, spec.to_string()).unwrap();
    for (alpha, code, status) in [(0, 0, "PASS"), (255, 1, "FAIL")] {
        let mut encoder = png::Encoder::new(fs::File::create(&flipbook_path).unwrap(), 2, 1);
        encoder.set_color(png::ColorType::Rgba);
        let mut png = encoder.write_header().unwrap();
        png.write_image_data(&[1, 2, 3, alpha, 1, 2, 3, 255])
            .unwrap();
        png.finish().unwrap();
        let (_, report) = run(&args, code, status);
        assert_eq!(report["visual_engine_status"], "NOT_RUN");
        assert_eq!(report["flipbook_sha256"].as_str().unwrap().len(), 64);
        assert_eq!(
            report["flipbook_facts"]["transparent_pixel_count"],
            u32::from(alpha < 255)
        );
    }
    fs::write(&flipbook_path, b"broken png").unwrap();
    run(&args, 2, "INVALID_INPUT");
    fs::write(&spec_path, spec.to_string().replace("0.5", "NaN")).unwrap();
    run(&args[..4], 2, "INVALID_INPUT");
    fs::write(&spec_path, spec.to_string()).unwrap();
    for limit in ["1", "0", "16777217"] {
        let limited = [&args[..4], &[OsStr::new("--max-bytes"), OsStr::new(limit)]].concat();
        run(&limited, 2, "INVALID_INPUT");
    }
}

#[test]
fn sprite_inspect_reports_contract_and_input_failures() {
    let dir = tempfile::tempdir().unwrap();
    let sheet = dir.path().join("sprite sheet.png");
    let spec = dir.path().join("manifest.json");
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&[0, 0, 0, 0, 1, 2, 3, 255, 0, 0, 0, 0, 7, 8, 9, 128])
        .unwrap();
    writer.finish().unwrap();
    fs::write(&sheet, &png).unwrap();
    let manifest = json!({
        "sheet": {"frame_width": 1, "frame_height": 1, "columns": 2, "rows": 2},
        "frames": (0..4).map(|index| json!({
            "index": index, "col": index % 2, "row": index / 2,
            "anchor_x": -0.0, "anchor_y": 0.5,
        })).collect::<Vec<_>>(),
        "looping": {"loop_cycle": true, "start_frame": 0, "end_frame": 3}
    });
    fs::write(&spec, manifest.to_string()).unwrap();
    let args = [
        OsStr::new("--call-id"),
        OsStr::new("sprite-1"),
        OsStr::new("sprite-inspect"),
        sheet.as_os_str(),
        OsStr::new("--spec"),
        spec.as_os_str(),
    ];
    let (out, pass) = invoke(&args);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, invoke(&args).0.stdout);
    assert_eq!(pass["status"], "PASS");
    assert_eq!(pass["command"], "sprite-inspect");
    assert_eq!(pass["call_id"], "sprite-1");
    let result = &pass["results"];
    assert_eq!(result["scope"], "inspected_sprite_sheet");
    assert_eq!(result["frame_count"], 4);
    assert_eq!(result["has_transparency"], true);
    assert_eq!(result["empty_frames"], json!([0, 2]));
    assert_eq!(result["duplicate_frame_groups"], json!([[0, 2]]));
    assert_eq!(result["looping"], manifest["looping"]);
    assert_eq!(result["semantic_quality_status"], "NOT_RUN");
    assert_eq!(result["frames"][0]["anchor"].to_string(), "[0.0,0.5]");

    for (pointer, value) in [
        ("/frames/1/index", json!(2)),
        ("/frames/0/anchor_y", json!(1.2)),
        ("/sheet/columns", json!(3)),
        ("/looping/end_frame", json!(4)),
    ] {
        let mut bad = manifest.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        fs::write(&spec, bad.to_string()).unwrap();
        let (out, response) = invoke(&args);
        assert_eq!(out.status.code(), Some(1), "{pointer}");
        assert_eq!(response["status"], "FAIL");
    }

    fs::write(&spec, manifest.to_string()).unwrap();
    for (limit, field) in [
        ("1".to_owned(), Some("file")),
        (png.len().to_string(), Some("spec")),
        ("0".to_owned(), None),
        ("16777217".to_owned(), None),
    ] {
        let mut bounded = args.to_vec();
        bounded.extend([OsStr::new("--max-bytes"), OsStr::new(&limit)]);
        let (out, response) = invoke(&bounded);
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(response["status"], "INVALID_INPUT");
        if let Some(field) = field {
            assert_eq!(response["errors"][0]["field_path"], field);
        }
    }
    fs::write(&spec, "{").unwrap();
    let (out, response) = invoke(&args);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(response["status"], "INVALID_INPUT");
    fs::write(&spec, manifest.to_string()).unwrap();
    fs::write(&sheet, &png[..png.len() - 1]).unwrap();
    let (out, response) = invoke(&args);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(response["status"], "INVALID_INPUT");
}

#[test]
fn card_template_check_reports_independent_art_and_failures() {
    let dir = tempfile::tempdir().unwrap();
    let template = dir.path().join("template.json");
    let art = dir.path().join("illustration.png");
    let make_art = |pixel| {
        let f = fs::File::create(&art).unwrap();
        let mut writer = png::Encoder::new(f, 2, 1).write_header().unwrap();
        writer.write_image_data(&[pixel, pixel]).unwrap();
        writer.finish().unwrap();
    };
    let args = [
        OsStr::new("--call-id"),
        OsStr::new("card-1"),
        OsStr::new("card-template-check"),
        template.as_os_str(),
        OsStr::new("--illustration"),
        art.as_os_str(),
    ];
    let check = |args: &[&OsStr], code, status| {
        let (out, receipt) = invoke(args);
        assert_eq!(out.status.code(), Some(code), "{receipt}");
        assert_eq!(receipt["status"], status);
        assert_eq!(receipt["command"], "card-template-check");
        assert_eq!(receipt["call_id"], "card-1");
        receipt["results"].clone()
    };
    let spec = json!({
        "kind": "card",
        "canvas": {"width_px": 60, "height_px": 80},
        "illustration_slot": {"slot_id": "art", "x": 0, "y": 0, "width": 60,
            "height": 40, "editable_independently": true, "fit_mode": "contain"},
        "text_boxes": [{"box_id": "title", "x": 0, "y": 40, "width": 60,
            "height": 20, "text": "oak", "font_px": 8, "line_height_px": 10,
            "max_chars_per_line": 4, "max_lines": 2}]
    });
    fs::write(&template, spec.to_string()).unwrap();
    make_art(42);
    let a = check(&args, 0, "PASS");
    assert_eq!(invoke(&args).0.stdout, invoke(&args).0.stdout);
    make_art(99);
    let b = check(&args, 0, "PASS");
    assert_eq!(a["template_layout_sha256"], b["template_layout_sha256"]);
    assert_ne!(a["illustration_sha256"], b["illustration_sha256"]);
    assert_eq!(b["illustration_width"], 2);
    assert_eq!(b["illustration_height"], 1);
    assert_eq!(b["visual_quality_status"], "NOT_RUN");
    let no_art = check(&args[..4], 0, "PASS");
    assert_eq!(
        no_art["template_layout_sha256"],
        a["template_layout_sha256"]
    );
    assert!(no_art["illustration_sha256"].is_null());

    for kind in ["ui", "tile"] {
        let mut valid = spec.clone();
        valid["kind"] = json!(kind);
        fs::write(&template, valid.to_string()).unwrap();
        check(&args, 0, "PASS");
    }
    for (pointer, value) in [
        ("/text_boxes/0/text", json!("a\nb\nc")),
        ("/text_boxes/0/text", json!("unbreakable")),
        ("/text_boxes/0/x", json!(1)),
        ("/illustration_slot/editable_independently", json!(false)),
        (
            "/text_boxes",
            json!([spec["text_boxes"][0], {
                "box_id": "second", "x": 0, "y": 45, "width": 60,
                "height": 20, "text": "ash", "font_px": 8, "line_height_px": 10,
                "max_chars_per_line": 4, "max_lines": 2
            }]),
        ),
    ] {
        let mut invalid = spec.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        fs::write(&template, invalid.to_string()).unwrap();
        check(&args, 1, "FAIL");
    }
    fs::write(&template, b"{").unwrap();
    check(&args, 2, "INVALID_INPUT");
    fs::write(&template, spec.to_string()).unwrap();
    fs::write(&art, b"not a PNG").unwrap();
    check(&args, 2, "INVALID_INPUT");
    fs::remove_file(&art).unwrap();
    check(&args, 2, "INVALID_INPUT");
    for limit in ["1", "0"] {
        let mut limited = args.to_vec();
        limited.extend([OsStr::new("--max-bytes"), OsStr::new(limit)]);
        check(&limited, 2, "INVALID_INPUT");
    }
}

fn retarget_fixture(height: f64) -> Value {
    let rig = |height: f64, upper: f64, lower: f64| {
        json!({
            "height_m":height, "limbs":[{
                "name":"foot", "upper_len_m":upper, "lower_len_m":lower, "max_extension_ratio":0.9
            }]
        })
    };
    let frame = |x: f64, contact: Option<[f64; 3]>| {
        json!({
            "root_pos":[0, 0, 0], "targets":[{
                "limb":"foot", "hip_pos":[x, 0.5, 0], "end_effector_pos":[x, 0, 0],
                "foot_contact_pos":contact
            }]
        })
    };
    json!({
        "source_rig":rig(1.75, 0.5, 0.4), "target_rig":rig(height, 0.4, 0.35),
        "contact":{"foot_marker":"foot","stance_frames":[0, 1],"max_slip_m":0.01,"loop_tolerance_m":0.02},
        "frames":[frame(0.0, Some([0.0, 0.0, 0.0])), frame(0.005, Some([0.005, 0.0, 0.0])), frame(0.0, None)]
    })
}

#[test]
fn motion_interaction_cli_reports_verdicts_and_invalid_input() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("interaction.json");
    let args = [
        "--call-id".as_ref(),
        "shared-clock".as_ref(),
        "motion-interaction".as_ref(),
        path.as_os_str(),
    ];
    let verify = |args: &[&OsStr], code, status| {
        let (out, value) = invoke(args);
        assert_eq!(out.status.code(), Some(code), "{value}");
        assert_eq!(value["status"], status);
        assert_eq!(value["command"], "motion-interaction");
        assert_eq!(value["call_id"], "shared-clock");
        (out, value)
    };
    verify(&args, 2, "INVALID_INPUT");
    let spec = interaction_fixture();
    fs::write(&path, spec.to_string()).unwrap();
    let (out, value) = verify(&args, 0, "PASS");
    assert_eq!(value["results"]["visual_engine_status"], "NOT_RUN");
    assert_eq!(
        value["results"]["interaction_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(out.stdout, invoke(&args).0.stdout);
    for limit in ["16", "0", "bogus"] {
        let bounded = [&args[..], &["--max-bytes".as_ref(), limit.as_ref()]].concat();
        verify(&bounded, 2, "INVALID_INPUT");
    }
    for (reason, pointer, replacement) in [
        ("ClockDesync", "/actors/1/timestamps_ms/7", json!(281)),
        (
            "ContactMiss",
            "/actors/1/anchor_positions_m/4/0",
            json!(0.04),
        ),
        (
            "Interpenetration",
            "/actors/1/anchor_positions_m/2/0",
            json!(0.0),
        ),
        (
            "InsufficientSeparation",
            "/actors/1/anchor_positions_m/5/0",
            json!(0.1),
        ),
        (
            "InsufficientSeparation",
            "/actors/1/root_positions_m/7/0",
            json!(0.1),
        ),
    ] {
        let mut changed = spec.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        fs::write(&path, changed.to_string()).unwrap();
        let (_, value) = verify(&args, 1, "FAIL");
        assert_eq!(value["results"]["failure"]["reason"], reason);
    }
    let mut ai = spec.clone();
    ai["interaction"]["ai_planner_present"] = json!(true);
    for contents in [
        "{".into(),
        spec.to_string().replace("0.03", "NaN"),
        ai.to_string(),
    ] {
        fs::write(&path, contents).unwrap();
        verify(&args, 2, "INVALID_INPUT");
    }
}

fn interaction_fixture() -> Value {
    let actor = |id, root_x, anchor_x| {
        let anchors: Vec<_> = (0..8)
            .map(|frame| {
                [
                    if (2..=4).contains(&frame) {
                        anchor_x
                    } else {
                        root_x
                    },
                    0.0,
                    0.0,
                ]
            })
            .collect();
        json!({
            "actor_id": id,
            "timestamps_ms": [0, 40, 80, 120, 160, 200, 240, 280],
            "root_positions_m": vec![[root_x, 0.0, 0.0]; 8],
            "anchor_positions_m": anchors,
        })
    };
    json!({
        "scope": "shared_clock_kinematic",
        "actors": [actor("a", 0.0, 0.0), actor("b", 1.0, 0.02)],
        "interaction": {
            "actor_a": "a",
            "actor_b": "b",
            "contact_window_frames": [2, 4],
            "separation_window_frames": [5, 7],
            "max_contact_distance_m": 0.03,
            "min_separation_distance_m": 0.5,
            "forbidden_interpenetration_m": 0.01,
        },
    })
}

#[test]
fn motion_retarget_cli_reports_each_stage_and_invalid_input() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("retarget.json");
    let args = [
        "--call-id".as_ref(),
        "retarget-test".as_ref(),
        "motion-retarget".as_ref(),
        path.as_os_str(),
    ];
    let stages = ["proportion", "ik_reach", "contact_slip", "loop_closure"];
    let run = |spec: &Value, failed: Option<usize>| {
        fs::write(&path, serde_json::to_vec(spec).unwrap()).unwrap();
        let (out, value) = invoke(&args);
        assert_eq!(
            out.status.code(),
            Some(i32::from(failed.is_some())),
            "{value}"
        );
        assert_eq!(out.stdout, invoke(&args).0.stdout);
        assert_eq!(value["command"], "motion-retarget", "{args:?}: {value}");
        assert_eq!(value["call_id"], "retarget-test");
        assert_eq!(
            value["status"],
            if failed.is_some() { "FAIL" } else { "PASS" }
        );
        let report = &value["results"];
        assert_eq!(report["visual_engine_status"], "NOT_RUN");
        assert_eq!(
            report["failed_stage"],
            json!(failed.map(|index| stages[index]))
        );
        assert_eq!(report["stages"].as_array().unwrap().len(), stages.len());
        for (index, stage) in stages.iter().enumerate() {
            assert_eq!(&report["stages"][index]["stage"], stage);
            let expected = match failed {
                Some(failed) if index == failed => "FAIL",
                Some(failed) if index > failed => "NOT_RUN",
                _ => "PASS",
            };
            assert_eq!(report["stages"][index]["status"], expected);
        }
    };
    for height in [1.4, 2.1] {
        run(&retarget_fixture(height), None);
    }
    for (index, pointer, replacement) in [
        (0, "/target_rig/height_m", json!(0.175)),
        (
            1,
            "/frames/1/targets/0/end_effector_pos",
            json!([0, -0.3, 0]),
        ),
        (
            2,
            "/frames/1/targets/0/foot_contact_pos",
            json!([0.02, 0, 0]),
        ),
        (
            3,
            "/frames/2/targets/0/end_effector_pos",
            json!([0.03, 0, 0]),
        ),
    ] {
        let mut spec = retarget_fixture(1.4);
        *spec.pointer_mut(pointer).unwrap() = replacement;
        run(&spec, Some(index));
    }

    let invalid = |args: &[&OsStr]| {
        let (out, value) = invoke(args);
        assert_eq!(out.status.code(), Some(2), "{value}");
        assert_eq!(value["status"], "INVALID_INPUT");
        assert_eq!(value["command"], "motion-retarget", "{args:?}: {value}");
    };
    fs::remove_file(&path).unwrap();
    invalid(&args);
    let valid = retarget_fixture(1.4).to_string();
    let nan = valid.replace("\"root_pos\":[0,0,0]", "\"root_pos\":[NaN,0,0]");
    assert_ne!(nan, valid);
    for contents in ["{", nan.as_str()] {
        fs::write(&path, contents).unwrap();
        invalid(&args);
    }
    fs::write(&path, valid).unwrap();
    for limit in ["0", "bogus", "16"] {
        invalid(&[&args[..], &["--max-bytes".as_ref(), limit.as_ref()]].concat());
        invalid(&[&["--max-bytes".as_ref(), limit.as_ref()][..], &args].concat());
    }
}

#[test]
fn cloth_inspect_wire_statuses_and_replay() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("clothing.glb");
    let spec_path = dir.path().join("cloth.json");
    let rig = include_bytes!("../../../examples/rigged-simple/RiggedSimple.glb");
    let json_end = 20 + u32::from_le_bytes(rig[12..16].try_into().unwrap()) as usize;
    let mut document: Value = serde_json::from_slice(&rig[20..json_end]).unwrap();
    let position = document["meshes"][0]["primitives"][0]["attributes"]["POSITION"].clone();
    document["nodes"][2]["name"] = json!("tunic");
    let nodes = document["nodes"].as_array_mut().unwrap();
    let body_index = nodes.len();
    nodes.extend([
        json!({"name":"body","mesh":1}),
        json!({"name":"buckle","mesh":1}),
    ]);
    document["scenes"][0]["nodes"]
        .as_array_mut()
        .unwrap()
        .extend([json!(body_index), json!(body_index + 1)]);
    document["meshes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"primitives":[{"attributes":{"POSITION":position}}]}));
    let mut serialized = serde_json::to_vec(&document).unwrap();
    serialized.resize(serialized.len().next_multiple_of(4), b' ');
    let mut bytes = rig[..8].to_vec();
    bytes.extend(((20 + serialized.len() + rig.len() - json_end) as u32).to_le_bytes());
    bytes.extend((serialized.len() as u32).to_le_bytes());
    bytes.extend(b"JSON");
    bytes.extend(serialized);
    bytes.extend(&rig[json_end..]);
    fs::write(&file, bytes).unwrap();

    let garment = |node, binding| {
        json!({
            "node_name":node,"binding_kind":binding,"body_mesh_node":"body",
            "max_weight_sum_error":0.0001,"max_stretch_ratio":0.1,
            "max_penetration_m":0.05,"require_supported_structure":true
        })
    };
    let pose = |frame| {
        json!({
            "frame":frame,"body_vertices":[[0,0,0],[1,1,1]],"garment_vertices":[[0,0,1.5],[1,0,1.5]],
            "rest_garment_edges":[[0,1]],"reference_edge_lengths_m":[1]
        })
    };
    let mut spec = json!({"garments":[garment("tunic","skinned"),garment("buckle","rigid")],
        "sampled_motion_poses":[pose(1),pose(2)]});
    let args = [
        OsStr::new("--call-id"),
        OsStr::new("cloth-1"),
        OsStr::new("cloth-inspect"),
        file.as_os_str(),
        OsStr::new("--spec"),
        spec_path.as_os_str(),
    ];
    let run = |spec: &Value| {
        fs::write(&spec_path, spec.to_string()).unwrap();
        invoke(&args)
    };
    let (output, pass) = run(&spec);
    assert_eq!(output.status.code(), Some(0), "{pass}");
    assert_eq!(pass["status"], "PASS");
    assert_eq!(pass["command"], "cloth-inspect");
    assert_eq!(pass["call_id"], "cloth-1");
    assert_eq!(
        pass["results"]["garments"][0]["garment_classification"],
        "skinned"
    );
    assert_eq!(
        pass["results"]["garments"][1]["garment_classification"],
        "rigid"
    );
    assert_eq!(pass["results"]["visual_engine_status"], "NOT_RUN");
    assert_eq!(output.stdout, invoke(&args).0.stdout);

    spec["garments"][1]["binding_kind"] = json!("skinned");
    spec["garments"][0]["body_mesh_node"] = json!("missing");
    spec["sampled_motion_poses"][1]["garment_vertices"] = json!([[0.5, 0.5, 0.5], [2.5, 0.5, 0.5]]);
    let (output, fail) = run(&spec);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fail["status"], "FAIL");
    assert_eq!(fail["results"]["garments"][0]["status"], "UNSUPPORTED");
    assert_eq!(
        fail["results"]["unsupported_structures"],
        json!(["MissingOrAmbiguousBodyMesh"])
    );
    for violation in [
        "BindingMismatch",
        "UnsupportedStructure",
        "ClothPenetration",
        "ExcessiveDeformation",
    ] {
        assert!(
            fail["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|error| error["message"] == violation),
            "{fail}"
        );
    }

    for limit in ["0", "1"] {
        let (output, invalid) = invoke(
            &[
                args.as_slice(),
                &[OsStr::new("--max-bytes"), OsStr::new(limit)],
            ]
            .concat(),
        );
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(invalid["command"], "cloth-inspect");
    }
    fs::write(&spec_path, spec.to_string().replace("0.05", "NaN")).unwrap();
    let (output, invalid) = invoke(&args);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(invalid["errors"][0]["field_path"], "spec");
    fs::write(&file, b"glTF").unwrap();
    let (output, invalid) = run(&spec);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(invalid["status"], "INVALID_INPUT");
    assert_eq!(invalid["errors"][0]["field_path"], "file");

    // A small valid GLB lets this limit clear the file read and reject only the spec.
    fs::copy(fixture("mesh_only.glb"), &file).unwrap();
    let limit = fs::metadata(&file).unwrap().len().to_string();
    let (output, invalid) = invoke(
        &[
            args.as_slice(),
            &[OsStr::new("--max-bytes"), OsStr::new(&limit)],
        ]
        .concat(),
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(invalid["errors"][0]["field_path"], "spec");
}
