//! Responsibility: M6 bounded machine CLI: read-only GLB/declaration checks plus persisted
//! workflow status/cancel over a caller-supplied existing library; v1 JSON wire DTOs.
//! Dependencies: clap grammar, serde wire parsing, Assurance checks, Production specification
//! and the shared Application workflow use cases.
//! Callers: humans, automation, C1 black-box tests.
//! Invariants: one JSON line stdout; no generation, shell or network calls; workflow commands
//! only parse, project and report persisted Production state, never execute or reconcile.
//! Supplied context is a declaration, never authenticated by this process.
use assetweft_core::{
    application::Application,
    assurance as val,
    library::LibraryError,
    production::specification as spec,
    workflow::{Outcome, State, StepRecord},
};
use clap::{Arg, ArgMatches, Command, error::ErrorKind, value_parser};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

fn grammar() -> Command {
    let file = || {
        Arg::new("file")
            .required(true)
            .value_parser(value_parser!(PathBuf))
    };
    let db = || {
        Arg::new("db")
            .long("db")
            .required(true)
            .value_parser(value_parser!(PathBuf))
    };
    let run_id = || {
        Arg::new("run-id")
            .long("run-id")
            .required(true)
            .value_parser(value_parser!(String))
    };
    Command::new("assetweft")
        .version(env!("CARGO_PKG_VERSION"))
        .about(
            "Bounded GLB/declaration checks plus persisted workflow status/cancel; no generation",
        )
        .arg(Arg::new("call-id").long("call-id").global(true))
        .arg(
            Arg::new("max-bytes")
                .long("max-bytes")
                .global(true)
                .default_value("16777216")
                .value_parser(value_parser!(u64).range(1..=16777216)),
        )
        .subcommand_required(true)
        .subcommand(Command::new("inspect").arg(file()))
        .subcommand(
            Command::new("validate").arg(file()).arg(
                Arg::new("role")
                    .long("role")
                    .default_value("static-mesh")
                    .value_parser(["static-mesh", "rigged-character", "animated-character"]),
            ),
        )
        .subcommand(
            Command::new("workflow")
                .about(
                    "Persisted local workflow state: status reads, cancel requests; no execution",
                )
                .subcommand_required(true)
                .subcommand(
                    Command::new("status")
                        .about("Read a run's cancellation request and ordered persisted steps")
                        .arg(db())
                        .arg(run_id()),
                )
                .subcommand(
                    Command::new("cancel")
                        .about("Persist a cancellation request; never stops processes or refunds")
                        .arg(db())
                        .arg(run_id()),
                ),
        )
        .subcommand(
            Command::new("plan")
                .arg(
                    Arg::new("request")
                        .long("request")
                        .required(true)
                        .value_parser(value_parser!(PathBuf)),
                )
                .arg(
                    Arg::new("trusted-context")
                        .long("trusted-context")
                        .required(true)
                        .value_parser(value_parser!(PathBuf)),
                ),
        )
}
fn envelope(
    command: &str,
    call_id: Option<&String>,
    status: &str,
    errors: Value,
    results: Value,
) -> Value {
    json!({"schema_version":1,"command":command,"call_id":call_id,"status":status,
        "errors":errors,"results":results})
}
fn error(path: &str, code: &str, message: impl ToString) -> Value {
    json!({"field_path":path,"code":code,"message":message.to_string()})
}
fn read_json<T: DeserializeOwned>(
    path: &std::path::Path,
    limits: val::ValidationLimits,
    field: &str,
) -> Result<T, Value> {
    let bytes = val::read_bounded(path, limits).map_err(|e| error(field, "INVALID_INPUT", e))?;
    serde_json::from_slice(&bytes).map_err(|e| error(field, "INVALID_INPUT", e))
}
fn source(s: &val::SourceDeclaration) -> Value {
    json!({"class":s.class.as_str(),"declared_license":s.declared_license,"verified_by_checker":s.verified_by_checker()})
}
fn issue_exit(code: spec::IssueCode) -> i32 {
    match code {
        spec::IssueCode::LockViolation => 4,
        spec::IssueCode::InvalidInput => 2,
        spec::IssueCode::InputIncomplete => 3,
        spec::IssueCode::Unsupported => 5,
        spec::IssueCode::Fail => 1,
    }
}
/// Rejects a missing or non-regular `--db` before any create-capable `Application::open`.
/// This precheck is not race-proof and does not make the later open read-only.
fn existing_database(path: &Path) -> Result<(), Value> {
    if path.as_os_str().is_empty() {
        return Err(error(
            "db",
            "INVALID_INPUT",
            "--db must name an existing regular file",
        ));
    }
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(error(
            "db",
            "INVALID_INPUT",
            "--db must be a regular file, not a directory or special file",
        )),
        Err(e) => Err(error(
            "db",
            "INVALID_INPUT",
            format!("--db does not exist or cannot be read: {e}"),
        )),
    }
}
/// Deterministic `LibraryError` categories only; no error-string parsing and no typed NotFound.
fn library_outcome(failure: &LibraryError) -> (i32, &'static str) {
    match failure {
        LibraryError::InvalidInput(_) | LibraryError::Constraint(_) => (2, "INVALID_INPUT"),
        LibraryError::Uninitialized => (3, "INPUT_INCOMPLETE"),
        LibraryError::FutureSchema(_) => (5, "UNSUPPORTED"),
        LibraryError::Busy(_) | LibraryError::Io(_) => (6, "INTERNAL"),
        LibraryError::Database(_) | LibraryError::UnknownValue(_) => (1, "FAIL"),
    }
}
/// Local wire projection of a non-Serialize persisted record; exhaustive, meaning-preserving.
fn step_value(record: &StepRecord) -> Result<Value, String> {
    let (state, raw, evidence) = match (&record.state, record.outcome.as_ref()) {
        (State::Pending, None) => ("pending", None, None),
        (State::Running, None) => ("running", None, None),
        (State::Succeeded, Some(Outcome::Success { raw, evidence })) => {
            ("succeeded", Some(raw.as_str()), evidence.as_deref())
        }
        (State::Failed, Some(Outcome::Failure { raw, evidence })) => {
            ("failed", Some(raw.as_str()), evidence.as_deref())
        }
        (state, outcome) => {
            return Err(format!(
                "inconsistent persisted step state: {state:?}/{outcome:?}"
            ));
        }
    };
    Ok(json!({"id":record.id,"state":state,"raw":raw,"evidence":evidence}))
}
/// Status reads steps then cancellation sequentially (not an atomic snapshot); cancel only
/// persists the request and never asserts process, billing or budget effects.
fn workflow_response(
    wire: &str,
    call_id: Option<&String>,
    action: &str,
    args: &ArgMatches,
) -> (i32, Value) {
    let db = args.get_one::<PathBuf>("db").expect("required");
    let run = args.get_one::<String>("run-id").expect("required");
    let finish =
        |code, status, errors, results| (code, envelope(wire, call_id, status, errors, results));
    if let Err(e) = existing_database(db) {
        return finish(2, "INVALID_INPUT", json!([e]), Value::Null);
    }
    if run.is_empty() {
        return finish(
            2,
            "INVALID_INPUT",
            json!([error(
                "run_id",
                "INVALID_INPUT",
                "--run-id must not be empty"
            )]),
            Value::Null,
        );
    }
    let mut app = match Application::open(db) {
        Ok(app) => app,
        Err(e) => {
            return finish(
                2,
                "INVALID_INPUT",
                json!([error("db", "INVALID_INPUT", e.to_string())]),
                Value::Null,
            );
        }
    };
    if action == "status" {
        let steps = match app.workflow_steps(run) {
            Ok(steps) => steps,
            Err(e) => {
                let (code, status) = library_outcome(&e);
                return finish(
                    code,
                    status,
                    json!([error("workflow", status, e.to_string())]),
                    Value::Null,
                );
            }
        };
        let cancelled = match app.workflow_cancelled(run) {
            Ok(cancelled) => cancelled,
            Err(e) => {
                let (code, status) = library_outcome(&e);
                return finish(
                    code,
                    status,
                    json!([error("workflow", status, e.to_string())]),
                    Value::Null,
                );
            }
        };
        let mut projected = Vec::with_capacity(steps.len());
        for record in &steps {
            match step_value(record) {
                Ok(value) => projected.push(value),
                Err(message) => {
                    return finish(
                        1,
                        "FAIL",
                        json!([error("workflow", "FAIL", message)]),
                        Value::Null,
                    );
                }
            }
        }
        return finish(
            0,
            "PASS",
            json!([]),
            json!({
                "scope":"persisted_workflow_state",
                "snapshot":"SEQUENTIAL_STEPS_THEN_CANCELLATION_NOT_ATOMIC",
                "run_id":run,
                "cancellation_requested":cancelled,
                "steps":projected,
            }),
        );
    }
    match app.cancel_workflow(run) {
        Ok(()) => finish(
            0,
            "PASS",
            json!([]),
            json!({
                "scope":"persisted_cancellation_request_only",
                "run_id":run,
                "cancellation_requested":true,
            }),
        ),
        Err(e) => {
            let (code, status) = library_outcome(&e);
            finish(
                code,
                status,
                json!([error("workflow", status, e.to_string())]),
                Value::Null,
            )
        }
    }
}
fn run() -> (i32, Value) {
    let argv: Vec<_> = std::env::args_os().collect();
    let matches = match grammar().try_get_matches_from(&argv) {
        Ok(m) => m,
        Err(e) => {
            let (code, status, command) = match e.kind() {
                ErrorKind::DisplayHelp => (0, "HELP", "help"),
                ErrorKind::DisplayVersion => (0, "VERSION", "version"),
                _ => (2, "INVALID_INPUT", "arguments"),
            };
            let recovered = grammar()
                .disable_help_flag(true)
                .disable_version_flag(true)
                .subcommand_required(false)
                .arg(
                    Arg::new("help")
                        .long("help")
                        .short('h')
                        .global(true)
                        .action(clap::ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("version")
                        .long("version")
                        .short('V')
                        .global(true)
                        .action(clap::ArgAction::SetTrue),
                )
                .ignore_errors(true)
                .try_get_matches_from(&argv)
                .ok();
            let recovered_id = recovered
                .as_ref()
                .and_then(|m| m.get_one::<String>("call-id"));
            let message = e.to_string();
            if code != 0 {
                let _ = write!(io::stderr().lock(), "{message}");
            }
            return (
                code,
                envelope(
                    command,
                    recovered_id,
                    status,
                    if code == 0 {
                        json!([])
                    } else {
                        json!([error("arguments", status, &message)])
                    },
                    if code == 0 {
                        json!({"text":message})
                    } else {
                        Value::Null
                    },
                ),
            );
        }
    };
    let (command, args) = matches.subcommand().expect("clap requires subcommand");
    let call_id = matches.get_one::<String>("call-id");
    if command == "workflow" {
        let (action, workflow_args) = args.subcommand().expect("clap requires a workflow action");
        let wire = match action {
            "status" => "workflow status",
            _ => "workflow cancel",
        };
        return workflow_response(wire, call_id, action, workflow_args);
    }
    let limits = val::ValidationLimits {
        max_bytes: *matches.get_one::<u64>("max-bytes").expect("default"),
    };
    let finish =
        |code, status, errors, results| (code, envelope(command, call_id, status, errors, results));
    match command {
        "inspect" => {
            let path = args.get_one::<PathBuf>("file").expect("required");
            match val::checks::inspect_geometry_file(path, limits) {
                Ok(report) => finish(
                    0,
                    "PASS",
                    json!([]),
                    json!({"scope":"static_glb_structure","geometry":report}),
                ),
                Err(e) => finish(
                    2,
                    "INVALID_INPUT",
                    json!([error("file", "INVALID_INPUT", format!("{e:?}"))]),
                    Value::Null,
                ),
            }
        }
        "validate" => {
            let role = match args.get_one::<String>("role").expect("default").as_str() {
                "rigged-character" => val::RoleRequirement::RiggedCharacter,
                "animated-character" => val::RoleRequirement::AnimatedCharacter,
                _ => val::RoleRequirement::StaticMesh,
            };
            let report = match val::checks::validate_file(
                args.get_one::<PathBuf>("file").expect("required"),
                role,
                val::SourceDeclaration::declared(val::SourceClass::Unknown),
                limits,
            ) {
                Ok(r) => r,
                Err(e) => {
                    return finish(
                        2,
                        "INVALID_INPUT",
                        json!([error("file", "INVALID_INPUT", e)]),
                        Value::Null,
                    );
                }
            };
            let errors: Vec<_> = report
                .checks
                .iter()
                .filter(|c| c.status == val::CheckStatus::Fail)
                .map(|c| error(c.id, "FAIL", &c.detail))
                .collect();
            let failed = report.format_status == val::CheckStatus::Fail
                || report.role.status == val::CheckStatus::Fail
                || !errors.is_empty();
            finish(
                if failed { 1 } else { 0 },
                if failed { "FAIL" } else { "PASS" },
                json!(errors),
                json!({
                "scope":"bounded_file_and_role_checks","checker_version":report.checker_version,
                "file_sha256":report.file_sha256,"bytes_read":report.bytes_read,"format":report.format,
                "format_status":report.format_status.as_str(),"units":report.units,"source":source(&report.source),
                "role":{"requirement":report.role.requirement.as_str(),"status":report.role.status.as_str(),
                    "satisfied":report.role.satisfied,"present":report.role.present,"missing":report.role.missing},
                "checks":report.checks.iter().map(|c|json!({"id":c.id,"status":c.status.as_str(),"detail":c.detail})).collect::<Vec<_>>(),
                "not_run":report.not_run}),
            )
        }
        "plan" => {
            let request: spec::GenerationSpecRequest = match read_json(
                args.get_one::<PathBuf>("request").expect("required"),
                limits,
                "request",
            ) {
                Ok(v) => v,
                Err(e) => return finish(2, "INVALID_INPUT", json!([e]), Value::Null),
            };
            let context: spec::TrustedContext = match read_json(
                args.get_one::<PathBuf>("trusted-context")
                    .expect("required"),
                limits,
                "trusted_context",
            ) {
                Ok(v) => v,
                Err(e) => return finish(2, "INVALID_INPUT", json!([e]), Value::Null),
            };
            let outcome = spec::validate(&request, &context);
            // Deterministic priority when the core returns multiple issue categories.
            let code = [4, 2, 3, 5, 1]
                .into_iter()
                .find(|c| outcome.issues.iter().any(|i| issue_exit(i.code) == *c))
                .unwrap_or(0);
            let status = if code == 0 {
                spec::STATUS_DECLARATION_CONSISTENT
            } else {
                match code {
                    4 => "LOCK_VIOLATION",
                    2 => "INVALID_INPUT",
                    3 => "INPUT_INCOMPLETE",
                    5 => "UNSUPPORTED",
                    _ => "FAIL",
                }
            };
            finish(
                code,
                status,
                json!(
                    outcome
                        .issues
                        .iter()
                        .map(|i| error(&i.field_path, i.code.as_str(), &i.message))
                        .collect::<Vec<_>>()
                ),
                json!({
                "scope":"generation_spec_declaration_only","declaration_status":outcome.status,
                "execution":"NOT_RUN","authenticity":"NOT_VERIFIED","source":source(&outcome.source),"not_run":outcome.not_run,
                "recipe":{"description":"Read supplied base/reference declarations; describe editable and locked scope; no execution or commit",
                    "base_version":request.base_version,"editable_components":request.editable_components,
                    "locked_components":request.locked_components.iter().map(|l|json!({"role":l.role,"declaration":l.declaration})).collect::<Vec<_>>(),
                    "output_contract":request.output_contract,"budget":request.budget.map(|b|json!({"max_calls":b.max_calls,"cost_status":b.cost_status,"cost_value":b.cost_value}))}}),
            )
        }
        _ => unreachable!("clap grammar"),
    }
}
fn main() {
    let (code, response) = run();
    let mut out = io::stdout().lock();
    if serde_json::to_writer(&mut out, &response)
        .and_then(|()| out.write_all(b"\n").map_err(serde_json::Error::io))
        .is_err()
    {
        let _ = writeln!(
            io::stderr().lock(),
            "assetweft: failed to write JSON response"
        );
        std::process::exit(6);
    }
    std::process::exit(code);
}
