use assetweft_core::{assurance::ValidationLimits, motion::retarget::assess};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;

fn spec(height: f64) -> Value {
    let rig = |height, upper, lower| {
        json!({
            "height_m": height,
            "limbs": [
                {"name":"foot","upper_len_m":upper,"lower_len_m":lower,"max_extension_ratio":0.8},
                {"name":"hand","upper_len_m":0.3,"lower_len_m":0.25,"max_extension_ratio":1.0}
            ]
        })
    };
    let frame = |slip: f64| {
        json!({
            "root_pos":[-0.0,0,0],
            "targets":[
                {"limb":"hand","hip_pos":[0,0.3,0],"end_effector_pos":[0,0,0]},
                {"limb":"foot","hip_pos":[0,0.4,0],"end_effector_pos":[0,0,0],
                 "foot_contact_pos":[slip,5.0,0]}
            ]
        })
    };
    json!({
        "source_rig":rig(1.75,0.55,0.45),
        "target_rig":rig(height,0.5,0.25),
        "contact":{"foot_marker":"foot","stance_frames":[0,1],"max_slip_m":0.01,"loop_tolerance_m":0.02},
        "frames":[frame(0.0),frame(0.005),frame(0.0)]
    })
}

fn check(input: &Value) -> Value {
    let file = tempfile::NamedTempFile::new().unwrap();
    let bytes = serde_json::to_vec(input).unwrap();
    fs::write(file.path(), &bytes).unwrap();
    let report = assess(file.path(), ValidationLimits { max_bytes: 4096 }).unwrap();
    let report = serde_json::to_value(report).unwrap();
    assert_eq!(
        report["input_sha256"],
        Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    assert_eq!(report["visual_engine_status"], "NOT_RUN");
    report
}

#[test]
fn different_height_and_limb_proportions_pass() {
    for (height, expected_scale) in [(1.4, 0.8), (2.1, 1.2)] {
        let mut input = spec(height);
        input["frames"][1]["targets"]
            .as_array_mut()
            .unwrap()
            .reverse();
        input["frames"][1]["targets"][0]["foot_contact_pos"][1] = json!(777);
        let report = check(&input);
        assert_eq!(report["failed_stage"], Value::Null);
        assert!((report["scale_ratio"].as_f64().unwrap() - expected_scale).abs() < 1e-12);
        assert!((report["max_reach_utilization"].as_f64().unwrap() - 2.0 / 3.0).abs() < 1e-12);
        assert_eq!(report["max_contact_slip_m"], 0.005);
        assert_eq!(report["max_loop_error_m"], 0.0);
        for stage in report["stages"].as_array().unwrap() {
            assert_eq!(stage["status"], "PASS");
        }
    }
}

#[test]
fn first_stage_failure_localizes_and_blocks_later_stages() {
    let names = ["proportion", "ik_reach", "contact_slip", "loop_closure"];
    for (index, pointer, value, frame, limb) in [
        (0, "/target_rig/height_m", json!(0.175), None, None),
        (0, "/target_rig/limbs", json!([]), None, None),
        (
            1,
            "/frames/1/targets/1/end_effector_pos",
            json!([0, -0.21, 0]),
            Some(1),
            Some("foot"),
        ),
        (
            1,
            "/frames/1/targets/1/end_effector_pos",
            json!([0, 0.16, 0]),
            Some(1),
            Some("foot"),
        ),
        (1, "/frames/1/targets", json!([]), Some(1), Some("foot")),
        (
            2,
            "/frames/1/targets/1/foot_contact_pos",
            json!([0, 0, 0.01000000051]),
            Some(1),
            Some("foot"),
        ),
        (
            2,
            "/frames/1/targets/1/foot_contact_pos",
            Value::Null,
            Some(1),
            Some("foot"),
        ),
        (3, "/frames/2/root_pos", json!([0.03, 0, 0]), Some(2), None),
        (3, "/frames/2/root_pos", json!([0, 0.03, 0]), Some(2), None),
        (
            3,
            "/frames/2/targets/0/end_effector_pos",
            json!([0, 0, 0.03]),
            Some(2),
            Some("hand"),
        ),
    ] {
        let mut input = spec(1.4);
        *input.pointer_mut(pointer).unwrap() = value;
        if index < 2 {
            input["frames"][0]["targets"][1]["foot_contact_pos"] = Value::Null;
        }
        let report = check(&input);
        assert_eq!(report["failed_stage"], names[index], "{pointer}");
        for (i, name) in names.iter().enumerate() {
            let stage = &report["stages"][i];
            assert_eq!(&stage["stage"], name);
            let expected = match i.cmp(&index) {
                std::cmp::Ordering::Less => "PASS",
                std::cmp::Ordering::Equal => "FAIL",
                std::cmp::Ordering::Greater => "NOT_RUN",
            };
            assert_eq!(stage["status"], expected);
        }
        assert_eq!(report["stages"][index]["frame"], json!(frame));
        assert_eq!(report["stages"][index]["limb"], json!(limb));
        for (stage, metric) in [
            (1, "max_reach_utilization"),
            (2, "max_contact_slip_m"),
            (3, "max_loop_error_m"),
        ] {
            if stage > index {
                assert_eq!(report[metric], Value::Null);
            }
        }
    }
}

#[test]
fn stance_uses_ties_to_even_and_keeps_raw_distance() {
    let mut input = spec(1.4);
    input["frames"][1]["targets"][1]["foot_contact_pos"] = json!([0.01000000050, 0, 0]);
    let report = check(&input);
    assert_eq!(report["failed_stage"], Value::Null);
    assert_eq!(report["max_contact_slip_m"], 0.01000000050);
    input["contact"]["stance_frames"] = json!([1, 1]);
    assert_eq!(check(&input)["max_contact_slip_m"], 0.0);
}

#[test]
fn rejects_invalid_spec_fields_and_bounds() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut duplicate = spec(1.4);
    duplicate["target_rig"]["limbs"][1]["name"] = json!("foot");
    let mut unknown = spec(1.4);
    unknown["frames"][0]["targets"][0]["unexpected"] = json!(0);
    let mut backwards = spec(1.4);
    backwards["contact"]["stance_frames"] = json!([1, 0]);
    let mut invalid_ratio = spec(1.4);
    invalid_ratio["source_rig"]["limbs"][0]["max_extension_ratio"] = json!(0.4);
    for input in [duplicate, unknown, backwards, invalid_ratio] {
        fs::write(file.path(), input.to_string()).unwrap();
        assert!(assess(file.path(), ValidationLimits { max_bytes: 4096 }).is_err());
    }
    fs::write(file.path(), spec(1.4).to_string()).unwrap();
    assert!(assess(file.path(), ValidationLimits { max_bytes: 16 }).is_err());
}
