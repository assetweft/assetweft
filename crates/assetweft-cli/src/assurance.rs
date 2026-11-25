use crate::protocol::{envelope, error, source};
use assetweft_core::assurance as val;
use clap::ArgMatches;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

pub(super) fn audio_inspect_response(
    command: &str,
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let file = args.get_one::<PathBuf>("file").expect("required");
    let inspected = if command == "audio-connector-inspect" {
        val::checks::inspect_audio_connector_file(file, limits)
            .map(|report| (json!(report), report.violations))
    } else {
        let spec = args.get_one::<PathBuf>("spec").map(PathBuf::as_path);
        val::checks::inspect_audio_file(file, spec, limits)
            .map(|report| (json!(report), report.violations))
    };
    match inspected {
        Ok((results, violations)) => {
            let errors: Vec<_> = violations.iter().map(|&id| error(id, "FAIL", id)).collect();
            let (code, status) = if errors.is_empty() {
                (0, "PASS")
            } else {
                (1, "FAIL")
            };
            (
                code,
                envelope(command, call_id, status, json!(errors), results),
            )
        }
        Err(e) => (
            2,
            envelope(
                command,
                call_id,
                "INVALID_INPUT",
                json!([error(e.field, "INVALID_INPUT", e.detail)]),
                Value::Null,
            ),
        ),
    }
}

pub(super) fn rigged_simple_components_response(
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let path = args.get_one::<PathBuf>("file").expect("required");
    let out_dir = args.get_one::<PathBuf>("out-dir").expect("required");
    let bytes = match val::read_bounded(path, limits) {
        Ok(bytes) => bytes,
        Err(e) => {
            return (
                2,
                envelope(
                    "rigged-simple-components",
                    call_id,
                    "INVALID_INPUT",
                    json!([error("file", "INVALID_INPUT", format!("{e:?}"))]),
                    Value::Null,
                ),
            );
        }
    };
    let parts = match assetweft_core::motion::components::rigged_simple_components(&bytes) {
        Ok(parts) => parts,
        Err(msg) => {
            return (
                1,
                envelope(
                    "rigged-simple-components",
                    call_id,
                    "NOT_RUN",
                    json!([error("rigged-simple-components", "NOT_RUN", msg)]),
                    Value::Null,
                ),
            );
        }
    };
    if let Err(e) = fs::create_dir(out_dir) {
        return (
            2,
            envelope(
                "rigged-simple-components",
                call_id,
                "INVALID_INPUT",
                json!([error("out-dir", "INVALID_INPUT", e.to_string())]),
                Value::Null,
            ),
        );
    }
    let mut components = Vec::new();
    let mut written = Vec::new();
    for (role, data) in parts {
        let role = format!("{role:?}").to_lowercase();
        let file = format!("{role}.json");
        let target = out_dir.join(&file);
        if let Err(e) = fs::write(&target, &data) {
            for component in &written {
                let _ = fs::remove_file(component);
            }
            let _ = fs::remove_file(target);
            return (
                2,
                envelope(
                    "rigged-simple-components",
                    call_id,
                    "INVALID_INPUT",
                    json!([error("out-dir", "INVALID_INPUT", e.to_string())]),
                    Value::Null,
                ),
            );
        }
        written.push(target);
        components.push(json!({"role":role,"file":file,"byte_length":data.len()}));
    }
    (
        0,
        envelope(
            "rigged-simple-components",
            call_id,
            "PASS",
            json!([]),
            json!({"scope":"rigged_simple_components_v1","out_dir":out_dir.display().to_string(),"components":components}),
        ),
    )
}

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

pub(super) fn rig_profile_response(
    command: &str,
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let path = |id| args.get_one::<PathBuf>(id).expect("required").as_path();
    let result = match command {
        "rig-bend" => val::rig_blender::measure_rigged_simple(
            path("blender"),
            path("file"),
            path("out"),
            limits,
        ),
        "motion-strike" if args.get_flag("with-idle") => {
            assetweft_core::motion::add_strike_with_idle(path("file"), path("out"), limits)
        }
        "motion-strike" => assetweft_core::motion::add_strike(path("file"), path("out"), limits),
        _ => unreachable!("clap grammar"),
    };
    match result {
        Ok(report) => (0, envelope(command, call_id, "PASS", json!([]), report)),
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
                    command,
                    call_id,
                    status,
                    json!([error(command, status, message)]),
                    Value::Null,
                ),
            )
        }
    }
}

