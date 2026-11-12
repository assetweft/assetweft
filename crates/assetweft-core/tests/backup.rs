//! AW012 real App/SQLite/CAS gates. All data is synthetic in exclusively owned tempdirs.
use assetweft_core::{
    application::Application,
    backup::{BackupError, Limits, Progress},
    history::{Decision, Feedback},
    library::{AssetVersion, Presence},
    storage::{Collection, StorageError},
};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn sample(id: &str, bytes: &[u8]) -> AssetVersion {
    AssetVersion {
        asset_id: id.into(),
        version_id: id.into(),
        content_hash: hash(bytes),
        input_id: "input".into(),
        source: "synthetic".into(),
        authorization: "UNKNOWN".into(),
        authorized: false,
        license_id: "UNKNOWN".into(),
        usage_scope: "local".into(),
        source_format: "BIN".into(),
        contract_version: "1.0.0".into(),
        presence: Presence::Unknown,
    }
}
fn fixture() -> (tempfile::TempDir, Application) {
    let d = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    fs::create_dir(d.path().join("source")).unwrap();
    let mut a = Application::open(&d.path().join("source/original.sqlite")).unwrap();
    a.initialize_library().unwrap();
    a.commit_object(&sample("managed", b"bytes"), &mut &b"bytes"[..], 5, 100)
        .unwrap();
    (d, a)
}
fn backup(a: &Application, p: &Path) {
    a.backup_library(p, Limits::default(), &mut |_| true)
        .unwrap();
}
fn restore(p: &Path, q: &Path) -> Result<assetweft_core::backup::Manifest, BackupError> {
    Application::restore_library(p, q, Limits::default(), &mut |_| true)
}
#[test]
fn roundtrip_exact_identity_history_metadata_and_bytes() {
    let (d, mut a) = fixture();
    let root = d.path();
    let unknown = sample("unknown", b"not managed");
    a.add_asset_version(&unknown).unwrap();
    let mut missing = sample("missing", b"not managed");
    missing.presence = Presence::Missing;
    a.add_asset_version(&missing).unwrap();
    let original = a
        .append_feedback(&Feedback {
            feedback_id: "opinion".into(),
            asset_id: "managed".into(),
            version_id: "managed".into(),
            decision: Decision::Accept,
            reason: None,
        })
        .unwrap();
    let refs = assetweft_core::library::ComponentReference {
        owner_version_id: "managed".into(),
        role: assetweft_core::library::ComponentRole::Mesh,
        component_version_id: "unknown".into(),
    };
    a.add_component_reference(&refs).unwrap();
    backup(&a, &root.join("backup"));
    let manifest = restore(&root.join("backup"), &root.join("restored")).unwrap();
    assert_eq!(manifest.schema, 6);
    assert_eq!(manifest.objects.len(), 1);
    let mut b = Application::open(&root.join("restored/library.sqlite")).unwrap();
    b.initialize_library().unwrap();
    assert_eq!(b.asset_version("unknown").unwrap(), Some(unknown));
    assert_eq!(b.asset_version("missing").unwrap(), Some(missing));
    assert_eq!(
        b.asset_version("managed").unwrap(),
        a.asset_version("managed").unwrap()
    );
    assert_eq!(b.component_references("managed").unwrap(), vec![refs]);
    assert_eq!(
        b.latest_feedback("managed", Some("managed")).unwrap(),
        Some(original)
    );
    assert!(b.managed_object("unknown").unwrap().is_none());
    assert!(b.managed_object("missing").unwrap().is_none());
    assert!(b.managed_object("managed").unwrap().is_some());
    assert_eq!(
        fs::read(root.join("restored/objects").join(hash(b"bytes"))).unwrap(),
        b"bytes"
    );
    assert_eq!(
        fs::read(root.join("restored/.aw-object-owner")).unwrap(),
        b"library.sqlite"
    );
    assert!(!root.join("restored/.aw-storage.lock").exists());
}
#[test]
fn lease_blocks_actual_collector_and_reference_then_releases() {
    let (d, a) = fixture();
    let root = d.path();
    let db = root.join("source/original.sqlite");
    let mut other = Application::open(&db).unwrap();
    let mut observed = false;
    a.backup_library(&root.join("backup"), Limits::default(), &mut |p| {
        if p == Progress::ObjectsReady {
            observed = true;
            assert!(matches!(
                other.collect_object(&hash(b"bytes")),
                Err(StorageError::Busy)
            ));
            assert!(other.add_asset_version(&sample("late", b"bytes")).is_err());
            // Independent SQL metadata writes can proceed; object IO holds no source write tx.
            other
                .append_feedback(&Feedback {
                    feedback_id: "later".into(),
                    asset_id: "managed".into(),
                    version_id: "managed".into(),
                    decision: Decision::Accept,
                    reason: None,
                })
                .unwrap();
        }
        true
    })
    .unwrap();
    assert!(observed);
    assert_eq!(
        other.collect_object(&hash(b"bytes")).unwrap(),
        Collection::Protected
    );
    other.add_asset_version(&sample("late", b"bytes")).unwrap();
    // Owned fixture-only removal simulates later loss of source references; real collection API follows.
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.execute_batch("PRAGMA foreign_keys=OFF; DROP TRIGGER aw_feedback_delete; DELETE FROM aw_feedback; DELETE FROM aw_managed_versions; DELETE FROM aw_asset_versions;").unwrap();
    other.protect_object(&hash(b"bytes"), false, false).unwrap();
    assert_eq!(
        other.collect_object(&hash(b"bytes")).unwrap(),
        Collection::Removed
    );
    restore(&root.join("backup"), &root.join("restored")).unwrap();
    let mut b = Application::open(&root.join("restored/library.sqlite")).unwrap();
    assert!(b.managed_object("managed").unwrap().is_some());
    assert!(b.latest_feedback("managed", None).unwrap().is_none());
}
#[test]
fn missing_corrupt_cancelled_and_limits_never_publish_completion() {
    for mode in ["missing", "corrupt", "cancel", "limit"] {
        let (d, a) = fixture();
        let object = d.path().join("source/objects").join(hash(b"bytes"));
        if mode == "missing" {
            fs::remove_file(&object).unwrap();
        }
        if mode == "corrupt" {
            fs::write(&object, b"wrong").unwrap();
        }
        let limits = if mode == "limit" {
            Limits {
                max_bytes: 1,
                ..Limits::default()
            }
        } else {
            Limits::default()
        };
        assert!(
            a.backup_library(&d.path().join("backup"), limits, &mut |_| mode != "cancel")
                .is_err()
        );
        assert!(!d.path().join("backup/manifest.json").exists());
        assert!(!d.path().join("source/.aw-storage.lock").exists());
    }
}
#[test]
fn existing_destinations_and_bad_packages_preserve_originals() {
    let (d, a) = fixture();
    let root = d.path();
    backup(&a, &root.join("backup"));
    let source = fs::read(root.join("source/original.sqlite")).unwrap();
    let snapshot = fs::read(root.join("backup/library.sqlite")).unwrap();
    assert!(restore(&root.join("backup"), &root.join("source")).is_err());
    assert!(restore(&root.join("backup"), &root.join("backup")).is_err());
    assert!(
        a.backup_library(&root.join("backup"), Limits::default(), &mut |_| true)
            .is_err()
    );
    assert_eq!(
        fs::read(root.join("source/original.sqlite")).unwrap(),
        source
    );
    assert_eq!(
        fs::read(root.join("backup/library.sqlite")).unwrap(),
        snapshot
    );
    let manifest = fs::read(root.join("backup/manifest.json")).unwrap();
    for schema in [0, 1, 5, 7, 999] {
        let mut m: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
        m["schema"] = schema.into();
        fs::write(
            root.join("backup/manifest.json"),
            serde_json::to_vec(&m).unwrap(),
        )
        .unwrap();
        assert!(restore(&root.join("backup"), &root.join(format!("reject-{schema}"))).is_err());
        assert!(!root.join(format!("reject-{schema}")).exists());
    }
    fs::write(root.join("backup/manifest.json"), manifest).unwrap();
    fs::write(root.join("backup/objects").join(hash(b"bytes")), b"wrong").unwrap();
    assert!(restore(&root.join("backup"), &root.join("bad-object")).is_err());
    assert!(!root.join("bad-object/manifest.json").exists());
    assert_eq!(
        fs::read(root.join("source/original.sqlite")).unwrap(),
        source
    );
    assert_eq!(
        fs::read(root.join("backup/library.sqlite")).unwrap(),
        snapshot
    );
}
#[test]
fn rejects_database_tamper_and_manifest_traversal() {
    let (d, a) = fixture();
    let root = d.path();
    backup(&a, &root.join("backup"));
    let path = root.join("backup/manifest.json");
    let original = fs::read(&path).unwrap();
    let mut m: serde_json::Value = serde_json::from_slice(&original).unwrap();
    m["objects"][0]["hash"] = "../../escape".into();
    fs::write(&path, serde_json::to_vec(&m).unwrap()).unwrap();
    assert!(restore(&root.join("backup"), &root.join("reject")).is_err());
    assert!(!root.join("escape").exists());
    fs::write(path, original).unwrap();
    fs::write(root.join("backup/library.sqlite"), b"corrupt").unwrap();
    assert!(restore(&root.join("backup"), &root.join("reject2")).is_err());
}
#[cfg(unix)]
#[test]
fn symlinks_and_parent_traversal_are_rejected() {
    let (d, a) = fixture();
    let root = d.path();
    backup(&a, &root.join("backup"));
    fs::create_dir(root.join("outside")).unwrap();
    std::os::unix::fs::symlink(root.join("outside"), root.join("alias")).unwrap();
    assert!(restore(&root.join("backup"), &root.join("alias/escape")).is_err());
    assert!(!root.join("outside/escape").exists());
    assert!(restore(&root.join("backup"), &root.join("outside/../escape")).is_err());
    let obj = root.join("backup/objects").join(hash(b"bytes"));
    fs::remove_file(&obj).unwrap();
    fs::write(root.join("outside/sentinel"), b"bytes").unwrap();
    std::os::unix::fs::symlink(root.join("outside/sentinel"), obj).unwrap();
    assert!(restore(&root.join("backup"), &root.join("bad")).is_err());
    assert_eq!(fs::read(root.join("outside/sentinel")).unwrap(), b"bytes");
}
#[test]
fn actual_future_schema_and_busy_source_fail_explicitly() {
    let (d, a) = fixture();
    let root = d.path();
    let conn = rusqlite::Connection::open(root.join("source/original.sqlite")).unwrap();
    conn.execute_batch("UPDATE aw_schema_version SET version=99")
        .unwrap();
    assert!(
        a.backup_library(&root.join("future"), Limits::default(), &mut |_| true)
            .is_err()
    );
    conn.execute_batch("UPDATE aw_schema_version SET version=6; BEGIN EXCLUSIVE")
        .unwrap();
    assert!(
        a.backup_library(&root.join("busy"), Limits::default(), &mut |_| true)
            .is_err()
    );
    conn.execute_batch("ROLLBACK").unwrap();
    backup(&a, &root.join("good"));
}

