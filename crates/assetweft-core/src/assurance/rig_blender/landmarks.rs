//! Rigged Simple native receipt gate. Used only by the pinned Assurance profile.
//! References are glTF world metres, derived from the original joint matrices,
//! inverse binds, positions and weights (source vertices 0, 10 and 80).
use serde::Deserialize;
use serde_json::Value;

const EPS: f64 = 1e-5;
type Point = [f64; 3];
const LANDMARKS: [(Point, Point); 3] = [
    (
        [-0.00000004, -4.57507706, 0.99999958],
        [-0.00000004, -4.57507706, 0.99999958],
    ),
    (
        [0.40690891, 0.00000014, 0.27188792],
        [0.30962049, 0.10081572, 0.27188792],
    ),
    (
        [-0.41581905, 4.57507754, -0.17223692],
        [-4.54035296, -0.43704914, -0.17223692],
    ),
];

#[derive(Deserialize)]
struct Samples {
    rest: Vec<Point>,
    positive: Vec<Point>,
    disabled_positive: Vec<Point>,
}

fn close(a: Point, b: Point) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= EPS)
}

pub(super) fn verify(result: &Value) -> Result<(), String> {
    if result["input_sha256"] != super::RIGGED_SIMPLE
        || result["version"] != "5.1.2"
        || result["build_hash"] != "ec6e62d40fa9"
        || result["joint_node"] != 4
        || result["mesh_node"] != 2
        || result["skin"] != 0
        || result["mesh"] != 0
        || result["joint_name"] != "Bone.001"
        || result["axis"] != "world +Z"
        || result["angle_degrees"] != 90
    {
        return Err("unexpected asset, pose or native build identity".into());
    }
    let s = Samples::deserialize(&result["samples"]).map_err(|e| e.to_string())?;
    verify_samples(&s)
}

fn verify_samples(s: &Samples) -> Result<(), String> {
    if [s.rest.len(), s.positive.len(), s.disabled_positive.len()] != [160; 3]
        || !s
            .rest
            .iter()
            .zip(&s.disabled_positive)
            .all(|(&a, &b)| close(a, b))
    {
        return Err("vertex count or disabled-driver control outside tolerance".into());
    }
    for (rest, target) in LANDMARKS {
        let mut found = false;
        for (i, &p) in s.rest.iter().enumerate().filter(|(_, p)| close(**p, rest)) {
            found = true;
            if !close(s.positive[i], target) {
                return Err(format!("bent landmark at {p:?} outside {EPS} metres"));
            }
        }
        if !found {
            return Err(format!("missing rest landmark {rest:?}"));
        }
    }
    Ok(())
}

#[test]
fn target_landmark_rejects_an_absent_bend_with_a_valid_rest_control() {
    let mut rest = vec![LANDMARKS[0].0; 160];
    rest[1] = LANDMARKS[1].0;
    rest[2] = LANDMARKS[2].0;
    let mut samples = Samples {
        disabled_positive: rest.clone(),
        positive: rest.clone(),
        rest,
    };
    assert!(
        verify_samples(&samples)
            .unwrap_err()
            .contains("bent landmark")
    );
    samples.positive[1] = LANDMARKS[1].1;
    samples.positive[2] = LANDMARKS[2].1;
    assert!(verify_samples(&samples).is_ok());
}
