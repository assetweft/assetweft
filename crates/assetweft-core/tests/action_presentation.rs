use assetweft_core::assurance::{ValidationLimits, checks::inspect_action_presentation_file};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn spec() -> Value {
    json!({
        "clip_duration_ms": 300,
        "phases": {"charge_ms": [10, 100], "release_ms": [100, 200], "recovery_ms": [200, 280]},
        "bindings": [
            {"id": "charge", "kind": "vfx", "phase": "charge", "trigger_ms": 10, "duration_ms": 90,
             "cancel_cleanup_policy": "stop_and_clear", "event_source": "fixed_timeline", "claims_gameplay_hit": false},
            {"id": "release", "kind": "vfx", "phase": "release", "trigger_ms": 199, "duration_ms": 30,
             "cancel_cleanup_policy": "stop_and_clear", "event_source": "authoritative_hit", "claims_gameplay_hit": true},
            {"id": "tail", "kind": "sfx", "phase": "recovery", "trigger_ms": 200, "duration_ms": 80,
             "cancel_cleanup_policy": "fade_out", "event_source": "fixed_timeline", "claims_gameplay_hit": false}
        ]
    })
}

fn inspect(path: &Path, spec: &Value) -> assetweft_core::assurance::ActionPresentationReport {
    fs::write(path, spec.to_string()).unwrap();
    inspect_action_presentation_file(path, ValidationLimits::default()).unwrap()
}

#[test]
fn synchronized_bindings_have_a_canonical_exact_digest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("action.json");
    let mut input = spec();
    let report = inspect(&path, &input);
    assert!(report.violations.is_empty());
    assert_eq!(report.engine_playback_status, "NOT_RUN");
    let bindings = serde_json::to_string_pretty(&input["bindings"]).unwrap();
    fs::write(&path, format!(r#"{{ "bindings": {bindings}, "phases": {{ "recovery_ms": [200,280], "release_ms": [100,200], "charge_ms": [10,100] }}, "clip_duration_ms": 300 }}"#)).unwrap();
    let formatted = inspect_action_presentation_file(&path, ValidationLimits::default()).unwrap();
    assert_eq!(formatted.presentation_sha256, report.presentation_sha256);

    input["bindings"][0]["duration_ms"] = json!(9_007_199_254_740_992_u64);
    let first = inspect(&path, &input);
    input["bindings"][0]["duration_ms"] = json!(9_007_199_254_740_993_u64);
    let second = inspect(&path, &input);
    assert_ne!(first.presentation_sha256, report.presentation_sha256);
    assert_ne!(first.presentation_sha256, second.presentation_sha256);
}

#[test]
fn rejects_invalid_phase_bounds_and_binding_policies() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("action.json");
    for (pointer, value, expected) in [
        ("/clip_duration_ms", json!(0), "InvalidPhaseBounds"),
        ("/phases/charge_ms", json!([10, 10]), "InvalidPhaseBounds"),
        (
            "/phases/release_ms",
            json!([200, 100]),
            "InvalidPhaseBounds",
        ),
        ("/phases/release_ms/0", json!(99), "InvalidPhaseBounds"),
        ("/phases/release_ms/0", json!(101), "InvalidPhaseBounds"),
        ("/phases/recovery_ms/1", json!(301), "InvalidPhaseBounds"),
        ("/bindings", json!([]), "EmptyBindings"),
        ("/bindings/0/trigger_ms", json!(100), "TriggerOutsidePhase"),
        ("/bindings/0/trigger_ms", json!(9), "TriggerOutsidePhase"),
        (
            "/bindings/0/cancel_cleanup_policy",
            json!("persist_leak"),
            "UncleanCancelCleanup",
        ),
        (
            "/bindings/2/cancel_cleanup_policy",
            json!("persist_leak"),
            "UncleanCancelCleanup",
        ),
        (
            "/bindings/0/claims_gameplay_hit",
            json!(true),
            "HitEventConflation",
        ),
    ] {
        let mut input = spec();
        *input.pointer_mut(pointer).unwrap() = value;
        let report = inspect(&path, &input);
        assert_eq!(report.violations[0].code, expected, "{pointer}");
    }
}

#[test]
fn rejects_malformed_oversized_and_non_strict_input() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("action.json");
    let rejected = || inspect_action_presentation_file(&path, ValidationLimits::default()).is_err();
    assert!(rejected());
    fs::write(&path, b"{").unwrap();
    assert!(rejected());
    for pointer in ["", "/phases", "/bindings/0"] {
        let mut input = spec();
        input.pointer_mut(pointer).unwrap()["authoritative_hit"] = json!(true);
        fs::write(&path, input.to_string()).unwrap();
        assert!(rejected(), "{pointer}");
    }
    for field in ["event_source", "claims_gameplay_hit"] {
        let mut input = spec();
        input["bindings"][0].as_object_mut().unwrap().remove(field);
        fs::write(&path, input.to_string()).unwrap();
        assert!(rejected(), "{field}");
    }
    fs::write(&path, spec().to_string()).unwrap();
    assert!(inspect_action_presentation_file(&path, ValidationLimits { max_bytes: 8 }).is_err());
}
