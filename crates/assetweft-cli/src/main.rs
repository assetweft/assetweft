mod assurance;
mod catalog;
mod consumption;
mod execution;
mod production;
mod protocol;

use assetweft_core::{application::Application, assurance as val, backup};
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
    let spec_file = || {
        Arg::new("spec")
            .long("spec")
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
        .subcommand(Command::new("audio-inspect").arg(file()).arg(spec_file().required(false)))
        .subcommand(Command::new("sprite-inspect").arg(file()).arg(spec_file()))
        .subcommand(Command::new("card-template-check").arg(file()).arg(
            Arg::new("illustration").long("illustration").value_parser(value_parser!(PathBuf)),
        ))
        .subcommand(Command::new("face-attachment-inspect").arg(file()).arg(spec_file()))
        .subcommand(Command::new("static-kit-inspect").arg(file()).arg(spec_file()))
        .subcommand(Command::new("rigged-simple-components").arg(file()).arg(
            Arg::new("out-dir").long("out-dir").required(true).value_parser(value_parser!(PathBuf)),
        ))
        .subcommand(Command::new("motion-retarget")
            .about("Verify sampled retargeting and two-bone IK in four stages")
            .arg(file()))
        .subcommand(Command::new("motion-stance")
            .about("Check a declared support foot using sampled world-space motion")
            .arg(file())
            .arg(Arg::new("role").long("role").required(true).value_parser(["grounded-humanoid", "hovering"])))
        .subcommand(Command::new("motion-strike")
            .about("Append the in-place strike clip to Cesium Rigged Simple")
            .arg(Arg::new("with-idle").long("with-idle").help("Include a looping fixed-root distal sway").action(clap::ArgAction::SetTrue))
            .arg(file())
            .arg(Arg::new("out").long("out").required(true).value_parser(value_parser!(PathBuf))))
        .subcommand(Command::new("rig-bend")
            .about("Measure the pinned Cesium Rigged Simple bend using optional Blender 5.1.2")
            .arg(file())
            .arg(Arg::new("blender").long("blender").required(true).value_parser(value_parser!(PathBuf)))
            .arg(Arg::new("out").long("out").required(true).value_parser(value_parser!(PathBuf))))
        .subcommand(
            Command::new("library")
                .about("Initialize, back up, or restore a local library")
                .subcommand_required(true)
                .subcommand(
                    Command::new("init")
                        .about("Initialize a named file under an existing parent; no downgrade")
                        .arg(db()),
                )
                .subcommand(
                    Command::new("backup")
                        .about("Back up an existing library to a new directory")
                        .arg(db())
                        .arg(Arg::new("out").long("out").required(true).value_parser(value_parser!(PathBuf))),
                )
                .subcommand(
                    Command::new("restore")
                        .about("Restore a verified backup to a new directory")
                        .arg(Arg::new("from").long("from").required(true).value_parser(value_parser!(PathBuf)))
                        .arg(Arg::new("dest").long("dest").required(true).value_parser(value_parser!(PathBuf))),
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
                .about("Search and manage local Catalog metadata, references, version exports and feedback")
                .subcommand_required(true)
                .subcommand(
                    Command::new("commit-object")
                        .about("Commit verified local managed bytes; no rights/readiness verification")
                        .arg(db())
                        .arg(Arg::new("version").long("version").required(true).value_parser(value_parser!(PathBuf)))
                        .arg(Arg::new("file").long("file").required(true).value_parser(value_parser!(PathBuf)))
                        .arg(Arg::new("expected-len").long("expected-len").required(true).value_parser(value_parser!(u64))),
                )
                .subcommands([
                    ("materialize-export", "Create or verify one writable local copy of this managed version"),
                    ("evict-export", "Remove this version's verified, library-produced export copy"),
                ].map(|(name, help)| Command::new(name).about(help).arg(db())
                    .arg(Arg::new("version-id").long("version-id").required(true))))
                .subcommand(
                    Command::new("register-version")
                        .about("Register immutable declared metadata; no managed bytes or rights verification")
                        .arg(db())
                        .arg(Arg::new("version").long("version").required(true)
                            .value_parser(value_parser!(PathBuf))),
                )
                .subcommand(
                    Command::new("add-reference")
                        .about("Register an exact existing component reference; no latest resolution")
                        .arg(db())
                        .arg(Arg::new("reference").long("reference").required(true)
                            .value_parser(value_parser!(PathBuf))),
                )
                .subcommands([
                    ("preview", "Generate a PNG thumbnail or WAV audition clip for one exact managed version"),
                    ("version", "Read stored exact version metadata; no byte validation"),
                    ("file-availability", "Check local managed bytes for one exact version"),
                ].map(|(name, help)| Command::new(name).about(help).arg(db())
                    .arg(Arg::new("version-id").long("version-id").required(true))))
                .subcommand(
                    Command::new("list")
                        .about("List registered versions in asset and version ID order")
                        .arg(db())
                        .arg(Arg::new("limit").long("limit").default_value("100").allow_hyphen_values(true)),
                )
                .subcommand(
                    Command::new("search")
                        .about("Find up to 100 registered versions by a literal asset or version ID substring")
                        .arg(db())
                        .arg(Arg::new("query").long("query").required(true)),
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
                        .about("Read stored local head and contract; no fresh validation")
                        .arg(db())
                        .arg(Arg::new("asset-id").long("asset-id").required(true)
                            .value_parser(value_parser!(String))),
                )
                .subcommand(
                    Command::new("feedback-add")
                        .about("Append an opinion for an exact registered version")
                        .arg(db())
                        .arg(Arg::new("feedback-id").long("feedback-id").required(true))
                        .arg(Arg::new("asset-id").long("asset-id").required(true))
                        .arg(Arg::new("version-id").long("version-id").required(true))
                        .arg(Arg::new("decision").long("decision").required(true)
                            .value_parser(["ACCEPT", "REJECT"]))
                        .arg(Arg::new("reason").long("reason")),
                )
                .subcommand(
                    Command::new("feedback-latest")
                        .about("Read the newest opinion for an asset or an exact version")
                        .arg(db())
                        .arg(Arg::new("asset-id").long("asset-id").required(true))
                        .arg(Arg::new("version-id").long("version-id")),
                )
                .subcommand(
                    Command::new("feedback-get")
                        .about("Read one opinion by its exact asset, version and feedback IDs")
                        .arg(db())
                        .arg(Arg::new("asset-id").long("asset-id").required(true))
                        .arg(Arg::new("version-id").long("version-id").required(true))
                        .arg(Arg::new("feedback-id").long("feedback-id").required(true)),
                ),
        )
        .subcommand(
            Command::new("candidate")
                .about("Stage local candidates, attempt declared contracts or query stored records")
                .subcommand_required(true)
                .subcommand(
                    Command::new("initialize")
                        .about("Initialize an absent local head under a declared contract")
                        .arg(db())
                        .arg(Arg::new("version-id").long("version-id").required(true)
                            .value_parser(value_parser!(String)))
                        .arg(Arg::new("contract").long("contract").required(true)
                            .value_parser(value_parser!(PathBuf))),
                )
                .subcommand(
                    Command::new("propose")
                        .about("Stage a managed local byte fixture descriptor; no acceptance or head advance")
                        .arg(db())
                        .arg(Arg::new("proposal").long("proposal").required(true)
                            .value_parser(value_parser!(PathBuf))),
                )
                .subcommand(
                    Command::new("schedule-rigged-simple-motion")
                        .about("Schedule affected steps for a pinned motion edit on an initialized head")
                        .arg(db())
                        .arg(Arg::new("proposal").long("proposal").required(true).value_parser(value_parser!(PathBuf))),
                )
                .subcommand(
                    Command::new("propose-rigged-simple-motion")
                        .about("Freeze an exact Cesium Rigged Simple motion-only byte candidate")
                        .arg(db())
                        .arg(Arg::new("proposal").long("proposal").required(true).value_parser(value_parser!(PathBuf))),
                )
                .subcommand(
                    Command::new("attempt")
                        .about("Attempt a declared local structural contract; may commit a receipt and head")
                        .arg(db())
                        .arg(Arg::new("candidate-id").long("candidate-id").required(true)
                            .value_parser(value_parser!(String))),
                )
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
                    Command::new("claim")
                        .about("Durably claim one ready step; never executes or finishes it")
                        .arg(db()).arg(run_id()),
                )
                .subcommand(
                    Command::new("finish")
                        .about("Record a caller-supplied actual outcome; no execution or verification")
                        .arg(db()).arg(run_id())
                        .arg(Arg::new("step-id").long("step-id").required(true).value_parser(value_parser!(String)))
                        .arg(Arg::new("outcome").long("outcome").required(true).value_parser(value_parser!(PathBuf))),
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
                    Command::new("receipts")
                        .about("Read persisted LOCAL usage receipts for an exact call intent ID")
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
        .subcommand(execution::grammar())
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
fn library_init_response(call_id: Option<&String>, args: &ArgMatches) -> (i32, Value) {
    let db = args.get_one::<PathBuf>("db").expect("required");
    let finish = |code, status, errors, results| {
        (
            code,
            envelope("library init", call_id, status, errors, results),
        )
    };
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
fn library_backup_response(
    action: &str,
    call_id: Option<&String>,
    args: &ArgMatches,
) -> (i32, Value) {
    let path = |name| args.get_one::<PathBuf>(name).expect("required");
    let limits = backup::Limits::default();
    let (command, result) = match action {
        "backup" => {
            let command = "library backup";
            let db = path("db");
            let out = path("out");
            if let Err(e) = protocol::existing_database(db) {
                return (
                    2,
                    envelope(command, call_id, "INVALID_INPUT", json!([e]), Value::Null),
                );
            }
            let app = match Application::open_existing(db) {
                Ok(app) => app,
                Err(e) => return protocol::database_open_error(command, call_id, e),
            };
            let result = app.backup_library(out, limits, &mut |_| true).map(|manifest| {
                json!({"scope":"backed_up_local_library","out_dir":out.display().to_string(),"manifest":manifest})
            });
            (command, result)
        }
        "restore" => {
            let dest = path("dest");
            let result = Application::restore_library(path("from"), dest, limits, &mut |_| true)
                .map(|manifest| {
                    json!({"scope":"restored_local_library","dest_dir":dest.display().to_string(),"manifest":manifest})
                });
            ("library restore", result)
        }
        _ => unreachable!("clap grammar"),
    };
    match result {
        Ok(results) => (0, envelope(command, call_id, "PASS", json!([]), results)),
        Err(e) => {
            let (code, status) = protocol::backup_outcome(&e);
            (
                code,
                envelope(
                    command,
                    call_id,
                    status,
                    json!([error("library", status, e)]),
                    Value::Null,
                ),
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
                .mut_arg("max-bytes", |arg| arg.value_parser(value_parser!(String)))
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
            let command = recovered
                .as_ref()
                .and_then(ArgMatches::subcommand_name)
                .filter(|&name| {
                    code == 2
                        && matches!(
                            name,
                            "static-kit-inspect"
                                | "card-template-check"
                                | "motion-retarget"
                                | "audio-inspect"
                                | "face-attachment-inspect"
                        )
                })
                .unwrap_or(command);
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
    if command == "byo-gpu" {
        return execution::response(args, call_id, limits.max_bytes);
    }
    if command == "library" {
        let (action, args) = args.subcommand().expect("clap requires a library action");
        return match action {
            "init" => library_init_response(call_id, args),
            _ => library_backup_response(action, call_id, args),
        };
    }
    if command == "budget" {
        let (action, budget_args) = args.subcommand().expect("clap requires a budget action");
        let wire = match action {
            "approve" => "budget approve",
            "balance" => "budget balance",
            "reserve" => "budget reserve",
            "intent" => "budget intent",
            "receipts" => "budget receipts",
            "cancel" => "budget cancel",
            _ => unreachable!("clap grammar"),
        };
        return consumption::budget_response(wire, call_id, action, budget_args, limits);
    }
    if command == "catalog" {
        let (action, query_args) = args.subcommand().expect("clap requires a catalog action");
        let wire = match action {
            "materialize-export" => "catalog materialize-export",
            "evict-export" => "catalog evict-export",
            "commit-object" => "catalog commit-object",
            "register-version" => "catalog register-version",
            "add-reference" => "catalog add-reference",
            "version" => "catalog version",
            "search" => "catalog search",
            "list" => "catalog list",
            "preview" => "catalog preview",
            "file-availability" => "catalog file-availability",
            "references" => "catalog references",
            "current" => "catalog current",
            "feedback-add" => "catalog feedback-add",
            "feedback-latest" => "catalog feedback-latest",
            "feedback-get" => "catalog feedback-get",
            _ => unreachable!("clap grammar"),
        };
        if matches!(
            action,
            "materialize-export"
                | "evict-export"
                | "commit-object"
                | "register-version"
                | "add-reference"
                | "feedback-add"
        ) {
            return catalog::write_response(wire, call_id, action, query_args, limits);
        }
        return catalog::query_response(wire, call_id, action, query_args);
    }
    if command == "candidate" {
        let (action, query_args) = args.subcommand().expect("clap requires a candidate action");
        let wire = match action {
            "initialize" => "candidate initialize",
            "propose" => "candidate propose",
            "schedule-rigged-simple-motion" => "candidate schedule-rigged-simple-motion",
            "propose-rigged-simple-motion" => "candidate propose-rigged-simple-motion",
            "attempt" => "candidate attempt",
            "show" => "candidate show",
            "receipt" => "candidate receipt",
            _ => unreachable!("clap grammar"),
        };
        if matches!(
            action,
            "initialize"
                | "propose"
                | "propose-rigged-simple-motion"
                | "schedule-rigged-simple-motion"
        ) {
            return production::candidate_write_response(wire, call_id, action, query_args, limits);
        }
        return production::candidate_response(wire, call_id, action, query_args);
    }
    if command == "workflow" {
        let (action, workflow_args) = args.subcommand().expect("clap requires a workflow action");
        let wire = match action {
            "create" => "workflow create",
            "status" => "workflow status",
            "cancel" => "workflow cancel",
            "claim" => "workflow claim",
            "finish" => "workflow finish",
            _ => unreachable!("clap grammar"),
        };
        return production::workflow_response(wire, call_id, action, workflow_args, limits);
    }
    match command {
        "motion-strike" | "rig-bend" => {
            assurance::rig_profile_response(command, call_id, args, limits)
        }
        "rigged-simple-components" => {
            assurance::rigged_simple_components_response(call_id, args, limits)
        }
        "motion-retarget" => assurance::retarget_response(call_id, args, limits),
        "motion-stance" => assurance::stance_response(call_id, args, limits),
        "sprite-inspect" => assurance::sprite_inspect_response(call_id, args, limits),
        "audio-inspect" => assurance::audio_inspect_response(call_id, args, limits),
        "card-template-check" => assurance::card_template_check_response(call_id, args, limits),
        "face-attachment-inspect" => {
            assurance::face_attachment_inspect_response(call_id, args, limits)
        }
        "static-kit-inspect" => assurance::static_kit_inspect_response(call_id, args, limits),
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