pub(super) fn retarget_response(
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let command = "motion-retarget";
    let path = args.get_one::<PathBuf>("file").expect("required");
    match assetweft_core::motion::retarget::assess(path, limits) {
        Ok(report) => {
            let (code, status) = if report.failed_stage.is_some() {
                (1, "FAIL")
            } else {
                (0, "PASS")
            };
            (
                code,
                envelope(command, call_id, status, json!([]), json!(report)),
            )
        }
        Err(e) => (
            2,
            envelope(
                command,
                call_id,
                "INVALID_INPUT",
                json!([error("file", "INVALID_INPUT", e)]),
                Value::Null,
            ),
        ),
    }
}

pub(super) fn interaction_response(
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let command = "motion-interaction";
    let path = args.get_one::<PathBuf>("file").expect("required");
    match assetweft_core::motion::interaction::assess(path, limits) {
        Ok(report) => {
            let (code, status) = if report.failure.is_some() {
                (1, "FAIL")
            } else {
                (0, "PASS")
            };
            (
                code,
                envelope(command, call_id, status, json!([]), json!(report)),
            )
        }
        Err(e) => (
            2,
            envelope(
                command,
                call_id,
                "INVALID_INPUT",
                json!([error("file", "INVALID_INPUT", e)]),
                Value::Null,
            ),
        ),
    }
}

pub(super) fn stance_response(
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    use assetweft_core::motion::stance::{MotionRole, Status, assess};
    let role = match args.get_one::<String>("role").expect("required").as_str() {
        "grounded-humanoid" => MotionRole::GroundedHumanoid,
        "hovering" => MotionRole::Hovering,
        _ => unreachable!("clap grammar"),
    };
    match assess(
        args.get_one::<PathBuf>("file").expect("required"),
        role,
        limits,
    ) {
        Ok(report) => {
            let (code, status) = match report.status {
                Status::Pass => (0, "PASS"),
                Status::Fail => (1, "FAIL"),
                Status::Inapplicable => (5, "INAPPLICABLE"),
                Status::NotRun => (3, "NOT_RUN"),
            };
            (
                code,
                envelope("motion-stance", call_id, status, json!([]), json!(report)),
            )
        }
        Err(e) => (
            2,
            envelope(
                "motion-stance",
                call_id,
                "INVALID_INPUT",
                json!([error("file", "INVALID_INPUT", e)]),
                Value::Null,
            ),
        ),
    }
}

pub(super) fn static_kit_inspect_response(
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let command = "static-kit-inspect";
    let path = |name| args.get_one::<PathBuf>(name).expect("required");
    match val::checks::inspect_static_kit_file(path("file"), path("spec"), limits) {
        Ok(report) => {
            let errors: Vec<_> = report
                .violations
                .iter()
                .map(|violation| error(violation.field, "FAIL", &violation.detail))
                .collect();
            let (code, status) = if errors.is_empty() {
                (0, "PASS")
            } else {
                (1, "FAIL")
            };
            (
                code,
                envelope(command, call_id, status, json!(errors), json!(report)),
            )
        }
        Err(e) => (
            2,
            envelope(
                command,
                call_id,
                "INVALID_INPUT",
                json!([error(e.field, "INVALID_INPUT", e.detail)]),
                Value::Null,
            ),
        ),
    }
}

pub(super) fn action_presentation_inspect_response(
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let command = "action-presentation-inspect";
    let spec = args.get_one::<PathBuf>("file").expect("required");
    match val::checks::inspect_action_presentation_file(spec, limits) {
        Ok(report) => {
            let errors: Vec<_> = report
                .violations
                .iter()
                .map(|violation| error(&violation.field, violation.code, violation.detail))
                .collect();
            let (code, status) = if errors.is_empty() {
                (0, "PASS")
            } else {
                (1, "FAIL")
            };
            (
                code,
                envelope(command, call_id, status, json!(errors), json!(report)),
            )
        }
        Err(detail) => (
            2,
            envelope(
                command,
                call_id,
                "INVALID_INPUT",
                json!([error("spec", "INVALID_INPUT", detail)]),
                Value::Null,
            ),
        ),
    }
}

pub(super) fn vfx_inspect_response(
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let command = "vfx-inspect";
    let spec = args.get_one::<PathBuf>("spec").expect("required");
    let flipbook = args.get_one::<PathBuf>("flipbook").map(PathBuf::as_path);
    match val::checks::inspect_vfx_file(spec, flipbook, limits) {
        Ok(report) => {
            let errors: Vec<_> = report
                .violations
                .iter()
                .map(|violation| error(violation.field, "FAIL", violation.detail))
                .collect();
            let (code, status) = if errors.is_empty() {
                (0, "PASS")
            } else {
                (1, "FAIL")
            };
            (
                code,
                envelope(command, call_id, status, json!(errors), json!(report)),
            )
        }
        Err(e) => (
            2,
            envelope(
                command,
                call_id,
                "INVALID_INPUT",
                json!([error(e.field, "INVALID_INPUT", e.detail)]),
                Value::Null,
            ),
        ),
    }
}

