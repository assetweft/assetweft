use assetweft_core::{
    application::Application,
    library::{AssetVersion, Presence},
};
use rusqlite::{Connection, params};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const CHILD_MODE_ENV: &str = "AW004_NATIVE_RECOVERY_CHILD";
const CHILD_DB_ENV: &str = "AW004_NATIVE_RECOVERY_DB";
const CHILD_READY_ENV: &str = "AW004_NATIVE_RECOVERY_READY";
const CHILD_TEST_NAME: &str = "native_recovery_child_process";

const COMMITTED_VERSION: &str = "native-recovery-committed";
const UNCOMMITTED_VERSION: &str = "native-recovery-uncommitted";
const READY_DEADLINE: Duration = Duration::from_secs(30);
const CHILD_IDLE_DEADLINE: Duration = Duration::from_secs(60);

fn sample(asset: &str, version: &str) -> AssetVersion {
    AssetVersion {
        asset_id: asset.into(),
        version_id: version.into(),
        content_hash: "a".repeat(64),
        input_id: "input-native-recovery".into(),
        source: "synthetic native recovery test".into(),
        authorization: "UNKNOWN".into(),
        authorized: false,
        license_id: "UNKNOWN".into(),
        usage_scope: "local".into(),
        source_format: "BIN".into(),
        contract_version: "1.0.0".into(),
        presence: Presence::Unknown,
    }
}

fn temp_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("aw004 原生恢复 ")
        .tempdir_in(std::env::temp_dir())
        .expect("create exclusively owned temp dir")
}

/// SQLite 的 sidecar 路径是数据库路径原样加后缀（`-journal`），不能用 `set_extension`。
fn sidecar(db: &Path, suffix: &str) -> PathBuf {
    let mut raw: OsString = db.as_os_str().to_owned();
    raw.push(suffix);
    PathBuf::from(raw)
}

fn primary_code(err: &rusqlite::Error) -> Option<i32> {
    match err {
        rusqlite::Error::SqliteFailure(code, _) => Some(code.extended_code & 0xff),
        _ => None,
    }
}

struct ChildGuard {
    child: Child,
    logs: PathBuf,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        for name in ["child.stdout", "child.stderr"] {
            eprintln!(
                "{name}: {}",
                fs::read_to_string(self.logs.join(name)).unwrap_or_default()
            );
        }
    }
}

fn ready_marker(pid: u32) -> String {
    format!(
        "pid={pid} journal_mode=delete committed={COMMITTED_VERSION} uncommitted={UNCOMMITTED_VERSION}\n"
    )
}

fn has_complete_marker(ready: &Path, expected: &str) -> bool {
    fs::read_to_string(ready).is_ok_and(|marker| marker == expected)
}

#[test]
fn readiness_rejects_empty_and_partial_publication() {
    let dir = temp_dir();
    let ready = dir.path().join("ready.marker");
    let expected = ready_marker(42);
    for incomplete in ["", "pid=42", expected.trim_end()] {
        fs::write(&ready, incomplete).expect("publish incomplete marker");
        assert!(
            !has_complete_marker(&ready, &expected),
            "incomplete marker accepted"
        );
    }
    fs::write(&ready, &expected).expect("finish publication");
    assert!(has_complete_marker(&ready, &expected));
}

