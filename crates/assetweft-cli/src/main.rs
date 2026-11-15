//! Responsibility: CLI process entry, argument grammar/dispatch and explicit library bootstrap.
//! Dependencies: clap, shared transport, business incoming adapters and existing Application init.
//! Callers: humans, automation and unchanged C1 black-box tests.
//! Invariants: one compact v1 JSON line; stderr is best effort, stdout failure exits 6. Only explicit library init initializes/migrates; no generation, shell, network or replay.

mod assurance;
mod catalog;
mod consumption;
mod production;
mod protocol;

use assetweft_core::{application::Application, assurance as val};
use clap::{Arg, ArgMatches, Command, error::ErrorKind, value_parser};
use protocol::{envelope, error, library_outcome};
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
            "Bounded checks, library/workflow state and local budget declarations; no generation",
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
            Command::new("library")
                .about("Explicit local library initialization; may apply existing product migrations")
                .subcommand_required(true)
                .subcommand(
                    Command::new("init")
                        .about("Initialize a named file under an existing parent; no downgrade")
                        .arg(db()),
                ),
        )
        .subcommand(
            Command::new("validate").arg(file()).arg(
                Arg::new("role")
                    .long("role")
                    .default_value("static-mesh")
                    .value_parser(["static-mesh", "rigged-character", "animated-character"]),
            ),
        )
        .subcommand(
            Command::new("catalog")
                .about("Query stored local metadata and fixture head; no availability or acceptance check")
                .subcommand_required(true)
                .subcommand(
                    Command::new("version")
                        .about("Read stored exact version metadata; no byte validation")
                        .arg(db())
                        .arg(Arg::new("version-id").long("version-id").required(true)
                            .value_parser(value_parser!(String))),
                )
                .subcommand(
                    Command::new("references")
                        .about("Read stored ordered exact component references; no expansion")
                        .arg(db())
                        .arg(Arg::new("version-id").long("version-id").required(true)
                            .value_parser(value_parser!(String))),
                )
                .subcommand(
                    Command::new("current")
                        .about("Read stored local fixture head and contract; no fresh validation")
                        .arg(db())
                        .arg(Arg::new("asset-id").long("asset-id").required(true)
                            .value_parser(value_parser!(String))),
                ),
        )
        .subcommand(
            Command::new("candidate")
                .about("Query stored local descriptors and historical receipts; no validation or acceptance")
                .subcommand_required(true)
                .subcommand(
                    Command::new("show")
                        .about("Read the stored local candidate descriptor; no remeasurement")
                        .arg(db())
                        .arg(Arg::new("candidate-id").long("candidate-id").required(true)
                            .value_parser(value_parser!(String))),
                )
                .subcommand(
                    Command::new("receipt")
                        .about("Read the stored historical acceptance receipt; no fresh validation")
                        .arg(db())
                        .arg(Arg::new("candidate-id").long("candidate-id").required(true)
                            .value_parser(value_parser!(String))),
                ),
        )
        .subcommand(
            Command::new("workflow")
                .about(
                    "Create/read local workflow state or request cancellation; no execution or auto-init",
                )
                .subcommand_required(true)
                .subcommand(
                    Command::new("create")
                        .about("Persist steps JSON in an existing initialized library; never executes")
                        .arg(db())
                        .arg(run_id())
                        .arg(
                            Arg::new("steps")
                                .long("steps")
                                .required(true)
                                .value_parser(value_parser!(PathBuf)),
                        ),
                )
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
            Command::new("budget")
                .about("LOCAL declarations and call-intent ledger operations; no payment or provider submission")
                .subcommand_required(true)
                .subcommand(
                    Command::new("approve")
                        .about("Store a caller's local declaration in an existing initialized library")
                        .arg(db())
                        .arg(Arg::new("allowance").long("allowance").required(true)
                            .value_parser(value_parser!(PathBuf))),
                )
                .subcommand(
                    Command::new("balance")
                        .about("Read the exact local allowance, reserved and spent integers")
                        .arg(db())
                        .arg(Arg::new("budget-id").long("budget-id").required(true)
                            .value_parser(value_parser!(String))),
                )
                .subcommand(
                    Command::new("reserve")
                        .about("Reserve a caller's LOCAL call intent; no authentication or provider price approval")
                        .arg(db())
                        .arg(Arg::new("request").long("request").required(true)
                            .value_parser(value_parser!(PathBuf))),
                )
                .subcommand(
                    Command::new("intent")
                        .about("Read the durable LOCAL call intent; no provider query")
                        .arg(db())
                        .arg(Arg::new("intent-id").long("intent-id").required(true)
                            .value_parser(value_parser!(String))),
                )
                .subcommand(
                    Command::new("cancel")
                        .about("Request LOCAL call cancellation; remote work and fees may continue")
                        .arg(db())
                        .arg(Arg::new("intent-id").long("intent-id").required(true)
                            .value_parser(value_parser!(String))),
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
/// The only CLI path authorized to create/initialize a library, using existing migrations.
fn library_init_response(call_id: Option<&String>, args: &ArgMatches) -> (i32, Value) {
    let db = args.get_one::<PathBuf>("db").expect("required");
    let finish = |code, status, errors, results| {
        (
            code,
            envelope("library init", call_id, status, errors, results),
        )
    };
    // New files are allowed; existing directories/special files are not SQLite inputs.
    match std::fs::metadata(db) {
        Ok(metadata) if !metadata.is_file() => {
            return finish(
                2,
                "INVALID_INPUT",
                json!([error(
                    "db",
                    "INVALID_INPUT",
                    "--db must name a regular file"
                )]),
                Value::Null,
            );
        }
        Err(e) if e.kind() != io::ErrorKind::NotFound => {
            return finish(
                2,
                "INVALID_INPUT",
                json!([error("db", "INVALID_INPUT", e)]),
                Value::Null,
            );
        }
        _ => {}
    }
    let mut app = match Application::open(db) {
        Ok(app) => app,
        Err(e) => {
            return finish(
                2,
                "INVALID_INPUT",
                json!([error("db", "INVALID_INPUT", e)]),
                Value::Null,
            );
        }
    };
    match app.initialize_library() {
        Ok(()) => finish(0, "PASS", json!([]), json!({"scope":"initialized_library"})),
        Err(e) => {
            let (code, status) = library_outcome(&e);
            finish(code, status, json!([error("db", status, e)]), Value::Null)
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
    let limits = val::ValidationLimits {
        max_bytes: *matches.get_one::<u64>("max-bytes").expect("default"),
    };
    if command == "library" {
        let (_, init_args) = args.subcommand().expect("clap requires library init");
        return library_init_response(call_id, init_args);
    }
    if command == "budget" {
        let (action, budget_args) = args.subcommand().expect("clap requires a budget action");
        let wire = match action {
            "approve" => "budget approve",
            "balance" => "budget balance",
            "reserve" => "budget reserve",
            "intent" => "budget intent",
            "cancel" => "budget cancel",
            _ => unreachable!("clap grammar"),
        };
        return consumption::budget_response(wire, call_id, action, budget_args, limits);
    }
    if command == "catalog" {
        let (action, query_args) = args.subcommand().expect("clap requires a catalog query");
        let wire = match action {
            "version" => "catalog version",
            "references" => "catalog references",
            "current" => "catalog current",
            _ => unreachable!("clap grammar"),
        };
        return catalog::query_response(wire, call_id, action, query_args);
    }
    if command == "candidate" {
        let (action, query_args) = args.subcommand().expect("clap requires a candidate query");
        let wire = match action {
            "show" => "candidate show",
            "receipt" => "candidate receipt",
            _ => unreachable!("clap grammar"),
        };
        return production::candidate_response(wire, call_id, action, query_args);
    }
    if command == "workflow" {
        let (action, workflow_args) = args.subcommand().expect("clap requires a workflow action");
        let wire = match action {
            "create" => "workflow create",
            "status" => "workflow status",
            _ => "workflow cancel",
        };
        return production::workflow_response(wire, call_id, action, workflow_args, limits);
    }
    match command {
        "inspect" => assurance::inspect_response(call_id, args, limits),
        "validate" => assurance::validate_response(call_id, args, limits),
        "plan" => production::plan_response(call_id, args, limits),
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
