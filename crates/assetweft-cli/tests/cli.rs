//! Responsibility: C2 actual-binary protocol and delegation regression gate.
//! Dependencies: core fixtures/APIs, std process, tempfile; called by C1 and native CI.
//! Invariants: synthetic evidence only; no mocks; full stdout parse, repeat stability,
//! bounded reads, Chinese/space paths and unchanged input bytes.
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

// A closed Unix socket peer gives the real child EPIPE on writes, including on macOS.
// No synthetic writer or production fault switch is involved.
#[cfg(unix)]
fn broken_output() -> std::process::Stdio {
    use std::os::{fd::OwnedFd, unix::net::UnixStream};
    let (writer, reader) = UnixStream::pair().unwrap();
    drop(reader);
    std::process::Stdio::from(OwnedFd::from(writer))
}

// An owned read-only file handle gives the real Windows child a write failure.
// File's default delete sharing keeps the handle valid after the temp path is removed.
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
// Seeds durable state only through the existing product API: initialize, create, then a
// real succeeded record and a real claimed/running record left unresolved.
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
    // A fresh process opening the same durable store sees identical values.
    let app = Application::open(&db).unwrap();
    assert_eq!(
        app.workflow_steps("运行-α").unwrap()[0].state,
        State::Succeeded
    );
    assert!(!app.workflow_cancelled("运行-α").unwrap());
    drop(app);
    // Existing product use cases advance state; status reports the new persisted facts.
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
    // Repeating the explicit request remains a scoped success with identical output.
    let (again, v_again) = invoke(&args);
    assert_eq!(out.stdout, again.stdout);
    assert_eq!(v_again["results"]["cancellation_requested"], true);
    // Reopen proves the durable flag and unchanged records/results.
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
    // Status of the same run reports the request, not a stopped process.
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
    // A directory (non-regular path) is rejected just as explicitly and left alone.
    let dirdb = dir.path().join("目录 db");
    fs::create_dir(&dirdb).unwrap();
    let cancel = workflow_args("cancel", dirdb.as_os_str(), run);
    let (out, v) = invoke(&cancel);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(v["errors"][0]["field_path"], "db");
    assert!(dirdb.is_dir());
    // Unknown run fails truthfully with the preserved core diagnostic.
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
    // Empty identity is rejected verbatim, never trimmed or substituted.
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
    // Missing required flags stay an argument error.
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
    // An existing but uninitialized file is reported, never auto-initialized or migrated.
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
    // Execution/submission and unqualified create syntax still fail truthfully.
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
    // A closed stdout must map to the existing INTERNAL exit 6 on success and error paths.
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
    // Dependency order is retained, not topologically sorted or identities trimmed.
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
    // Shape failures belong to the CLI; graph policy retains actual core Database => FAIL.
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
    // Core's existing 64-step bound, not a second CLI policy.
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
    // Duplicate creation errors survive broken stderr without becoming success.
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
    // Output failure does not roll back the committed operation or trigger a replay.
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
