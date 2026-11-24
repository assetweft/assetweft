use assetweft_core::{
    application::Application,
    budget::{Allowance, Approval, Resolution, State, Unit},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const DUMMY: &str = "hf_dummy_secret_do_not_leak_123456";
const ARTIFACT: &[u8] = b"deterministic synthetic export";

fn contains_secret(bytes: &[u8]) -> bool {
    bytes
        .windows(DUMMY.len())
        .any(|window| window == DUMMY.as_bytes())
}

fn invoke(command: &mut Command) -> (i32, Value) {
    let Output {
        status,
        stdout,
        stderr,
    } = command.output().unwrap();
    assert!(
        !contains_secret(&stdout),
        "stdout exposed adapter credentials"
    );
    assert!(
        !contains_secret(&stderr),
        "stderr exposed adapter credentials"
    );
    assert_eq!(stdout.iter().filter(|b| **b == b'\n').count(), 1);
    let value: Value = serde_json::from_slice(&stdout).expect("single JSON envelope");
    assert_eq!(value["schema_version"], 1);
    assert!(stderr.is_empty(), "adapter output must be suppressed");
    (status.code().unwrap(), value)
}

fn cli(action: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assetweft"));
    command.args(["byo-gpu", action]);
    command
}

fn metadata() -> Value {
    json!({
        "id": "testuser/private-trellis2",
        "title": "Private fixture",
        "code_file": "src/main.py",
        "language": "python",
        "kernel_type": "script",
        "is_private": true,
        "enable_gpu": true,
        "machine_shape": "NvidiaL4",
    })
}

fn write_metadata(bundle: &Path, value: &Value) {
    fs::write(
        bundle.join("kernel-metadata.json"),
        serde_json::to_vec(value).unwrap(),
    )
    .unwrap();
}

fn bundle_in(root: &Path) -> PathBuf {
    let bundle = root.join("kaggle bundle");
    fs::create_dir_all(bundle.join("src")).unwrap();
    fs::write(bundle.join("src/main.py"), "# synthetic kernel").unwrap();
    write_metadata(&bundle, &metadata());
    bundle
}

#[test]
fn profiles_declare_exact_models_and_public_auth_metadata() {
    let (code, result) = invoke(
        cli("profiles")
            .args(["--call-id", "profile-check"])
            .env("HF_TOKEN", DUMMY),
    );
    assert_eq!(code, 0);
    assert_eq!(result["status"], "PASS");
    assert_eq!(result["command"], "byo-gpu profiles");
    assert_eq!(result["call_id"], "profile-check");
    let profiles = result["results"]["profiles"].as_array().unwrap();
    assert_eq!(profiles.len(), 2);
    for (profile, id, auth_names, expected_models) in [
        (
            &profiles[0],
            "hf-zerogpu",
            json!(["HF_TOKEN"]),
            vec![
                ("z-image-turbo", "Apache-2.0"),
                ("flux2-klein", "Apache-2.0"),
                ("trellis2", "MIT"),
                ("hi3dgen", "MIT"),
            ],
        ),
        (
            &profiles[1],
            "kaggle-cli",
            json!(["KAGGLE_API_TOKEN", "KAGGLE_USERNAME", "KAGGLE_KEY"]),
            vec![
                ("trellis2", "MIT"),
                ("hi3dgen", "MIT"),
                ("z-image-turbo", "Apache-2.0"),
            ],
        ),
    ] {
        assert_eq!(profile["id"], id);
        assert_eq!(profile["auth_env_vars"], auth_names);
        assert_eq!(profile.get("cash_cost"), Some(&Value::Null));
        assert_eq!(profile["cost_unknown"], true);
        assert!(profile["external_runtime"].is_string());
        let models: Vec<_> = profile["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| (m["id"].as_str().unwrap(), m["license"].as_str().unwrap()))
            .collect();
        assert_eq!(models, expected_models);
    }
    assert_eq!(profiles[1]["is_private"], true);
    assert_eq!(
        profiles[1]["accelerators"],
        json!(["NvidiaTeslaT4", "NvidiaL4"])
    );
    assert!(
        profiles[1]["external_runtime"]
            .as_str()
            .unwrap()
            .contains("--with six")
    );
}

#[test]
fn kaggle_validation_is_local_and_rejects_invalid_or_escaping_bundles() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = bundle_in(dir.path());
    let validate = || {
        invoke(
            cli("kaggle-validate")
                .arg("--bundle")
                .arg(&bundle)
                .env("KAGGLE_API_TOKEN", DUMMY),
        )
    };
    let (code, result) = validate();
    assert_eq!(code, 0);
    assert_eq!(
        result["results"],
        json!({
            "is_private": true, "enable_gpu": true, "credential_present": true, "accelerator": "NvidiaL4",
        })
    );
    fs::write(dir.path().join("escape.py"), "# must stay outside").unwrap();
    for (field, value, expected) in [
        ("is_private", json!(false), "bundle.is_private"),
        ("code_file", json!("missing.py"), "bundle.code_file"),
        ("code_file", json!("../escape.py"), "bundle.code_file"),
        ("id", json!("<YOUR_KAGGLE_USERNAME>/fixture"), "bundle.id"),
    ] {
        let mut m = metadata();
        m[field] = value;
        write_metadata(&bundle, &m);
        let (code, result) = validate();
        assert_eq!(code, 2);
        assert_eq!(result["status"], "INVALID_INPUT");
        assert_eq!(result["errors"][0]["field_path"], expected);
    }
    #[cfg(unix)]
    {
        let mut m = metadata();
        std::os::unix::fs::symlink(dir.path(), bundle.join("external")).unwrap();
        m["code_file"] = json!("external/escape.py");
        write_metadata(&bundle, &m);
        let (code, result) = validate();
        assert_eq!(code, 2);
        assert_eq!(result["errors"][0]["field_path"], "bundle.code_file");
    }
}

#[test]
fn kaggle_rejects_positional_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = bundle_in(dir.path());
    let validate = || invoke(cli("kaggle-validate").arg("--bundle").arg(&bundle));
    write_metadata(
        &bundle,
        &json!([
            "testuser/private-trellis2",
            null,
            "Private fixture",
            "src/main.py",
            "python",
            "script",
            true,
            true,
            false,
            "NvidiaL4",
        ]),
    );
    let (code, result) = validate();
    assert_eq!(code, 2, "{result}");
}

