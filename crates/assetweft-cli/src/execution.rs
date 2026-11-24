use super::protocol::{envelope, error, existing_database, library_outcome};
use assetweft_core::{
    application::Application,
    execution::byo_gpu::{self, Invalid, Outcome, Profile, Run, RunError},
};
use clap::{Arg, ArgAction, ArgMatches, Command, value_parser};
use serde_json::{Value, json};
use std::path::PathBuf;

pub(super) fn grammar() -> Command {
    let path = |name, required| {
        Arg::new(name)
            .long(name)
            .required(required)
            .value_parser(value_parser!(PathBuf))
    };
    let string = |name, required| Arg::new(name).long(name).required(required);
    Command::new("byo-gpu")
        .about("Use an optional operator-supplied remote GPU runner")
        .subcommand_required(true)
        .subcommand(Command::new("profiles").about("List supported profiles and local policy"))
        .subcommand(
            Command::new("kaggle-validate")
                .about("Check a private Kaggle bundle and credential presence locally")
                .arg(path("bundle", true)),
        )
        .subcommand(
            Command::new("run")
                .about("Claim one approved local Task and run an external adapter at most once")
                .arg(path("db", true))
                .arg(string("budget-id", true))
                .arg(string("intent-id", true))
                .arg(string("profile", true).value_parser(["hf-zerogpu", "kaggle-cli"]))
                .arg(string("model", true))
                .arg(path("out-dir", true))
                .arg(path("image", false))
                .arg(string("prompt", false))
                .arg(path("bundle", false))
                .arg(path("adapter-bin", false).help("Trusted executable path or name on PATH"))
                .arg(
                    Arg::new("allow-remote-upload")
                        .long("allow-remote-upload")
                        .action(ArgAction::SetTrue),
                ),
        )
}

pub(super) fn response(
    args: &ArgMatches,
    call_id: Option<&String>,
    max_bytes: u64,
) -> (i32, Value) {
    let (action, args) = args.subcommand().expect("clap requires a BYO GPU action");
    let command = format!("byo-gpu {action}");
    let finish = |code, status, errors, results| {
        (code, envelope(&command, call_id, status, errors, results))
    };
    let invalid = |Invalid(field, message)| {
        finish(
            2,
            "INVALID_INPUT",
            json!([error(field, "INVALID_INPUT", message)]),
            Value::Null,
        )
    };
    let path = |name| args.get_one::<PathBuf>(name).cloned();
    let string = |name| args.get_one::<String>(name).cloned();
    match action {
        "profiles" => finish(
            0,
            "PASS",
            json!([]),
            json!({"profiles": byo_gpu::profiles()}),
        ),
        "kaggle-validate" => {
            match byo_gpu::validate_kaggle_bundle(&path("bundle").expect("required"), max_bytes) {
                Ok(result) => finish(
                    0,
                    "PASS",
                    json!([]),
                    json!({
                        "is_private": true,
                        "enable_gpu": true,
                        "accelerator": result.accelerator,
                        "credential_present": result.credential_present,
                    }),
                ),
                Err(e) => invalid(e),
            }
        }
        "run" => {
            let db = path("db").expect("required");
            if existing_database(&db).is_err() {
                return invalid(Invalid(
                    "db",
                    "--db must name an existing regular library file",
                ));
            }
            let mut app = match Application::open(&db) {
                Ok(app) => app,
                Err(_) => return invalid(Invalid("db", "could not open the existing library")),
            };
            let run = Run {
                budget_id: string("budget-id").expect("required"),
                intent_id: string("intent-id").expect("required"),
                profile: Profile::from_id(&string("profile").expect("required"))
                    .expect("clap grammar"),
                model: string("model").expect("required"),
                out_dir: path("out-dir").expect("required"),
                image: path("image"),
                prompt: string("prompt"),
                bundle: path("bundle"),
                adapter_bin: path("adapter-bin"),
                allow_remote_upload: args.get_flag("allow-remote-upload"),
                max_bytes,
            };
            let report = match app.run_byo_gpu(&run) {
                Ok(report) => report,
                Err(RunError::Invalid(e)) => return invalid(e),
                Err(RunError::Credentials) => {
                    return finish(
                        3,
                        "INPUT_INCOMPLETE",
                        json!([error(
                            "bundle",
                            "INPUT_INCOMPLETE",
                            "Kaggle credential presence could not be established"
                        )]),
                        Value::Null,
                    );
                }
                Err(RunError::Ledger(e)) => {
                    let (code, status) = library_outcome(&e);
                    return finish(
                        code,
                        status,
                        json!([error(
                            "budget",
                            status,
                            "local budget ledger rejected or could not read the Task intent"
                        )]),
                        Value::Null,
                    );
                }
            };
            let (code, status, state, errors, artifact) = match report.outcome {
                Outcome::Succeeded { sha256 } => (
                    0,
                    "PASS",
                    "SUCCEEDED",
                    json!([]),
                    json!({"file": report.artifact, "sha256": sha256}),
                ),
                Outcome::Unknown { diagnostic } => (
                    1,
                    "UNKNOWN",
                    "UNKNOWN",
                    json!([error("adapter", "UNKNOWN", diagnostic)]),
                    Value::Null,
                ),
                Outcome::Released => (
                    1,
                    "RELEASED",
                    "RELEASED",
                    json!([error(
                        "budget",
                        "RELEASED",
                        "local Task reservation was released"
                    )]),
                    Value::Null,
                ),
            };
            finish(
                code,
                status,
                errors,
                json!({
                    "intent_id": run.intent_id,
                    "state": state,
                    "invoked": report.attempted,
                    "unit": "Task",
                    "cash_cost": null,
                    "cost_unknown": true,
                    "artifact": artifact,
                }),
            )
        }
        _ => unreachable!("clap grammar"),
    }
}
