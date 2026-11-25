use assetweft_core::{
    assurance::ValidationLimits,
    motion::interaction::{Failure, Report, assess},
};
use serde_json::{Value, json};
use std::{error::Error, fs};

fn interaction_fixture() -> Value {
    let actor = |id, root_x, anchor_x| {
        let anchors: Vec<_> = (0..8)
            .map(|frame| {
                [
                    if (2..=4).contains(&frame) {
                        anchor_x
                    } else {
                        root_x
                    },
                    0.0,
                    0.0,
                ]
            })
            .collect();
        json!({
            "actor_id": id,
            "timestamps_ms": [0, 40, 80, 120, 160, 200, 240, 280],
            "root_positions_m": vec![[root_x, 0.0, 0.0]; 8],
            "anchor_positions_m": anchors,
        })
    };
    json!({
        "scope": "shared_clock_kinematic",
        "actors": [actor("a", 0.0, 0.0), actor("b", 1.0, 0.02)],
        "interaction": {
            "actor_a": "a",
            "actor_b": "b",
            "contact_window_frames": [2, 4],
            "separation_window_frames": [5, 7],
            "max_contact_distance_m": 0.03,
            "min_separation_distance_m": 0.5,
            "forbidden_interpenetration_m": 0.01,
        },
    })
}

fn run(spec: &Value) -> Result<Report, Box<dyn Error>> {
    let file = tempfile::NamedTempFile::new().unwrap();
    fs::write(file.path(), spec.to_string()).unwrap();
    assess(file.path(), ValidationLimits::default())
}

#[test]
fn verifies_inclusive_windows_and_hashes_semantic_input() {
    let mut spec = interaction_fixture();
    spec["actors"][1]["anchor_positions_m"][2][0] = json!(0.01);
    spec["actors"][1]["anchor_positions_m"][4][0] = json!(0.03);
    spec["actors"][1]["anchor_positions_m"][5][0] = json!(0.5);
    spec["actors"][1]["root_positions_m"][7][0] = json!(0.5);
    let report = run(&spec).unwrap();
    assert!(report.failure.is_none());
    let report = serde_json::to_value(report).unwrap();
    assert_eq!(report["visual_engine_status"], "NOT_RUN");
    spec["actors"][0]["root_positions_m"][0][0] = json!(-0.0);
    spec["actors"].as_array_mut().unwrap().reverse();
    assert_eq!(serde_json::to_value(run(&spec).unwrap()).unwrap(), report);

    for actor in spec["actors"].as_array_mut().unwrap() {
        actor["timestamps_ms"][7] = json!(u64::MAX - 1);
    }
    let first = serde_json::to_value(run(&spec).unwrap()).unwrap();
    for actor in spec["actors"].as_array_mut().unwrap() {
        actor["timestamps_ms"][7] = json!(u64::MAX);
    }
    let second = serde_json::to_value(run(&spec).unwrap()).unwrap();
    assert_ne!(report["interaction_sha256"], first["interaction_sha256"]);
    assert_ne!(first["interaction_sha256"], second["interaction_sha256"]);
}

#[test]
fn checks_every_actor_and_reports_clock_mismatches_before_geometry() {
    let mut spec = interaction_fixture();
    let mut third = spec["actors"][1].clone();
    third["actor_id"] = json!("c");
    spec["actors"].as_array_mut().unwrap().push(third);
    assert!(run(&spec).unwrap().failure.is_none());
    for key in ["timestamps_ms", "root_positions_m", "anchor_positions_m"] {
        spec["actors"][2][key].as_array_mut().unwrap().pop();
    }
    spec["actors"][1]["anchor_positions_m"][4][0] = json!(0.04);
    assert!(matches!(
        run(&spec).unwrap().failure,
        Some(Failure::ClockDesync { actor_id }) if actor_id == "c"
    ));
}

#[test]
fn reports_first_failing_frame_for_each_geometry_rule() {
    for (pointer, value, frame) in [
        ("/actors/1/anchor_positions_m/4/0", 0.04, 4),
        ("/actors/1/anchor_positions_m/2/0", 0.0, 2),
        ("/actors/1/anchor_positions_m/5/0", 0.1, 5),
        ("/actors/1/root_positions_m/7/0", 0.1, 7),
    ] {
        let mut spec = interaction_fixture();
        *spec.pointer_mut(pointer).unwrap() = json!(value);
        let report = serde_json::to_value(run(&spec).unwrap()).unwrap();
        let reason = match frame {
            4 => "ContactMiss",
            2 => "Interpenetration",
            _ => "InsufficientSeparation",
        };
        assert_eq!(report["failure"]["reason"], reason);
        assert_eq!(report["failure"]["frame"], frame);
    }
}

#[test]
fn rejects_structural_and_scope_errors() {
    for (pointer, value) in [
        ("/scope", json!("game_ai")),
        ("/actors/1/actor_id", json!("a")),
        ("/actors/1/timestamps_ms/1", json!(0)),
        ("/actors/1/anchor_positions_m", json!([])),
        ("/interaction/actor_b", json!("absent")),
        ("/interaction/actor_b", json!("a")),
        ("/interaction/contact_window_frames", json!([4, 2])),
        ("/interaction/contact_window_frames", json!([2, 5])),
        ("/interaction/separation_window_frames", json!([5, 8])),
        ("/interaction/forbidden_interpenetration_m", json!(0.04)),
        ("/interaction/min_separation_distance_m", json!(-0.1)),
    ] {
        let mut spec = interaction_fixture();
        *spec.pointer_mut(pointer).unwrap() = value;
        assert!(run(&spec).is_err(), "{pointer}");
    }
    for target in ["", "/actors/0", "/interaction"] {
        let mut spec = interaction_fixture();
        spec.pointer_mut(target).unwrap()["ai_planner_present"] = json!(true);
        assert!(run(&spec).is_err(), "{target}");
    }
}