#[test]
fn kaggle_accepts_only_supported_template_flags() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = bundle_in(dir.path());
    let validate = || invoke(cli("kaggle-validate").arg("--bundle").arg(&bundle));
    let mut template = metadata();
    template["is_private"] = json!("true");
    template["enable_gpu"] = json!("true");
    template["enable_tpu"] = json!("false");
    write_metadata(&bundle, &template);
    let (code, result) = validate();
    assert_eq!(code, 0, "{result}");
    for (field, value, path) in [
        ("is_private", json!("false"), "bundle.is_private"),
        ("enable_gpu", json!("false"), "bundle.enable_gpu"),
        ("enable_tpu", json!("true"), "bundle.enable_gpu"),
        ("enable_gpu", json!("yes"), "bundle"),
        ("enable_gpu", json!(1), "bundle"),
        ("id_no", json!(123), "bundle.id"),
    ] {
        let mut rejected = template.clone();
        rejected[field] = value;
        write_metadata(&bundle, &rejected);
        let (code, result) = validate();
        assert_eq!(code, 2, "{result}");
        assert_eq!(result["errors"][0]["field_path"], path);
    }
}

fn isolated_kaggle_auth<'a>(command: &'a mut Command, home: &Path) -> &'a mut Command {
    command
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env_remove("KAGGLE_CONFIG_DIR")
        .env_remove("KAGGLE_API_TOKEN")
        .env_remove("KAGGLE_USERNAME")
        .env_remove("KAGGLE_KEY")
}

#[test]
fn kaggle_validation_detects_standard_token_file() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = bundle_in(dir.path());
    let token = dir.path().join(".kaggle/access_token");
    fs::create_dir(token.parent().unwrap()).unwrap();
    fs::write(&token, DUMMY).unwrap();
    let validate = || {
        invoke(isolated_kaggle_auth(
            cli("kaggle-validate").arg("--bundle").arg(&bundle),
            dir.path(),
        ))
    };
    let (code, result) = validate();
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["results"]["credential_present"], true);
    fs::remove_file(token).unwrap();
    let (code, result) = validate();
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["results"]["credential_present"], false);
}