#[test]
fn live_wal_snapshot_has_latest_committed_data_and_no_sidecar_dependency() {
    let (d, mut a) = fixture();
    let root = d.path();
    let conn = rusqlite::Connection::open(root.join("source/original.sqlite")).unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    conn.execute_batch("PRAGMA wal_autocheckpoint=0").unwrap();
    a.add_asset_version(&sample("wal-row", b"metadata"))
        .unwrap();
    assert!(
        root.join("source/original.sqlite-wal")
            .metadata()
            .unwrap()
            .len()
            > 0
    );
    backup(&a, &root.join("backup"));
    assert!(!root.join("backup/library.sqlite-wal").exists());
    restore(&root.join("backup"), &root.join("restored")).unwrap();
    let mut b = Application::open(&root.join("restored/library.sqlite")).unwrap();
    assert_eq!(
        b.asset_version("wal-row").unwrap(),
        Some(sample("wal-row", b"metadata"))
    );
}

#[test]
fn snapshot_schema_disagrees_with_manifest_rejected_without_upgrade_or_source_write() {
    let (d, a) = fixture();
    let root = d.path();
    backup(&a, &root.join("backup"));
    let source = fs::read(root.join("source/original.sqlite")).unwrap();
    let db = root.join("backup/library.sqlite");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("UPDATE aw_schema_version SET version=5")
        .unwrap();
    drop(conn);
    let snapshot = fs::read(&db).unwrap();
    let path = root.join("backup/manifest.json");
    let mut m: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    m["database"]["hash"] = hash(&snapshot).into();
    fs::write(&path, serde_json::to_vec(&m).unwrap()).unwrap();
    let manifest = fs::read(&path).unwrap();
    assert!(restore(&root.join("backup"), &root.join("reject")).is_err());
    assert!(!root.join("reject").exists());
    assert_eq!(fs::read(db).unwrap(), snapshot);
    assert_eq!(fs::read(path).unwrap(), manifest);
    assert_eq!(
        fs::read(root.join("source/original.sqlite")).unwrap(),
        source
    );
}
