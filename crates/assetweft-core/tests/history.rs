//! M1 real-file feedback gates; fixtures are owned tempfile directories, not user assets.
use assetweft_core::{
    application::Application,
    history::{Decision, Feedback, HistoryQuery},
    library::{AssetVersion, LibraryError, Presence},
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn version(asset: &str, id: &str) -> AssetVersion {
    AssetVersion {
        asset_id: asset.into(),
        version_id: id.into(),
        content_hash: hash(b"original"),
        input_id: "input".into(),
        source: "fixture".into(),
        authorization: "UNKNOWN".into(),
        authorized: false,
        license_id: "UNKNOWN".into(),
        usage_scope: "UNKNOWN".into(),
        source_format: "BIN".into(),
        contract_version: "1.0.0".into(),
        presence: Presence::Unknown,
    }
}
fn feedback(id: &str, asset: &str, v: &str, decision: Decision) -> Feedback {
    Feedback {
        feedback_id: id.into(),
        asset_id: asset.into(),
        version_id: v.into(),
        decision,
        reason: None,
    }
}
fn fixture() -> (tempfile::TempDir, std::path::PathBuf, Application) {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("history.sqlite");
    let mut app = Application::open(&db).unwrap();
    app.initialize_library().unwrap();
    for (a, v) in [("a", "v1"), ("a", "v2"), ("b", "v3")] {
        app.add_asset_version(&version(a, v)).unwrap();
    }
    (d, db, app)
}
fn query<'a>(
    asset: &'a str,
    v: Option<&'a str>,
    limit: u16,
    before: Option<i64>,
) -> HistoryQuery<'a> {
    HistoryQuery {
        asset_id: asset,
        version_id: v,
        limit,
        before_sequence: before,
    }
}
#[test]
fn exact_version_binding_and_failed_writes_leave_no_partial_records() {
    let (_d, db, mut app) = fixture();
    for (a, v) in [("a", "missing"), ("a", "v3"), ("missing", "v1")] {
        assert!(matches!(
            app.append_feedback(&feedback("bad", a, v, Decision::Accept)),
            Err(LibraryError::Constraint(_))
        ));
    }
    assert!(
        app.feedback_history(query("a", None, 100, None))
            .unwrap()
            .is_empty()
    );
    let first = app
        .append_feedback(&feedback("id", "a", "v1", Decision::Reject))
        .unwrap();
    assert!(matches!(
        app.append_feedback(&feedback("id", "a", "v2", Decision::Accept)),
        Err(LibraryError::Constraint(_))
    ));
    assert_eq!(app.latest_feedback("a", None).unwrap(), Some(first));
    let raw = Connection::open(db).unwrap();
    assert_eq!(
        raw.query_row("SELECT count(*) FROM aw_feedback", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn history_is_append_only_paginated_and_stable_across_reopen() {
    let (_d, db, mut app) = fixture();
    let first = app
        .append_feedback(&feedback("1", "a", "v1", Decision::Reject))
        .unwrap();
    let second = app
        .append_feedback(&feedback("2", "a", "v2", Decision::Accept))
        .unwrap();
    let third = app
        .append_feedback(&feedback("3", "a", "v1", Decision::Accept))
        .unwrap();
    app.append_feedback(&feedback("4", "b", "v3", Decision::Reject))
        .unwrap();
    assert!(first.sequence < second.sequence && second.sequence < third.sequence);
    assert_eq!(
        app.feedback_history(query("a", None, 2, None)).unwrap(),
        vec![third.clone(), second.clone()]
    );
    let raw = Connection::open(&db).unwrap();
    for sql in [
        "UPDATE aw_feedback SET decision='REJECT'",
        "DELETE FROM aw_feedback",
    ] {
        assert!(raw.execute(sql, []).is_err());
    }
    app.close().unwrap();
    let mut app = Application::open(&db).unwrap();
    assert_eq!(
        app.feedback_history(query("a", None, 100, None)).unwrap(),
        vec![third.clone(), second.clone(), first.clone()]
    );
    app.append_feedback(&feedback("5", "a", "v2", Decision::Reject))
        .unwrap();
    assert_eq!(
        app.feedback_history(query("a", None, 2, Some(second.sequence)))
            .unwrap(),
        vec![first.clone()]
    );
    assert_eq!(
        app.feedback_history(query("a", Some("v1"), 100, None))
            .unwrap(),
        vec![third.clone(), first]
    );
    assert_eq!(app.latest_feedback("a", Some("v1")).unwrap(), Some(third));
    assert_eq!(app.latest_feedback("a", Some("v3")).unwrap(), None);
    assert_eq!(app.latest_feedback("absent", None).unwrap(), None);
}
#[test]
fn bounded_inputs_and_parameterized_text() {
    let (_d, _db, mut app) = fixture();
    for reason in [
        "".into(),
        "  ".into(),
        "x\0y".into(),
        "x".repeat(2049),
        "界".repeat(683),
    ] {
        let mut f = feedback("bad", "a", "v1", Decision::Reject);
        f.reason = Some(reason);
        assert!(matches!(
            app.append_feedback(&f),
            Err(LibraryError::InvalidInput(_))
        ));
    }
    for id in [
        "".into(),
        " ".into(),
        "..".into(),
        "a/b".into(),
        "x\0".into(),
        "x".repeat(257),
    ] {
        assert!(matches!(
            app.append_feedback(&feedback(&id, "a", "v1", Decision::Accept)),
            Err(LibraryError::InvalidInput(_))
        ));
    }
    for (limit, cursor) in [(0, None), (101, None), (1, Some(0)), (1, Some(-1))] {
        assert!(matches!(
            app.feedback_history(query("a", None, limit, cursor)),
            Err(LibraryError::InvalidInput(_))
        ));
    }
    assert!(
        app.feedback_history(query("a", None, 100, None))
            .unwrap()
            .is_empty()
    );
    let mut f = feedback("'; DROP TABLE aw_assets; --", "a", "v1", Decision::Accept);
    f.reason = Some("界".repeat(682) + "ab");
    assert_eq!(app.append_feedback(&f).unwrap().feedback, f);
    assert_eq!(app.latest_feedback("a", None).unwrap().unwrap().feedback, f);
    assert!(app.asset_version("v1").unwrap().is_some());
}
#[test]
fn feedback_preserves_originals_metadata_and_existing_current_pointer() {
    let (d, db, mut app) = fixture();
    let managed = version("a", "managed");
    let original = d.path().join("original.bin");
    std::fs::write(&original, b"original").unwrap();
    app.commit_object(&managed, &mut std::fs::File::open(&original).unwrap(), 8, 8)
        .unwrap();
    let mut missing = version("a", "missing-file");
    missing.presence = Presence::Missing;
    app.add_asset_version(&missing).unwrap();
    let raw = Connection::open(&db).unwrap();
    // Sentinel existing head, inspected directly: this does not claim candidate acceptance.
    raw.execute(
        "INSERT INTO aw_candidate_current VALUES ('a','v1',9,'sentinel')",
        [],
    )
    .unwrap();
    for v in ["v1", "managed", "missing-file"] {
        app.append_feedback(&feedback(v, "a", v, Decision::Accept))
            .unwrap();
    }
    assert_eq!(app.asset_version("managed").unwrap(), Some(managed.clone()));
    assert_eq!(app.asset_version("missing-file").unwrap(), Some(missing));
    assert_eq!(app.asset_version("v1").unwrap(), Some(version("a", "v1")));
    assert!(app.managed_object("managed").unwrap().is_some());
    assert_eq!(app.managed_object("missing-file").unwrap(), None);
    assert_eq!(app.managed_object("v1").unwrap(), None);
    for path in [
        original,
        d.path().join("objects").join(&managed.content_hash),
    ] {
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(bytes, b"original");
        assert_eq!(hash(&bytes), managed.content_hash);
    }
    assert_eq!(
        raw.query_row(
            "SELECT version_id,revision,contract FROM aw_candidate_current WHERE asset_id='a'",
            [],
            |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?
            ))
        )
        .unwrap(),
        ("v1".into(), 9, "sentinel".into())
    );
}
#[test]
fn migration_005_to_006_is_atomic_and_reopen_is_idempotent() {
    for conflict in [false, true] {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("old.sqlite");
        let raw = Connection::open(&db).unwrap();
        for sql in [
            include_str!("../src/library/001_initial.sql"),
            include_str!("../src/library/002_objects.sql"),
            include_str!("../src/library/003_workflow.sql"),
            include_str!("../src/library/004_budget.sql"),
            include_str!("../src/library/005_candidates.sql"),
        ] {
            raw.execute_batch(sql).unwrap();
        }
        raw.execute_batch("CREATE TABLE aw_schema_version(singleton INTEGER PRIMARY KEY, version INTEGER); INSERT INTO aw_schema_version VALUES(1,5); INSERT INTO aw_assets VALUES('legacy');").unwrap();
        if conflict {
            raw.execute_batch("CREATE INDEX aw_feedback_version_order ON aw_assets(asset_id);")
                .unwrap();
        }
        let mut app = Application::open(&db).unwrap();
        let result = app.initialize_library();
        assert_eq!(result.is_err(), conflict);
        assert_eq!(
            raw.query_row("SELECT version FROM aw_schema_version", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            if conflict { 5 } else { 6 }
        );
        assert_eq!(
            raw.query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='aw_feedback'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            if conflict { 0 } else { 1 }
        );
        assert_eq!(
            raw.query_row("SELECT asset_id FROM aw_assets", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "legacy"
        );
        if !conflict {
            app.close().unwrap();
            let mut app = Application::open(&db).unwrap();
            app.initialize_library().unwrap();
        }
        println!(
            "sqlite={} migration005to006 conflict={conflict} result={result:?}",
            rusqlite::version()
        );
    }
}
