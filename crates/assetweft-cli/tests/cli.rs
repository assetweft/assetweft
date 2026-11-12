//! Responsibility: C2 actual-binary protocol and delegation regression gate.
//! Dependencies: core fixtures/APIs, std process, tempfile; called by workspace C1.
//! Invariants: synthetic evidence only; no mocks; full stdout parse, repeat stability,
//! bounded reads, Chinese/space paths and unchanged input bytes.
use assetweft_core::{generation_spec as spec, validation as val};
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