fn wait_for_ready(ready: &Path, child: &mut Child) -> String {
    let started = Instant::now();
    loop {
        if has_complete_marker(ready, &ready_marker(child.id())) {
            return ready_marker(child.id());
        }
        if let Some(status) = child.try_wait().expect("poll child status") {
            panic!("child process exited before readiness handshake: {status}");
        }
        assert!(
            started.elapsed() < READY_DEADLINE,
            "readiness handshake timed out after {READY_DEADLINE:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn native_recovery_child_process() {
    let Some(ready) = std::env::var_os(CHILD_READY_ENV) else {
        return;
    };
    let db = PathBuf::from(std::env::var_os(CHILD_DB_ENV).expect("child db path env"));
    let mut app = Application::open(&db).expect("child: product entry open");
    app.initialize_library()
        .expect("child: initialize schema 1-6");
    app.add_asset_version(&sample("native-recovery-asset", COMMITTED_VERSION))
        .expect("child: committed version through product entry");

    app.close().expect("child: close committed baseline");
    fs::copy(&db, sidecar(&db, ".baseline")).expect("child: retain committed main DB bytes");

    let raw = Connection::open(&db).expect("child: raw connection for uncommitted transaction");
    raw.busy_timeout(Duration::from_millis(200))
        .expect("child: busy timeout");
    raw.pragma_update(None, "cache_size", 5_i64)
        .expect("child: bounded page cache");
    raw.pragma_update(None, "cache_spill", true)
        .expect("child: explicitly enable spill");
    let spill: i64 = raw
        .pragma_query_value(None, "cache_spill", |row| row.get(0))
        .expect("child: read spill threshold");
    assert!(spill > 0, "spill must be enabled");
    println!(
        "AW004_RUNTIME sqlite={} cache_spill={spill}",
        rusqlite::version()
    );
    let journal_mode: String = raw
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .expect("child: read journal_mode");
    raw.execute_batch("BEGIN IMMEDIATE")
        .expect("child: begin immediate transaction");
    raw.execute(
        "INSERT INTO aw_asset_versions(
             version_id, asset_id, content_hash, input_id, source, authorization,
             authorized, license_id, usage_scope, source_format, contract_version, presence
         ) VALUES (?1, 'native-recovery-asset', ?2, 'input-native-recovery',
                   'synthetic uncommitted', 'UNKNOWN', 0, 'UNKNOWN', 'local', 'BIN', '1.0.0', 'UNKNOWN')",
        params![UNCOMMITTED_VERSION, "b".repeat(64)],
    )
    .expect("child: uncommitted version insert");

    let huge_uncommitted_asset = "u".repeat(200_000);
    raw.execute(
        "INSERT INTO aw_assets(asset_id) VALUES (?1)",
        params![huge_uncommitted_asset],
    )
    .expect("child: uncommitted page-spilling insert");

    assert_eq!(journal_mode, "delete");
    let marker = ready_marker(std::process::id());
    fs::write(&ready, marker).expect("child: write readiness marker after real writes");

    let until = Instant::now() + CHILD_IDLE_DEADLINE;
    while Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("parent did not terminate the child within {CHILD_IDLE_DEADLINE:?}");
}

#[test]
fn abrupt_child_exit_rolls_back_uncommitted_and_keeps_committed_through_product_entry() {
    let dir = temp_dir();
    let db = dir.path().join("native crash.sqlite");
    let ready = dir.path().join("ready.marker");
    let journal = sidecar(&db, "-journal");

    let exe = std::env::current_exe().expect("current test executable");
    let child = Command::new(&exe)
        .arg("--exact")
        .arg(CHILD_TEST_NAME)
        .arg("--nocapture")
        .env(CHILD_MODE_ENV, "1")
        .env(CHILD_DB_ENV, &db)
        .env(CHILD_READY_ENV, &ready)
        .stdout(Stdio::from(
            fs::File::create(dir.path().join("child.stdout")).expect("child stdout"),
        ))
        .stderr(Stdio::from(
            fs::File::create(dir.path().join("child.stderr")).expect("child stderr"),
        ))
        .spawn()
        .expect("spawn only this test's own child process");
    let mut guard = ChildGuard {
        child,
        logs: dir.path().to_owned(),
    };

    let marker = wait_for_ready(&ready, &mut guard.child);
    assert!(
        marker.contains(&format!("pid={}", guard.child.id())),
        "readiness marker must come from this test's child: {marker}"
    );
    assert!(
        marker.contains("journal_mode=delete"),
        "product default journal mode must be rollback DELETE for this evidence: {marker}"
    );
    assert!(
        guard.child.try_wait().expect("poll child").is_none(),
        "child must still be alive with its transaction open at kill time"
    );
    assert!(
        journal.exists() && fs::metadata(&journal).expect("journal metadata").len() > 0,
        "abrupt exit must leave a non-empty real rollback journal at {journal:?}"
    );

    guard.child.kill().expect("terminate only own child");
    let status = guard.child.wait().expect("reap own child");
    assert!(
        !status.success(),
        "abrupt termination must not be reported as a clean success: {status}"
    );
    // SQLite lockingv3 §4 + fileformat2 §3 (2026-11-15): >512 bytes,
    // valid nonzero header. Own child is reaped, no other connection/ATTACH remains.
    let baseline = fs::read(sidecar(&db, ".baseline")).expect("committed baseline bytes");
    let crashed = fs::read(&db).expect("crash main DB bytes before ANY SQLite reopen");
    assert_ne!(
        crashed, baseline,
        "spill premise: main DB must differ from committed baseline"
    );
    let bytes = fs::read(&journal).expect("hot journal before recovery");
    assert!(bytes.len() > 512, "hot journal must exceed 512 bytes");
    assert_eq!(
        &bytes[..8],
        &[0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7],
        "valid nonzero journal magic"
    );
    let word = |offset: usize| u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let records = word(8);
    let original_pages = word(16);
    let sector = word(20);
    let page = word(24);
    assert!(records > 0, "journal must contain rollback records");
    assert!(sector.is_power_of_two() && (512..=65536).contains(&sector));
    assert!(page.is_power_of_two() && (512..=65536).contains(&page));
    assert_eq!(
        u64::from(original_pages) * u64::from(page),
        baseline.len() as u64
    );
    assert!(bytes.len() as u64 >= u64::from(sector) + u64::from(page) + 8);
    println!(
        "AW004_HOT_PREMISE journal_bytes={} records={records} sector={sector} page={page} baseline_bytes={} crash_bytes={} main_changed=true",
        bytes.len(),
        baseline.len(),
        crashed.len()
    );

    let mut app = Application::open(&db).expect("product reopen after abrupt exit");
    app.initialize_library()
        .expect("migration entry after crash recovery");
    let committed = app
        .asset_version(COMMITTED_VERSION)
        .expect("read committed version after recovery");
    assert_eq!(
        committed.as_ref().map(|v| v.version_id.as_str()),
        Some(COMMITTED_VERSION),
        "committed data must survive abrupt exit"
    );
    assert!(
        matches!(app.asset_version(UNCOMMITTED_VERSION), Ok(None)),
        "uncommitted version must be rolled back, not visible through product read"
    );
    app.close().expect("close recovered product entry");
    assert!(
        !journal.exists(),
        "recovery must consume and remove the hot rollback journal"
    );

    let verify = Connection::open(&db).expect("independent verification connection");
    let integrity: String = verify
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("integrity_check after recovery");
    assert_eq!(integrity, "ok", "database integrity after crash recovery");
    let schema: i64 = verify
        .query_row("SELECT version FROM aw_schema_version", [], |row| {
            row.get(0)
        })
        .expect("schema version after recovery");
    assert_eq!(
        schema, 6,
        "schema 1-6 chain must stay current after recovery"
    );
    let assets: i64 = verify
        .query_row("SELECT count(*) FROM aw_assets", [], |row| row.get(0))
        .expect("asset row count");
    assert_eq!(
        assets, 1,
        "only the committed asset may remain; uncommitted page-spilling row must roll back"
    );
    let uncommitted_versions: i64 = verify
        .query_row(
            "SELECT count(*) FROM aw_asset_versions WHERE version_id=?1",
            [UNCOMMITTED_VERSION],
            |row| row.get(0),
        )
        .expect("uncommitted version count");
    assert_eq!(uncommitted_versions, 0);

    println!(
        "AW004_NATIVE_RECOVERY_CRASH=PASS journal_bytes_before_recovery=observed committed={COMMITTED_VERSION} uncommitted_rolled_back=true integrity=ok schema=6"
    );
}

#[test]
fn real_sqlite_write_failures_leave_data_and_schema_usable() {
    let dir = temp_dir();
    let db = dir.path().join("write failure.sqlite");
    let mut app = Application::open(&db).expect("product open before injected failures");
    app.initialize_library().expect("initialize schema 1-6");
    app.add_asset_version(&sample("native-recovery-asset", COMMITTED_VERSION))
        .expect("committed baseline version");
    app.close().expect("close before failure injection");

    let readonly = Connection::open(&db).expect("read-only mechanism connection");
    readonly
        .busy_timeout(Duration::from_millis(200))
        .expect("busy timeout");
    readonly
        .pragma_update(None, "query_only", true)
        .expect("enable documented query_only");
    let denied_readonly = "denied-by-query-only";
    let readonly_err = readonly
        .execute(
            "INSERT INTO aw_assets(asset_id) VALUES (?1)",
            [denied_readonly],
        )
        .expect_err("query_only must reject INSERT");
    assert_eq!(
        primary_code(&readonly_err),
        Some(8),
        "expected SQLITE_READONLY from query_only, got: {readonly_err}"
    );
    let still_one: i64 = readonly
        .query_row("SELECT count(*) FROM aw_assets", [], |row| row.get(0))
        .expect("read allowed under query_only");
    assert_eq!(still_one, 1);
    drop(readonly);

    let bounded = Connection::open(&db).expect("max_page_count mechanism connection");
    bounded
        .busy_timeout(Duration::from_millis(200))
        .expect("busy timeout");
    let pages: i64 = bounded
        .query_row("PRAGMA page_count", [], |row| row.get(0))
        .expect("page_count");
    let limit: i64 = bounded
        .pragma_update_and_check(None, "max_page_count", pages, |row| row.get(0))
        .expect("set max_page_count to current page count");
    assert_eq!(limit, pages, "max_page_count cannot go below current size");
    let huge = "f".repeat(256 * 1024);
    let full_err = bounded
        .execute("INSERT INTO aw_assets(asset_id) VALUES (?1)", params![huge])
        .expect_err("max_page_count must refuse growth");
    assert_eq!(
        primary_code(&full_err),
        Some(13),
        "expected SQLITE_FULL from max_page_count, got: {full_err}"
    );
    drop(bounded);

    let mut reopened = Application::open(&db).expect("product reopen after write failures");
    reopened
        .initialize_library()
        .expect("migration entry after write failures");
    assert_eq!(
        reopened
            .asset_version(COMMITTED_VERSION)
            .expect("read committed after failures")
            .as_ref()
            .map(|v| v.version_id.as_str()),
        Some(COMMITTED_VERSION)
    );
    let rejected = Connection::open(&db).expect("verify rejected keys after product reopen");
    for key in [denied_readonly, huge.as_str()] {
        let count: i64 = rejected
            .query_row(
                "SELECT count(*) FROM aw_assets WHERE asset_id=?1",
                [key],
                |row| row.get(0),
            )
            .expect("actual rejected asset key count");
        assert_eq!(
            count, 0,
            "rejected aw_assets key must remain absent after reopen"
        );
    }
    let baseline_rows: i64 = rejected
        .query_row("SELECT count(*) FROM aw_assets", [], |row| row.get(0))
        .expect("baseline asset count after reopen");
    assert_eq!(
        baseline_rows, 1,
        "only baseline asset before successful write"
    );
    drop(rejected);
    reopened
        .add_asset_version(&sample(
            "native-recovery-asset",
            "native-recovery-after-failure",
        ))
        .expect("product write path must remain usable after engine-level failures");
    reopened.close().expect("close after recovery checks");

    let verify = Connection::open(&db).expect("independent verification connection");
    let integrity: String = verify
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("integrity_check after write failures");
    assert_eq!(integrity, "ok");
    let schema: i64 = verify
        .query_row("SELECT version FROM aw_schema_version", [], |row| {
            row.get(0)
        })
        .expect("schema version after write failures");
    assert_eq!(schema, 6);

    println!(
        "AW004_NATIVE_RECOVERY_WRITEFAIL=PASS sqlite_readonly=8 sqlite_full=13 integrity=ok schema=6 data_preserved=true"
    );
}
