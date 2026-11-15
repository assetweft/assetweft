//! Responsibility: M6 bounded read-only CLI and v1 JSON wire DTOs.
//! Dependencies: clap grammar, serde wire parsing, Assurance checks and Production specification APIs.
//! Callers: humans, automation, C1 black-box tests.
//! Invariants: one JSON line stdout; no generation, DB, shell or network calls;
//! supplied context is a declaration, never authenticated by this process.
use assetweft_core::{assurance as val, production::specification as spec};
use clap::{Arg, Command, error::ErrorKind, value_parser};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    path::PathBuf,
};

fn grammar() -> Command {
    let file = || {
        Arg::new("file")
            .required(true)
            .value_parser(value_parser!(PathBuf))
    };
    Command::new("assetweft")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Read-only GLB inspect/validate and generation declaration plan; no execution")
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
                eprint!("{message}");
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
        eprintln!("assetweft: failed to write JSON response");
        std::process::exit(6);
    }
    std::process::exit(code);
}
