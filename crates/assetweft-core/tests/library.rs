//! AW-009 product repository against exclusively owned real disk databases.
use assetweft_core::{
    application::Application,
    library::{AssetVersion, ComponentReference, ComponentRole, LibraryError, Presence},
};
use rusqlite::Connection;
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, Barrier,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Dir(PathBuf);
static DIR_SERIAL: AtomicU64 = AtomicU64::new(0);
impl Dir {
    fn new() -> Self {
        for _ in 0..100 {
            let path = std::env::temp_dir().join(format!(
                "aw009 测试 {} {} {}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                DIR_SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("creating test directory: {e}"),
            }
        }
        panic!("could not create unique test directory")
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn sample(asset: &str, version: &str) -> AssetVersion {
    AssetVersion {
        asset_id: asset.into(),
        version_id: version.into(),
        content_hash: "a".repeat(64),
        input_id: "input-1".into(),
        source: "artist declaration".into(),
        authorization: "user declaration".into(),
        authorized: false,
        license_id: "UNKNOWN".into(),
        usage_scope: "UNKNOWN".into(),
        source_format: "GLB".into(),
        contract_version: "1.0.0".into(),
        presence: Presence::Unknown,
    }
}

#[test]
fn v0_upgrade_preserves_existing_data_and_reopen_round_trips_metadata_and_refs() {
    let d = Dir::new();
    let db = d.0.join("资产 库.sqlite");
    // v0 is a real pre-product schema database, not a fabricated released version.
    let old = Connection::open(&db).unwrap();
    old.execute_batch("CREATE TABLE previous_data (value TEXT NOT NULL); INSERT INTO previous_data VALUES ('preserve');").unwrap();
    drop(old);
    let mut app = Application::open(&db).unwrap();
    assert!(matches!(
        app.asset_version("v1"),
        Err(LibraryError::Uninitialized)
    ));
    app.initialize_library().unwrap();
    app.initialize_library().unwrap();
    let mut first = sample("asset-1", "v1");
    first.presence = Presence::Missing;
    let second = sample("asset-2", "v2");
    app.add_asset_version(&first).unwrap();
    app.add_asset_version(&second).unwrap();
    let reference = ComponentReference {
        owner_version_id: "v1".into(),
        role: ComponentRole::Mesh,
        component_version_id: "v2".into(),
    };
    app.add_component_reference(&reference).unwrap();
    app.close().unwrap();
    let mut reopened = Application::open(&db).unwrap();
    reopened.initialize_library().unwrap();
    assert_eq!(reopened.asset_version("v1").unwrap(), Some(first));
    assert_eq!(reopened.asset_version("v2").unwrap(), Some(second));
    assert_eq!(
        reopened.component_references("v1").unwrap(),
        vec![reference]
    );
    assert_eq!(reopened.asset_version("absent").unwrap(), None);
    reopened.close().unwrap();
    let read = Connection::open(&db).unwrap();
    assert_eq!(
        read.query_row("SELECT value FROM previous_data", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "preserve"
    );
    assert_eq!(
        read.query_row("SELECT version FROM aw_schema_version", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        6
    );
}

#[test]
fn empty_init_fk_invalid_id_hash_and_immutable_versions() {
    let d = Dir::new();
    let db = d.0.join("空 库.sqlite");
    let mut app = Application::open(&db).unwrap();
    app.initialize_library().unwrap();
    let first = sample("asset", "version");
    app.add_asset_version(&first).unwrap();
    let orphan = ComponentReference {
        owner_version_id: "version".into(),
        role: ComponentRole::Rig,
        component_version_id: "missing".into(),
    };
    assert!(matches!(
        app.add_component_reference(&orphan),
        Err(LibraryError::Constraint(_))
    ));
    assert!(app.component_references("version").unwrap().is_empty());
    let mut duplicate = first.clone();
    duplicate.asset_id = "new-asset-on-failure".into();
    duplicate.source = "replacement".into();
    assert!(matches!(
        app.add_asset_version(&duplicate),
        Err(LibraryError::Constraint(_))
    ));
    assert_eq!(app.asset_version("version").unwrap(), Some(first));
    let raw = Connection::open(&db).unwrap();
    assert_eq!(
        raw.query_row(
            "SELECT count(*) FROM aw_assets WHERE asset_id='new-asset-on-failure'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    let mut bad = sample("/absolute/path", "v2");
    assert!(matches!(
        app.add_asset_version(&bad),
        Err(LibraryError::InvalidInput(_))
    ));
    bad.asset_id = "safe".into();
    bad.content_hash = "invalid".into();
    assert!(matches!(
        app.add_asset_version(&bad),
        Err(LibraryError::InvalidInput(_))
    ));
}

#[test]
fn failed_initial_migration_really_rolls_back_ddl_and_preserves_preexisting_data() {
    let d = Dir::new();
    let db = d.0.join("升级 回滚.sqlite");
    let raw = Connection::open(&db).unwrap();
    // Collision occurs after the migration has created both metadata and earlier tables.
    raw.execute_batch("CREATE TABLE preexisting (value TEXT); INSERT INTO preexisting VALUES ('intact'); CREATE TABLE aw_component_references (sentinel TEXT); INSERT INTO aw_component_references VALUES ('intact');").unwrap();
    drop(raw);
    let mut app = Application::open(&db).unwrap();
    assert!(app.initialize_library().is_err());
    app.close().unwrap();
    let raw = Connection::open(&db).unwrap();
    let count: i64 = raw.query_row("SELECT count(*) FROM sqlite_schema WHERE name IN ('aw_schema_version','aw_assets','aw_asset_versions')", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        raw.query_row("SELECT value FROM preexisting", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "intact"
    );
    assert_eq!(
        raw.query_row("SELECT sentinel FROM aw_component_references", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "intact"
    );
}

#[test]
fn future_schema_and_unknown_persisted_values_fail_closed() {
    let d = Dir::new();
    let db = d.0.join("未来.sqlite");
    let mut app = Application::open(&db).unwrap();
    app.initialize_library().unwrap();
    app.add_asset_version(&sample("a", "v")).unwrap();
    let raw = Connection::open(&db).unwrap();
    raw.execute_batch("PRAGMA ignore_check_constraints=ON; UPDATE aw_asset_versions SET presence='FUTURE' WHERE version_id='v';").unwrap();
    assert!(matches!(app.asset_version("v"), Err(LibraryError::UnknownValue(v)) if v == "FUTURE"));
    raw.execute(
        "INSERT INTO aw_component_references VALUES ('v','future-role','v')",
        [],
    )
    .unwrap();
    assert!(
        matches!(app.component_references("v"), Err(LibraryError::UnknownValue(v)) if v == "future-role")
    );
    raw.execute_batch("UPDATE aw_asset_versions SET presence='UNKNOWN', contract_version='2.0.0' WHERE version_id='v';").unwrap();
    assert!(matches!(app.asset_version("v"), Err(LibraryError::UnknownValue(v)) if v == "2.0.0"));
    raw.execute("UPDATE aw_schema_version SET version=7", [])
        .unwrap();
    assert!(matches!(
        app.add_asset_version(&sample("b", "v2")),
        Err(LibraryError::FutureSchema(7))
    ));
    assert!(matches!(
        app.initialize_library(),
        Err(LibraryError::FutureSchema(7))
    ));
    assert!(Application::open(&db).is_err());
    assert_eq!(
        raw.query_row("SELECT version FROM aw_schema_version", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        7
    );
    assert_eq!(
        raw.query_row("SELECT count(*) FROM aw_asset_versions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn two_application_product_writers_observe_bounded_busy_then_recover() {
    let d = Dir::new();
    let db = d.0.join("并发 写入.sqlite");
    let mut a = Application::open(&db).unwrap();
    a.initialize_library().unwrap();
    let b = Application::open(&db).unwrap();
    let locker = Connection::open(&db).unwrap();
    locker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let writers: Vec<_> = [a, b]
        .into_iter()
        .enumerate()
        .map(|(i, mut app)| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                let start = Instant::now();
                let result =
                    app.add_asset_version(&sample(&format!("asset-{i}"), &format!("version-{i}")));
                (app, result, start.elapsed())
            })
        })
        .collect();
    barrier.wait();
    let results: Vec<_> = writers.into_iter().map(|h| h.join().unwrap()).collect();
    for (_, result, elapsed) in &results {
        assert!(matches!(result, Err(LibraryError::Busy(_))), "{result:?}");
        assert!(*elapsed < Duration::from_secs(5));
    }
    locker.execute_batch("ROLLBACK").unwrap();
    for (i, (mut app, _, _)) in results.into_iter().enumerate() {
        app.add_asset_version(&sample(&format!("asset-{i}"), &format!("version-{i}")))
            .unwrap();
    }
    let mut verify = Application::open(&db).unwrap();
    assert!(verify.asset_version("version-0").unwrap().is_some());
    assert!(verify.asset_version("version-1").unwrap().is_some());
}