fn adapter(dir: &Path) -> PathBuf {
    let source = dir.join("runner.rs");
    let executable = dir.join(format!("runner{}", std::env::consts::EXE_SUFFIX));
    fs::write(&source, r##"
use std::{env, fs, io::Write, path::PathBuf};

fn main() {
    writeln!(fs::OpenOptions::new().create(true).append(true)
        .open(env::var_os("ASSETWEFT_TEST_MARKER").unwrap()).unwrap(), "invoked").unwrap();
    let token = env::var("HF_TOKEN").unwrap();
    assert!(!env::args_os().any(|arg| arg.to_string_lossy().contains(&token)));
    println!("{token}");
    eprintln!("{token}");
    let mode = env::var("ASSETWEFT_TEST_MODE").unwrap();
    if mode == "failure" { std::process::exit(19); }
    if mode == "missing-output" { return; }
    let args: Vec<_> = env::args_os().collect();
    let get = |flag: &str| -> PathBuf {
        args.windows(2).find(|pair| pair[0] == flag).unwrap()[1].clone().into()
    };
    if mode == "kaggle" {
        let bundle = get("--bundle");
        assert_eq!(get("--accelerator"), PathBuf::from("NvidiaL4"));
        assert_eq!(fs::read_to_string(bundle.join("src/main.py")).unwrap(), "# synthetic kernel");
        assert!(!bundle.join("unrelated.txt").exists());
        assert!(fs::read_to_string(bundle.join("kernel-metadata.json")).unwrap().contains("\"is_private\":true"));
    } else {
        assert_eq!(fs::read(get("--image")).unwrap(), b"synthetic input");
    }
    fs::write(get("--out-dir").join("artifact.glb"), b"deterministic synthetic export").unwrap();
}
"##).unwrap();
    let compile = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg("--edition=2024")
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    executable
}

struct Fixture {
    dir: tempfile::TempDir,
    db: PathBuf,
    output: PathBuf,
    marker: PathBuf,
    image: PathBuf,
    adapter: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("library.sqlite");
        let output = dir.path().join("output dir");
        let marker = dir.path().join("invocations");
        let image = dir.path().join("test image.png");
        let adapter = adapter(dir.path());
        fs::write(&image, b"synthetic input").unwrap();
        let mut app = Application::open(&db).unwrap();
        app.initialize_library().unwrap();
        app.approve_budget(&Allowance {
            id: "approved".into(),
            unit: Unit::Task,
            capacity: 1,
            approval: Approval::ExplicitBounded,
            source: "test".into(),
        })
        .unwrap();
        Self {
            dir,
            db,
            output,
            marker,
            image,
            adapter,
        }
    }

    fn run(&self, profile: &str, mode: &str) -> Command {
        let mut command = cli("run");
        command
            .args([
                OsStr::new("--db"),
                self.db.as_os_str(),
                OsStr::new("--out-dir"),
                self.output.as_os_str(),
                OsStr::new("--adapter-bin"),
                self.adapter.as_os_str(),
            ])
            .args([
                "--budget-id",
                "approved",
                "--intent-id",
                "once",
                "--model",
                "trellis2",
                "--profile",
                profile,
            ])
            .env("ASSETWEFT_TEST_MARKER", &self.marker)
            .env("ASSETWEFT_TEST_MODE", mode)
            .env("HF_TOKEN", DUMMY);
        command
    }

    fn balance(&self, reserved: u64, spent: u64, state: Option<State>) {
        let app = Application::open_read_only(&self.db).unwrap();
        let balance = app.budget_balance("approved").unwrap();
        assert_eq!((balance.reserved, balance.spent), (reserved, spent));
        let intent = app.call_intent("once").unwrap();
        assert_eq!(intent.as_ref().map(|i| i.state), state);
        if let Some(intent) = intent {
            assert_eq!(intent.request.unit, Unit::Task);
            assert_eq!(intent.request.reservation, 1);
            assert_eq!(intent.request.quote, None);
        }
        app.close().unwrap();
        assert!(
            !contains_secret(&fs::read(&self.db).unwrap()),
            "database exposed credentials"
        );
    }

    fn one_invocation(&self) {
        assert_eq!(fs::read_to_string(&self.marker).unwrap(), "invoked\n");
    }
}

#[test]
fn rejects_upload_and_public_kaggle_before_any_claim_or_invocation() {
    let fixture = Fixture::new();
    let (code, result) = invoke(
        fixture
            .run("hf-zerogpu", "success")
            .arg("--image")
            .arg(&fixture.image),
    );
    assert_eq!(code, 2);
    assert_eq!(result["status"], "INVALID_INPUT");
    assert_eq!(result["errors"][0]["field_path"], "allow-remote-upload");
    let bundle = bundle_in(fixture.dir.path());
    let mut m = metadata();
    m["is_private"] = json!(false);
    write_metadata(&bundle, &m);
    let (code, result) = invoke(
        fixture
            .run("kaggle-cli", "kaggle")
            .arg("--bundle")
            .arg(bundle)
            .arg("--allow-remote-upload")
            .env("KAGGLE_API_TOKEN", DUMMY),
    );
    assert_eq!(code, 2);
    assert_eq!(result["errors"][0]["field_path"], "bundle.is_private");
    assert!(!fixture.marker.exists());
    assert!(!fixture.output.exists());
    fixture.balance(0, 0, None);
}

