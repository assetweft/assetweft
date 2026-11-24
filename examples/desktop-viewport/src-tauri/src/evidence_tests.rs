use super::Evidence;
use std::{fs, path::PathBuf};

const PNG_A: &[u8] = b"\x89PNG\r\n\x1a\nfixture A";
const PNG_B: &[u8] = b"\x89PNG\r\n\x1a\nfixture B";

struct RunDir(PathBuf);

impl RunDir {
    fn new(case: &str) -> Self {
        let root = std::env::temp_dir().join(format!("aw-viewport-{}-{case}", std::process::id()));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}

impl Drop for RunDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn delayed_save_cannot_change_newer_pair() {
    let root = RunDir::new("out-of-order");
    let mut evidence = Evidence::default();
    assert!(evidence.begin(&root.0, 1).unwrap());
    assert!(evidence.begin(&root.0, 2).unwrap());
    evidence.admitted = true;

    let observations = r#"{"generation":2,"caseName":"fixture B"}"#;
    assert_eq!(
        evidence.save(&root.0, 2, observations, PNG_B).unwrap(),
        "saved"
    );
    assert_eq!(
        evidence
            .save(&root.0, 1, r#"{"generation":1}"#, PNG_A)
            .unwrap(),
        "stale-rejected"
    );
    assert_eq!(
        evidence.save(&root.0, 2, observations, PNG_A).unwrap(),
        "stale-rejected"
    );
    assert!(!evidence.begin(&root.0, 1).unwrap());

    let pair = root.0.join("evidence");
    assert_eq!(
        fs::read(pair.join("renderer-observations.json")).unwrap(),
        observations.as_bytes()
    );
    assert_eq!(fs::read(pair.join("render.png")).unwrap(), PNG_B);
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
}

#[test]
fn switch_clears_previous_evidence_even_without_a_new_render() {
    let root = RunDir::new("no-render");
    let mut evidence = Evidence::default();
    assert!(evidence.begin(&root.0, 1).unwrap());
    evidence.admitted = true;
    assert_eq!(
        evidence
            .save(&root.0, 1, r#"{"generation":1}"#, PNG_A)
            .unwrap(),
        "saved"
    );
    assert!(evidence.begin(&root.0, 2).unwrap());
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
    assert_eq!(
        evidence
            .save(&root.0, 1, r#"{"generation":1}"#, PNG_A)
            .unwrap(),
        "stale-rejected"
    );
    assert_eq!(
        evidence
            .save(&root.0, 2, r#"{"generation":2}"#, PNG_B)
            .unwrap(),
        "stale-rejected"
    );
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
}
