//! Shared application lifecycle over real SQLite files; no product schema is assumed.
use assetweft_core::application::Application;
use assetweft_core::validation::{
    RoleRequirement, SourceClass, SourceDeclaration, ValidationError, ValidationLimits,
};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "aw008-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn two_entries_open_validate_close_and_reopen_real_file() {
    let dir = Dir::new();
    let db = dir.0.join("库.db");
    let file = dir.0.join("large.glb");
    fs::write(&file, b"123456789").unwrap();
    let first = Application::open(&db).unwrap();
    let second = Application::open(&db).unwrap();
    assert!(db.is_file());
    assert_eq!(first.connection_config().foreign_keys, 1);
    assert_eq!(second.connection_config().foreign_keys, 1);
    assert_eq!(first.connection_config().busy_timeout_ms, 200);
    for entry in [&first, &second] {
        assert!(matches!(
            entry.validate_file(
                &file,
                RoleRequirement::StaticMesh,
                SourceDeclaration::declared(SourceClass::TestFixture),
                ValidationLimits { max_bytes: 4 }
            ),
            Err(ValidationError::InputTooLarge { .. })
        ));
    }
    first.close().unwrap();
    second.close().unwrap();
    Application::open(&db).unwrap().close().unwrap();
}

#[test]
fn open_rejects_non_file_and_reports_missing_parent() {
    assert!(Application::open(std::path::Path::new(":memory:")).is_err());
    let dir = Dir::new();
    assert!(Application::open(&dir.0.join("missing").join("db.sqlite")).is_err());
}

#[test]
fn reads_continue_during_writer_and_open_fails_bounded_under_exclusive_lock() {
    let dir = Dir::new();
    let db = dir.0.join("concurrency.db");
    let first = Application::open(&db).unwrap();
    let second = Application::open(&db).unwrap();
    let locker = rusqlite::Connection::open(&db).unwrap();
    locker
        .execute_batch(
            "CREATE TABLE fixture (id INTEGER); BEGIN IMMEDIATE; INSERT INTO fixture VALUES (1);",
        )
        .unwrap();
    assert_eq!(
        first.schema_version().unwrap(),
        second.schema_version().unwrap()
    );
    locker.execute_batch("ROLLBACK; BEGIN EXCLUSIVE;").unwrap();
    let start = std::time::Instant::now();
    let err = match Application::open(&db) {
        Ok(_) => panic!("exclusive lock must prevent inspection"),
        Err(e) => e,
    };
    assert_eq!(err.kind(), "BUSY");
    assert!(start.elapsed() < std::time::Duration::from_secs(5));
    locker.execute_batch("ROLLBACK").unwrap();
    Application::open(&db).unwrap().close().unwrap();
    first.close().unwrap();
    second.close().unwrap();
}