#[test]
fn run_requires_an_existing_initialized_library_and_task_allowance() {
    let fixture = Fixture::new();
    fs::remove_file(&fixture.db).unwrap();
    let run = || {
        invoke(
            fixture
                .run("hf-zerogpu", "success")
                .arg("--image")
                .arg(&fixture.image)
                .arg("--allow-remote-upload"),
        )
    };
    let (code, missing) = run();
    assert_eq!(code, 2);
    assert_eq!(missing["errors"][0]["field_path"], "db");
    assert!(!fixture.db.exists());

    fs::File::create(&fixture.db).unwrap();
    let (code, uninitialized) = run();
    assert_eq!(code, 3);
    assert_eq!(uninitialized["status"], "INPUT_INCOMPLETE");
    assert_eq!(fs::metadata(&fixture.db).unwrap().len(), 0);

    let mut app = Application::open(&fixture.db).unwrap();
    app.initialize_library().unwrap();
    let (code, missing_budget) = run();
    assert_eq!(code, 1, "{missing_budget}");
    assert_eq!(missing_budget["status"], "FAIL");
    assert_eq!(missing_budget["errors"][0]["field_path"], "budget");
    assert!(app.call_intent("once").unwrap().is_none());
    assert!(app.call_receipts("once").unwrap().is_empty());
    assert!(!fixture.marker.exists());
    assert!(!fixture.output.exists());

    app.approve_budget(&Allowance {
        id: "approved".into(),
        unit: Unit::Token,
        capacity: 1,
        approval: Approval::ExplicitBounded,
        source: "test".into(),
    })
    .unwrap();
    app.close().unwrap();
    let (code, wrong_unit) = run();
    assert_eq!(code, 2);
    assert_eq!(wrong_unit["errors"][0]["field_path"], "budget");
    assert!(!fixture.marker.exists());
    fixture.balance(0, 0, None);
}

#[test]
fn exhausted_budget_cleans_only_a_new_output_directory() {
    let fixture = Fixture::new();
    let mut app = Application::open(&fixture.db).unwrap();
    app.reserve_call(&assetweft_core::budget::Request {
        id: "existing-reservation".into(),
        account: "approved".into(),
        input_fingerprint: "0".repeat(64),
        unit: Unit::Task,
        quote: None,
        reservation: 1,
        source: "test".into(),
    })
    .unwrap();
    let before = app.budget_balance("approved").unwrap();
    app.close().unwrap();

    for preexisting in [true, false] {
        if preexisting {
            fs::create_dir(&fixture.output).unwrap();
        }
        let (code, result) = invoke(
            fixture
                .run("hf-zerogpu", "success")
                .arg("--image")
                .arg(&fixture.image)
                .arg("--allow-remote-upload"),
        );
        assert_eq!(code, 2, "{result}");
        assert_eq!(result["errors"][0]["field_path"], "budget");
        assert!(!fixture.marker.exists());
        let app = Application::open_read_only(&fixture.db).unwrap();
        assert_eq!(app.budget_balance("approved").unwrap(), before);
        assert!(app.call_intent("once").unwrap().is_none());
        assert!(app.call_receipts("once").unwrap().is_empty());
        assert_eq!(fixture.output.exists(), preexisting);
        if preexisting {
            fs::remove_dir(&fixture.output).unwrap();
        }
    }
}

