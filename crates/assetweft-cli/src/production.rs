//! Responsibility: Production CLI incoming adapter for plan, workflows and stored candidate queries.
//! Dependencies: shared transport, Production specification and existing Application/workflow APIs.
//! Callers: main command dispatch; DTOs and persisted-record projection remain private here.
//! Invariants: core owns graph/state/lock policy; no execution, SQL, replay or implicit init.

use crate::protocol::{envelope, error, existing_database, library_outcome, read_json, source};
use assetweft_core::{
    application::Application,
    assurance as val,
    production::specification as spec,
    workflow::{Outcome, State, Step, StepRecord},
};
use clap::ArgMatches;
use serde_json::{Value, json};
use std::path::PathBuf;

/// Read exactly one stored value through its owner; query PASS is not acceptance.
pub(super) fn candidate_response(
    wire: &str,
    call_id: Option<&String>,
    action: &str,
    args: &ArgMatches,
) -> (i32, Value) {
    let db = args.get_one::<PathBuf>("db").expect("required");
    let id = args.get_one::<String>("candidate-id").expect("required");
    let finish =
        |code, status, errors, results| (code, envelope(wire, call_id, status, errors, results));
    if let Err(e) = existing_database(db) {
        return finish(2, "INVALID_INPUT", json!([e]), Value::Null);
    }
    let app = match Application::open(db) {
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
    let result = match action {
        "show" => app.candidate(id).map(|candidate| {
            json!({"scope":"stored_local_candidate_descriptor","candidate":candidate})
        }),
        "receipt" => app.candidate_attempt(id).map(|receipt| {
            json!({"scope":"stored_local_candidate_acceptance_receipt","receipt":receipt})
        }),
        _ => unreachable!("clap grammar"),
    };
    match result {
        Ok(value) => finish(0, "PASS", json!([]), value),
        Err(e) => {
            let (code, status) = library_outcome(&e);
            finish(
                code,
                status,
                json!([error("candidate", status, e)]),
                Value::Null,
            )
        }
    }
}

/// Input shape only; Production owns graph, identity and count policies.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StepInput {
    id: String,
    dependencies: Vec<String>,
}
/// Serde structs also accept positional arrays; this wire requires JSON objects only.
struct StepObject(StepInput);
impl<'de> serde::Deserialize<'de> for StepObject {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor;
        impl<'de> serde::de::Visitor<'de> for ObjectVisitor {
            type Value = StepObject;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a step object with id and dependencies")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                map: M,
            ) -> Result<Self::Value, M::Error> {
                <StepInput as serde::Deserialize>::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
                .map(StepObject)
            }
        }
        deserializer.deserialize_map(ObjectVisitor)
    }
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
pub(super) fn workflow_response(
    wire: &str,
    call_id: Option<&String>,
    action: &str,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let db = args.get_one::<PathBuf>("db").expect("required");
    let run = args.get_one::<String>("run-id").expect("required");
    let finish =
        |code, status, errors, results| (code, envelope(wire, call_id, status, errors, results));
    if let Err(e) = existing_database(db) {
        return finish(2, "INVALID_INPUT", json!([e]), Value::Null);
    }
    if action != "create" && run.is_empty() {
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
    if action == "create" {
        let path = args.get_one::<PathBuf>("steps").expect("required");
        let input: Vec<StepObject> = match read_json(path, limits, "steps") {
            Ok(input) => input,
            Err(e) => return finish(2, "INVALID_INPUT", json!([e]), Value::Null),
        };
        let steps: Vec<_> = input
            .into_iter()
            .map(|StepObject(s)| Step {
                id: s.id,
                dependencies: s.dependencies,
            })
            .collect();
        return match app.create_workflow(run, &steps) {
            Ok(()) => finish(
                0,
                "PASS",
                json!([]),
                json!({"scope":"persisted_workflow_created","run_id":run}),
            ),
            Err(e) => {
                let (code, status) = library_outcome(&e);
                finish(
                    code,
                    status,
                    json!([error("workflow", status, e)]),
                    Value::Null,
                )
            }
        };
    }
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

pub(super) fn plan_response(
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let finish =
        |code, status, errors, results| (code, envelope("plan", call_id, status, errors, results));
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
