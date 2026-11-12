//! AW010 real-disk C2 fixtures; only tempfile-owned directories are removed.
use assetweft_core::{
    application::Application,
    library::{AssetVersion, ComponentReference, ComponentRole, Presence},
    storage::{Collection, StorageError},
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
};
fn hash(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
}
fn sample(id: &str, b: &[u8]) -> AssetVersion {
    AssetVersion {
        asset_id: format!("asset-{id}"),
        version_id: id.into(),
        content_hash: hash(b),
        input_id: "input".into(),
        source: "author".into(),
        authorization: "declaration".into(),
        authorized: false,
        license_id: format!("license-{id}"),
        usage_scope: "local".into(),
        source_format: "BIN".into(),
        contract_version: "1.0.0".into(),
        presence: Presence::Unknown,
    }
}
fn fixture() -> (tempfile::TempDir, std::path::PathBuf, Application) {
    let d = tempfile::Builder::new()
        .prefix("AW010 中文 空格 ")
        .tempdir()
        .unwrap();
    let db = d.path().join("素材 库.sqlite");
    let mut app = Application::open(&db).unwrap();
    app.initialize_library().unwrap();
    (d, db, app)
}
#[test]
fn exact_bytes_dedup_licenses_remain_per_version_and_metadata_is_not_managed() {
    let (d, db, mut app) = fixture();
    let bytes = b"same exact bytes";
    let source = d.path().join("中文 输入.bin");
    fs::write(&source, bytes).unwrap();
    let first = sample("one", bytes);
    let mut f = fs::File::open(&source).unwrap();
    app.commit_object(&first, &mut f, bytes.len() as u64, 1024)
        .unwrap();
    let second = sample("two", bytes);
    app.commit_object(&second, &mut &bytes[..], bytes.len() as u64, 1024)
        .unwrap();
    app.add_component_reference(&ComponentReference {
        owner_version_id: "one".into(),
        role: ComponentRole::Mesh,
        component_version_id: "two".into(),
    })
    .unwrap();
    let mut external = sample("external", bytes);
    external.presence = Presence::Missing;
    app.add_asset_version(&external).unwrap();
    assert_eq!(app.managed_object("external").unwrap(), None);
    assert_eq!(app.asset_version("one").unwrap(), Some(first.clone()));
    assert_eq!(app.asset_version("two").unwrap(), Some(second));
    assert_eq!(fs::read_dir(d.path().join("objects")).unwrap().count(), 1);
    assert_eq!(
        fs::read(d.path().join("objects").join(&first.content_hash)).unwrap(),
        bytes
    );
    app.protect_object(&first.content_hash, false, false)
        .unwrap();
    assert_eq!(
        app.collect_object(&first.content_hash).unwrap(),
        Collection::Protected
    );
    app.close().unwrap();
    let mut app = Application::open(&db).unwrap();
    assert!(app.managed_object("one").unwrap().is_some());
    fs::remove_file(d.path().join("objects").join(&first.content_hash)).unwrap();
    assert!(matches!(
        app.managed_object("one"),
        Err(StorageError::Io(_))
    ));
}
#[test]
fn collision_does_not_replace_corrupt_existing_object() {
    let (d, _, mut app) = fixture();
    let b = b"new bytes";
    let v = sample("v", b);
    fs::create_dir(d.path().join("objects")).unwrap();
    let target = d.path().join("objects").join(&v.content_hash);
    fs::write(&target, b"original different content").unwrap();
    assert!(matches!(
        app.commit_object(&v, &mut &b[..], b.len() as u64, 100),
        Err(StorageError::Corrupt)
    ));
    assert_eq!(fs::read(target).unwrap(), b"original different content");
    assert_eq!(app.asset_version("v").unwrap(), None);
}
#[test]
fn database_abort_after_publish_rolls_back_all_registration_and_orphan_is_collectable() {
    let (d, db, mut app) = fixture();
    let b = b"published orphan";
    let v = sample("abort", b);
    let raw = Connection::open(&db).unwrap();
    raw.execute_batch("CREATE TRIGGER fail_registration BEFORE INSERT ON aw_managed_versions BEGIN SELECT RAISE(ABORT,'fixture db failure after version insert'); END;").unwrap();
    assert!(matches!(
        app.commit_object(&v, &mut &b[..], b.len() as u64, 100),
        Err(StorageError::Library(_))
    ));
    assert_eq!(app.asset_version("abort").unwrap(), None);
    assert!(d.path().join("objects").join(&v.content_hash).is_file());
    for table in [
        "aw_assets",
        "aw_asset_versions",
        "aw_objects",
        "aw_managed_versions",
    ] {
        assert_eq!(
            raw.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert_eq!(
        app.collect_object(&v.content_hash).unwrap(),
        Collection::Removed
    );
    assert_eq!(
        app.collect_object(&v.content_hash).unwrap(),
        Collection::Absent
    );
}
#[test]
fn backup_and_unsynced_retention_are_actual_constraints_and_unprotected_orphan_can_be_removed() {
    let (d, db, mut app) = fixture();
    let b = b"retained";
    let v = sample("v", b);
    app.commit_object(&v, &mut &b[..], b.len() as u64, 100)
        .unwrap();
    // Fixture simulates later version retirement; production currently exposes no version deletion.
    let raw = Connection::open(&db).unwrap();
    raw.execute_batch("PRAGMA foreign_keys=ON; DELETE FROM aw_managed_versions; DELETE FROM aw_asset_versions; DELETE FROM aw_assets;").unwrap();
    assert_eq!(
        app.collect_object(&v.content_hash).unwrap(),
        Collection::Protected
    ); // default unsynced unique
    app.protect_object(&v.content_hash, true, false).unwrap();
    assert_eq!(
        app.collect_object(&v.content_hash).unwrap(),
        Collection::Protected
    );
    app.protect_object(&v.content_hash, false, true).unwrap();
    assert_eq!(
        app.collect_object(&v.content_hash).unwrap(),
        Collection::Protected
    );
    assert!(d.path().join("objects").join(&v.content_hash).exists());
    app.protect_object(&v.content_hash, false, false).unwrap();
    assert_eq!(
        app.collect_object(&v.content_hash).unwrap(),
        Collection::Removed
    );
    assert_eq!(
        raw.query_row("SELECT count(*) FROM aw_objects", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
struct Cancel;
impl Read for Cancel {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "fixture cancellation",
        ))
    }
}
#[test]
fn input_errors_limits_hash_mismatch_and_busy_leave_no_versions_or_temp_files() {
    let (d, _, mut app) = fixture();
    let b = b"bytes";
    let v = sample("v", b);
    assert!(app.commit_object(&v, &mut &b[..], 5, 4).is_err());
    assert!(app.commit_object(&v, &mut &b[..], 6, 10).is_err());
    assert!(app.commit_object(&v, &mut &b[..], 4, 10).is_err());
    assert!(app.commit_object(&v, &mut &b"wrong"[..], 5, 10).is_err());
    assert!(app.commit_object(&v, &mut Cancel, 5, 10).is_err());
    let lock = d.path().join(".aw-storage.lock");
    fs::write(&lock, b"owner sentinel").unwrap();
    assert!(matches!(
        app.commit_object(&v, &mut &b[..], 5, 10),
        Err(StorageError::Busy)
    ));
    assert_eq!(fs::read(&lock).unwrap(), b"owner sentinel");
    fs::remove_file(lock).unwrap();
    assert_eq!(app.asset_version("v").unwrap(), None);
    assert_eq!(fs::read_dir(d.path().join("objects")).unwrap().count(), 0);
    assert!(app.collect_object("../../outside").is_err());
}
#[test]
fn v1_upgrade_preserves_metadata_and_failed_v2_upgrade_rolls_back() {
    for fail in [false, true] {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("v1.sqlite");
        let raw = Connection::open(&db).unwrap();
        raw.execute_batch("CREATE TABLE aw_schema_version(singleton INTEGER PRIMARY KEY,version INTEGER NOT NULL); INSERT INTO aw_schema_version VALUES(1,1);").unwrap();
        raw.execute_batch(include_str!("../src/library/001_initial.sql"))
            .unwrap();
        raw.execute_batch("INSERT INTO aw_assets VALUES ('old'); INSERT INTO aw_asset_versions VALUES ('old','old','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','input','source','auth',0,'license','local','BIN','1.0.0','MISSING');").unwrap();
        if fail {
            raw.execute_batch("CREATE TABLE aw_managed_versions(sentinel TEXT); INSERT INTO aw_managed_versions VALUES('preserve');").unwrap();
        }
        let mut app = Application::open(&db).unwrap();
        let result = app.initialize_library();
        assert_eq!(result.is_err(), fail);
        assert_eq!(
            raw.query_row("SELECT version FROM aw_schema_version", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            if fail { 1 } else { 6 }
        );
        assert_eq!(
            raw.query_row("SELECT license_id FROM aw_asset_versions", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "license"
        );
        if fail {
            assert_eq!(
                raw.query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE name='aw_objects'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
            assert_eq!(
                raw.query_row("SELECT sentinel FROM aw_managed_versions", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "preserve"
            );
        } else {
            assert_eq!(app.managed_object("old").unwrap(), None);
            app.initialize_library().unwrap();
        }
    }
}
#[cfg(unix)]
#[test]
fn symlink_directory_and_object_rejected_without_touching_external_files() {
    use std::os::unix::fs::symlink;
    let (d, _, mut app) = fixture();
    let external = tempfile::tempdir().unwrap();
    let v = sample("v", b"abc");
    symlink(external.path(), d.path().join("objects")).unwrap();
    assert!(app.commit_object(&v, &mut &b"abc"[..], 3, 100).is_err());
    assert_eq!(fs::read_dir(external.path()).unwrap().count(), 0);
    fs::remove_file(d.path().join("objects")).unwrap();
    fs::create_dir(d.path().join("objects")).unwrap();
    let file = external.path().join("sentinel");
    fs::write(&file, b"abc").unwrap();
    symlink(&file, d.path().join("objects").join(&v.content_hash)).unwrap();
    assert!(app.commit_object(&v, &mut &b"abc"[..], 3, 100).is_err());
    assert!(app.collect_object(&v.content_hash).is_err());
    assert_eq!(fs::read(file).unwrap(), b"abc");
}

#[test]
fn different_database_in_same_directory_cannot_collect_another_librarys_objects() {
    let (d, _, mut first) = fixture();
    let b = b"shared directory";
    let v = sample("v", b);
    first
        .commit_object(&v, &mut &b[..], b.len() as u64, 100)
        .unwrap();
    let mut second = Application::open(&d.path().join("other.sqlite")).unwrap();
    second.initialize_library().unwrap();
    assert!(matches!(
        second.collect_object(&v.content_hash),
        Err(StorageError::Invalid(_))
    ));
    assert!(first.managed_object("v").unwrap().is_some());
}

#[test]
fn metadata_reference_obeys_collection_lease_and_preserves_typed_errors() {
    use assetweft_core::library::LibraryError;
    use std::fs::OpenOptions;
    let (d, _db, mut writer) = fixture();
    let bytes = b"orphan awaiting collection";
    let v = sample("new-reference", bytes);
    fs::create_dir(d.path().join("objects")).unwrap();
    let object = d.path().join("objects").join(&v.content_hash);
    fs::write(&object, bytes).unwrap();

    // Hold the exact exclusive file lease that collect keeps across SQLite commit
    // and unlink. No SQLite lock is held: this deterministically targets the gap
    // from P1-AW010-01, rather than getting Busy from unrelated SQL contention.
    let lock_path = d.path().join(".aw-storage.lock");
    let lease = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .unwrap();
    let result = writer.add_asset_version(&v);
    assert!(
        matches!(result, Err(LibraryError::Busy(_))),
        "metadata inserted during collector lease: {result:?}"
    );
    assert_eq!(writer.asset_version(&v.version_id).unwrap(), None);
    assert!(object.exists());
    assert!(
        lock_path.exists(),
        "failed writer must not remove collector lease"
    );
    drop(lease);
    fs::remove_file(&lock_path).unwrap();

    // Once the collector lease is released, ordinary metadata still inserts and
    // its hash protects an existing unregistered object against actual collect.
    writer.add_asset_version(&v).unwrap();
    assert_eq!(
        writer.collect_object(&v.content_hash).unwrap(),
        Collection::Protected
    );
    assert!(object.exists());
    assert_eq!(writer.managed_object(&v.version_id).unwrap(), None);
    assert!(matches!(
        writer.add_asset_version(&v),
        Err(LibraryError::Constraint(_))
    ));
    assert!(!lock_path.exists(), "SQL failure must release writer lease");

    // Metadata-only insertion still permits absent bytes; it never claims managed.
    let absent = sample("absent", b"bytes are not stored");
    writer.add_asset_version(&absent).unwrap();
    assert_eq!(writer.managed_object("absent").unwrap(), None);
    assert!(!d.path().join("objects").join(&absent.content_hash).exists());
}