#[test]
fn approved_iteration_hashes_and_settles_one_task_then_returns_the_receipt() {
    let fixture = Fixture::new();
    let run = || {
        invoke(
            fixture
                .run("hf-zerogpu", "success")
                .arg("--image")
                .arg(&fixture.image)
                .arg("--allow-remote-upload"),
        )
    };
    let (code, first) = run();
    assert_eq!(code, 0, "{first}");
    let sha: String = Sha256::digest(ARTIFACT)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        first["results"]["artifact"],
        json!({"file": "artifact.glb", "sha256": sha})
    );
    assert_eq!(first["results"]["state"], "SUCCEEDED");
    assert_eq!(first["results"]["invoked"], true);
    assert_eq!(first["results"]["unit"], "Task");
    assert_eq!(first["results"].get("cash_cost"), Some(&Value::Null));
    assert_eq!(first["results"]["cost_unknown"], true);
    assert_eq!(
        fs::read(fixture.output.join("artifact.glb")).unwrap(),
        ARTIFACT
    );
    fixture.balance(0, 1, Some(State::Confirmed));
    let (code, replay) = run();
    assert_eq!(code, 0, "{replay}");
    assert_eq!(replay["results"]["invoked"], false);
    assert_eq!(replay["results"]["artifact"], first["results"]["artifact"]);
    fs::write(&fixture.image, b"changed local input").unwrap();
    let (code, conflict) = run();
    assert_eq!(code, 2);
    assert_eq!(conflict["errors"][0]["field_path"], "budget");
    fixture.one_invocation();
    fixture.balance(0, 1, Some(State::Confirmed));
    let receipts = Application::open_read_only(&fixture.db)
        .unwrap()
        .call_receipts("once")
        .unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].resolution, Resolution::Confirmed { charge: 1 });
    assert_eq!(
        receipts[0].usage[0].source,
        format!("artifact-sha256:{sha}")
    );
}

#[cfg(unix)]
#[test]
fn local_staging_failure_does_not_reserve_or_claim_a_task() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new();
    fs::create_dir(&fixture.output).unwrap();
    fs::set_permissions(&fixture.output, fs::Permissions::from_mode(0o500)).unwrap();
    let run = || {
        invoke(
            fixture
                .run("hf-zerogpu", "success")
                .arg("--image")
                .arg(&fixture.image)
                .arg("--allow-remote-upload"),
        )
    };
    let (code, result) = run();
    fs::set_permissions(&fixture.output, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(code, 2, "{result}");
    assert_eq!(result["errors"][0]["field_path"], "out-dir");
    assert!(!fixture.marker.exists());
    fixture.balance(0, 0, None);

    let (code, result) = run();
    assert_eq!(code, 0, "{result}");
    fixture.one_invocation();
    fixture.balance(0, 1, Some(State::Confirmed));
}

#[test]
fn failed_or_missing_adapter_result_is_unknown_and_never_replayed() {
    for mode in ["failure", "missing-output"] {
        let fixture = Fixture::new();
        let run = |mode| {
            invoke(
                fixture
                    .run("hf-zerogpu", mode)
                    .arg("--image")
                    .arg(&fixture.image)
                    .arg("--allow-remote-upload"),
            )
        };
        let (code, result) = run(mode);
        assert_eq!(code, 1);
        assert_eq!(result["status"], "UNKNOWN");
        assert_eq!(result["results"]["invoked"], true);
        assert_eq!(result["results"]["artifact"], Value::Null);
        fixture.balance(1, 0, Some(State::Unknown));
        let (code, result) = run("success");
        assert_eq!(code, 1);
        assert_eq!(result["status"], "UNKNOWN");
        assert_eq!(result["results"]["invoked"], false);
        fixture.one_invocation();
        fixture.balance(1, 0, Some(State::Unknown));
    }
}

#[test]
fn kaggle_runner_gets_only_the_validated_private_kernel_and_accelerator() {
    let fixture = Fixture::new();
    let bundle = bundle_in(fixture.dir.path());
    fs::write(bundle.join("unrelated.txt"), "not a kernel input").unwrap();
    let run = || {
        let mut command = fixture.run("kaggle-cli", "kaggle");
        isolated_kaggle_auth(&mut command, fixture.dir.path())
            .arg("--bundle")
            .arg(&bundle)
            .arg("--allow-remote-upload");
        command
    };
    let (code, missing) = invoke(&mut run());
    assert_eq!(code, 3, "{missing}");
    fixture.balance(0, 0, None);
    assert!(!fixture.marker.exists());

    let token = fixture.dir.path().join(".kaggle/access_token");
    fs::create_dir(token.parent().unwrap()).unwrap();
    fs::write(&token, DUMMY).unwrap();
    let (code, result) = invoke(&mut run());
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["results"]["state"], "SUCCEEDED");
    fs::remove_file(token).unwrap();
    let (code, replay) = invoke(&mut run());
    assert_eq!(code, 0, "{replay}");
    assert_eq!(replay["results"]["invoked"], false);
    assert_eq!(replay["results"]["artifact"], result["results"]["artifact"]);

    fs::write(bundle.join("src/main.py"), "# changed synthetic kernel").unwrap();
    let (code, conflict) = invoke(&mut run());
    assert_eq!(code, 2, "{conflict}");
    assert_eq!(conflict["errors"][0]["field_path"], "budget");
    fixture.one_invocation();
    fixture.balance(0, 1, Some(State::Confirmed));
}
