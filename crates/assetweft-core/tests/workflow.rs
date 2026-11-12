//! Persistent local workflow behavior through the product API.
use assetweft_core::{
    application::Application,
    workflow::{Outcome, State, Step},
};
use tempfile::tempdir;

fn step(id: &str, deps: &[&str]) -> Step {
    Step {
        id: id.into(),
        dependencies: deps.iter().map(|x| (*x).into()).collect(),
    }
}
fn setup(path: &std::path::Path) -> Application {
    let mut a = Application::open(path).unwrap();
    a.initialize_library().unwrap();
    a
}
#[test]
fn failed_dependency_never_invokes_downstream() {
    let dir = tempdir().unwrap();
    let mut app = setup(&dir.path().join("db"));
    app.create_workflow("r", &[step("a", &[]), step("b", &["a"])])
        .unwrap();
    let mut calls = Vec::new();
    app.run_workflow_step("r", |id| {
        calls.push(id.to_owned());
        Outcome::Failure {
            raw: "error".into(),
            evidence: None,
        }
    })
    .unwrap();
    assert!(
        !app.run_workflow_step("r", |id| {
            calls.push(id.to_owned());
            Outcome::Success {
                raw: "bad".into(),
                evidence: None,
            }
        })
        .unwrap()
    );
    assert_eq!(calls, vec!["a"]);
    let s = app.workflow_steps("r").unwrap();
    assert_eq!(s[0].state, State::Failed);
    assert_eq!(
        s[0].outcome,
        Some(Outcome::Failure {
            raw: "error".into(),
            evidence: None
        })
    );
    assert_eq!(s[1].state, State::Pending);
}
#[test]
fn reopen_pending_and_interrupted_are_deterministic() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("db");
    let mut app = setup(&path);
    app.create_workflow("r", &[step("a", &[]), step("b", &["a"])])
        .unwrap();
    assert_eq!(app.claim_workflow_step("r").unwrap().as_deref(), Some("a"));
    app.close().unwrap();
    let mut app = Application::open(&path).unwrap();
    let s = app.workflow_steps("r").unwrap();
    assert_eq!(s[0].state, State::Running);
    assert_eq!(s[1].state, State::Pending);
    assert_eq!(app.claim_workflow_step("r").unwrap(), None);
    app.finish_workflow_step(
        "r",
        "a",
        &Outcome::Success {
            raw: "reconciled".into(),
            evidence: Some("evidence:1".into()),
        },
    )
    .unwrap();
    assert_eq!(app.claim_workflow_step("r").unwrap().as_deref(), Some("b"));
}
#[test]
fn cancel_preserves_result_and_does_not_call_driver() {
    let dir = tempdir().unwrap();
    let mut app = setup(&dir.path().join("db"));
    app.create_workflow("r", &[step("a", &[]), step("b", &["a"])])
        .unwrap();
    assert!(
        app.run_workflow_step("r", |_| Outcome::Success {
            raw: "bytes produced".into(),
            evidence: Some("artifact:1".into())
        })
        .unwrap()
    );
    app.cancel_workflow("r").unwrap();
    assert!(app.workflow_cancelled("r").unwrap());
    let mut calls = 0;
    assert!(
        !app.run_workflow_step("r", |_| {
            calls += 1;
            Outcome::Success {
                raw: String::new(),
                evidence: None,
            }
        })
        .unwrap()
    );
    assert_eq!(calls, 0);
    let s = app.workflow_steps("r").unwrap();
    assert_eq!(
        s[0].outcome,
        Some(Outcome::Success {
            raw: "bytes produced".into(),
            evidence: Some("artifact:1".into())
        })
    );
}

#[test]
fn success_chain_reopens_with_raw_outcomes() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("db");
    let mut app = setup(&path);
    app.create_workflow("r", &[step("a", &[]), step("b", &["a"])])
        .unwrap();
    for expected in ["a", "b"] {
        assert!(
            app.run_workflow_step("r", |id| {
                assert_eq!(id, expected);
                Outcome::Success {
                    raw: format!("raw:{id}"),
                    evidence: Some(format!("file:{id}")),
                }
            })
            .unwrap()
        );
    }
    app.close().unwrap();
    let app = Application::open(&path).unwrap();
    let steps = app.workflow_steps("r").unwrap();
    assert_eq!(
        steps[1].outcome,
        Some(Outcome::Success {
            raw: "raw:b".into(),
            evidence: Some("file:b".into())
        })
    );
}
#[test]
fn invalid_graphs_are_rejected_before_persistence() {
    let dir = tempdir().unwrap();
    let mut app = setup(&dir.path().join("db"));
    assert!(
        app.create_workflow("cycle", &[step("a", &["b"]), step("b", &["a"])])
            .unwrap_err()
            .to_string()
            .contains("cycle")
    );
    assert!(
        app.create_workflow("unknown", &[step("a", &["missing"])])
            .unwrap_err()
            .to_string()
            .contains("missing")
    );
    assert!(app.workflow_steps("cycle").is_err());
    assert!(app.workflow_steps("unknown").is_err());
}
#[test]
fn independent_connections_cannot_claim_same_step() {
    use std::sync::{Arc, Barrier};
    let dir = tempdir().unwrap();
    let path = dir.path().join("db");
    let mut app = setup(&path);
    app.create_workflow("r", &[step("a", &[])]).unwrap();
    app.close().unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let mut threads = vec![];
    for _ in 0..2 {
        let p = path.clone();
        let b = barrier.clone();
        threads.push(std::thread::spawn(move || {
            let mut app = Application::open(&p).unwrap();
            b.wait();
            app.claim_workflow_step("r").unwrap()
        }));
    }
    let claims: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(
        claims.iter().filter(|c| c.as_deref() == Some("a")).count(),
        1
    );
    assert_eq!(claims.iter().filter(|c| c.is_none()).count(), 1);
}
#[test]
fn failed_v3_migration_rolls_back_and_retains_v2() {
    use rusqlite::Connection;
    let dir = tempdir().unwrap();
    let path = dir.path().join("db");
    let raw = Connection::open(&path).unwrap();
    raw.execute_batch("CREATE TABLE aw_schema_version(singleton INTEGER PRIMARY KEY, version INTEGER NOT NULL); INSERT INTO aw_schema_version VALUES(1,2); CREATE TABLE aw_workflow_steps(sentinel TEXT); INSERT INTO aw_workflow_steps VALUES('keep');").unwrap();
    let mut app = Application::open(&path).unwrap();
    assert!(app.initialize_library().is_err());
    assert_eq!(
        raw.query_row("SELECT version FROM aw_schema_version", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        raw.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name='aw_workflow_runs'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        raw.query_row("SELECT sentinel FROM aw_workflow_steps", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "keep"
    );
}

#[test]
fn pending_survives_reopen_and_remains_claimable() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("db");
    let mut app = setup(&path);
    app.create_workflow("r", &[step("a", &[])]).unwrap();
    app.close().unwrap();
    let mut app = Application::open(&path).unwrap();
    assert_eq!(app.workflow_steps("r").unwrap()[0].state, State::Pending);
    assert_eq!(app.claim_workflow_step("r").unwrap().as_deref(), Some("a"));
}