pub(super) fn sprite_inspect_response(
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    use val::checks::{SpriteError, inspect_sprite_file};

    let command = "sprite-inspect";
    let path = |name| args.get_one::<PathBuf>(name).expect("required");
    let (code, status, field, detail) =
        match inspect_sprite_file(path("file"), path("spec"), limits) {
            Ok(report) => {
                return (
                    0,
                    envelope(command, call_id, "PASS", json!([]), json!(report)),
                );
            }
            Err(SpriteError::InvalidInput { field, detail }) => (2, "INVALID_INPUT", field, detail),
            Err(SpriteError::Contract(detail)) => (1, "FAIL", "spec", detail.to_owned()),
        };
    (
        code,
        envelope(
            command,
            call_id,
            status,
            json!([error(field, status, detail)]),
            Value::Null,
        ),
    )
}

pub(super) fn card_template_check_response(
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    use val::checks::{CardLayoutError, check_card_template_file};

    let command = "card-template-check";
    let template = args.get_one::<PathBuf>("file").expect("required");
    let illustration = args
        .get_one::<PathBuf>("illustration")
        .map(PathBuf::as_path);
    let (code, status, field, detail) =
        match check_card_template_file(template, illustration, limits) {
            Ok(report) => {
                return (
                    0,
                    envelope(command, call_id, "PASS", json!([]), json!(report)),
                );
            }
            Err(CardLayoutError::InvalidInput { field, detail }) => {
                (2, "INVALID_INPUT", field, detail)
            }
            Err(CardLayoutError::Contract(violation)) => {
                (1, "FAIL", "template", format!("{violation:?}"))
            }
        };
    (
        code,
        envelope(
            command,
            call_id,
            status,
            json!([error(field, status, detail)]),
            Value::Null,
        ),
    )
}

pub(super) fn face_attachment_inspect_response(
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let command = "face-attachment-inspect";
    let path = |name| args.get_one::<PathBuf>(name).expect("required");
    match val::checks::inspect_face_attachment_file(path("file"), path("spec"), limits) {
        Ok(report) => {
            let mut errors = Vec::new();
            if report.facial_status == "FAIL" {
                errors.push(error(
                    "facial_status",
                    "FAIL",
                    "facial requirements are not playable",
                ));
            }
            errors.extend(report.attachment_results.iter().enumerate().filter_map(
                |(index, part)| {
                    part.violation
                        .map(|detail| error(&format!("rigid_attachments[{index}]"), "FAIL", detail))
                },
            ));
            let (code, status) = if errors.is_empty() {
                (0, "PASS")
            } else {
                (1, "FAIL")
            };
            (
                code,
                envelope(command, call_id, status, json!(errors), json!(report)),
            )
        }
        Err(err) => (
            2,
            envelope(
                command,
                call_id,
                "INVALID_INPUT",
                json!([error(err.field, "INVALID_INPUT", err.detail)]),
                Value::Null,
            ),
        ),
    }
}

pub(super) fn cloth_inspect_response(
    call_id: Option<&String>,
    args: &ArgMatches,
    limits: val::ValidationLimits,
) -> (i32, Value) {
    let command = "cloth-inspect";
    let path = |name| args.get_one::<PathBuf>(name).expect("required");
    match val::checks::inspect_cloth_file(path("file"), path("spec"), limits) {
        Ok(report) => {
            let errors: Vec<_> = report
                .garments
                .iter()
                .enumerate()
                .flat_map(|(index, garment)| {
                    garment.violations.iter().map(move |violation| {
                        let field = format!("garments[{index}]");
                        error(&field, "FAIL", format!("{violation:?}"))
                    })
                })
                .collect();
            let code = i32::from(report.status == "FAIL");
            (
                code,
                envelope(
                    command,
                    call_id,
                    report.status,
                    json!(errors),
                    json!(report),
                ),
            )
        }
        Err(err) => (
            2,
            envelope(
                command,
                call_id,
                "INVALID_INPUT",
                json!([error(err.field, "INVALID_INPUT", err.detail)]),
                Value::Null,
            ),
        ),
    }
}
