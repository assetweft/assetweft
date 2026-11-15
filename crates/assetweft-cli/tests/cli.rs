//! Responsibility: C2 actual-binary protocol and delegation regression gate.
//! Dependencies: core fixtures/APIs, std process, tempfile; called by C1 and native CI.
//! Invariants: synthetic evidence only; no mocks; full stdout parse, repeat stability,
//! bounded reads, Chinese/space paths, unchanged input bytes and immutable local allowances.
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

// Shut down the writer itself: dropping a peer alone leaves writes possible while
// another process holds that peer. The real child must inherit an unwritable socket.
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

// Local declarations only: real CLI processes and SQLite through Application, no provider.
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
    // Seed only a local reservation through the existing facade; no submit/callback.
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
    // Deliberately different key order still yields the same typed descriptor.
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
        // Approval conflict must be valid core input to reach immutable identity check.
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
        assert_eq!(v["errors"][0]["message"], expected); // empty query ID stays verbatim; core does not validate it.
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
    // First declaration commits even when the real child cannot report it; no retry occurs.
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

// Local TEST_FIXTURE setup uses only public Application capabilities, never SQL.
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
    // Omitted and explicit-null quotes remain unknown on an ordinary allowance.
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

// Retain each real child receipt when the author run supplies an owned evidence directory.
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
// Public Application/CAS fixture only; pinned hashes name exact static synthetic bytes.
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
        // Advance the actual head once more: an Accepted receipt must keep its older Current.
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
        // Only our tempfile's synthetic CAS object is removed, after genuine persistence.
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
