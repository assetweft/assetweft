//! Responsibility: Assurance CLI incoming adapter for bounded inspect/validate reports.
//! Dependencies: shared transport and existing public Assurance checks/report types.
//! Callers: main command dispatch.
//! Invariants: unchanged bounds, role/source/check diagnostics and NOT_RUN scope; projection adds no validation policy or production acceptance claim.

use crate::protocol::{envelope, error, source};
use assetweft_core::assurance as val;
use clap::ArgMatches;
use serde_json::{Value, json};
use std::path::PathBuf;

pub(super) fn inspect_response(
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let finish = |code, status, errors, results| {
        (code, envelope("inspect", call_id, status, errors, results))
    };
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

pub(super) fn validate_response(
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let finish = |code, status, errors, results| {
        (code, envelope("validate", call_id, status, errors, results))
    };
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

pub(super) fn rig_bend_response(
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let path = |id| args.get_one::<PathBuf>(id).expect("required").as_path();
    match val::rig_blender::measure_rigged_simple(
        path("blender"),
        path("file"),
        path("out"),
        limits,
    ) {
        Ok(report) => (0, envelope("rig-bend", call_id, "PASS", json!([]), report)),
        Err(e) => {
            let message = e.to_string();
            let (code, status) = if e.is::<val::ValidationError>() {
                (2, "INVALID_INPUT")
            } else if message.starts_with("UNSUPPORTED") {
                (1, "NOT_RUN")
            } else {
                (1, "FAIL")
            };
            (
                code,
                envelope(
                    "rig-bend",
                    call_id,
                    status,
                    json!([error("rig-bend", status, message)]),
                    Value::Null,
                ),
            )
        }
    }
}
