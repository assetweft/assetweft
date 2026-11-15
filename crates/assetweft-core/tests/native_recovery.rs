//! 职责: AW-004:B 原生恢复 gate——真实子进程被强制终止后，真实磁盘 rollback journal 的
//!        hot-journal 回滚；以及真实 SQLite 写入失败（SQLITE_READONLY / SQLITE_FULL）后，
//!        既有数据与 schema 1–6 仍可通过产品入口使用。
//! 依赖: assetweft-core::application::Application、library 词表、rusqlite 0.42.0（既有依赖）、
//!        Rust std 与既有 dev-dependency tempfile；不新增依赖。
//! 调用方: `.github/workflows/native-recovery.yml`（windows-latest / macos-latest）与本地定向运行；
//!        亦经 workspace C1 在 Ubuntu 执行。
//! 不变量: 只终止本测试自己 spawn 的子进程，只操作本测试独占临时目录里的 scratch 数据库；
//!        同步使用子进程真实写入后的就绪标记 + 截止时间，不用纯 sleep 猜测状态；
//!        不声称模拟物理断电/掉电，不写产品源码或 schema，不弱化断言，失败保留原始输出。

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

/// 子进程模式标记；父进程设置后，被按名称 `--exact` 重新执行的测试才进入写入/等待分支。
const CHILD_MODE_ENV: &str = "AW004_NATIVE_RECOVERY_CHILD";
const CHILD_DB_ENV: &str = "AW004_NATIVE_RECOVERY_DB";
const CHILD_READY_ENV: &str = "AW004_NATIVE_RECOVERY_READY";
/// 集成测试二进制里的子进程测试名；父进程用 `--exact` 只执行它。
const CHILD_TEST_NAME: &str = "native_recovery_child_process";

const COMMITTED_VERSION: &str = "native-recovery-committed";
const UNCOMMITTED_VERSION: &str = "native-recovery-uncommitted";
/// 父进程等待子进程完成真实写入并发就绪标记的上限。
const READY_DEADLINE: Duration = Duration::from_secs(30);
/// 子进程未在期限内被父进程终止时自行失败，避免挂死 CI。
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

/// 只负责自己 spawn 的子进程：任何提前 return/panic 都先终止并回收它。
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

/// 等待子进程在完成真实写入后写出的就绪标记；按截止时间失败，不靠固定 sleep 猜测。
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

/// 子进程：通过产品入口初始化并提交一条版本，再开一个真实 IMMEDIATE 事务写入未提交数据，
/// 然后写就绪标记并保持事务打开，等待父进程强制终止这个进程。
#[test]
fn native_recovery_child_process() {
    let Some(ready) = std::env::var_os(CHILD_READY_ENV) else {
        // 正常 `cargo test` 全量执行时直接返回；只有父进程用环境变量 + `--exact` 才进入子进程分支。
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

    // 未提交事务走同一个真实磁盘文件；缩小缓存迫使页面落盘，让 -journal 成为真实的 hot journal 候选。
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

    // 足够大的未提交行会越过 5 页缓存并写脏主库页面，恢复必须依赖真实 journal 回滚。
    let huge_uncommitted_asset = "u".repeat(200_000);
    raw.execute(
        "INSERT INTO aw_assets(asset_id) VALUES (?1)",
        params![huge_uncommitted_asset],
    )
    .expect("child: uncommitted page-spilling insert");

    assert_eq!(journal_mode, "delete");
    let marker = ready_marker(std::process::id());
    fs::write(&ready, marker).expect("child: write readiness marker after real writes");

    // 事务与连接保持打开，等待父进程 kill；若父进程未按时终止则显式失败而不是挂死。
    let until = Instant::now() + CHILD_IDLE_DEADLINE;
    while Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("parent did not terminate the child within {CHILD_IDLE_DEADLINE:?}");
}

/// 真实异常退出：父进程终止自己创建的子进程后，已提交版本保留、未提交事务回滚、
/// hot journal 被消费，数据库经产品入口重新打开且完整性/schema 仍可用。
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

    // 产品真实入口：打开即触发 hot-journal 回滚，再走迁移入口确认 schema 1-6 当前。
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

/// 真实 SQLite 写入失败：`PRAGMA query_only` 的 SQLITE_READONLY 与 `PRAGMA max_page_count`
/// 的 SQLITE_FULL 都由引擎真实拒绝写入；随后原数据/schema 仍经产品入口可读且可继续写。
#[test]
fn real_sqlite_write_failures_leave_data_and_schema_usable() {
    let dir = temp_dir();
    let db = dir.path().join("write failure.sqlite");
    let mut app = Application::open(&db).expect("product open before injected failures");
    app.initialize_library().expect("initialize schema 1-6");
    app.add_asset_version(&sample("native-recovery-asset", COMMITTED_VERSION))
        .expect("committed baseline version");
    app.close().expect("close before failure injection");

    // 机制 1：query_only 连接上的写入必须由 SQLite 返回 SQLITE_READONLY(8)，不是测试伪造的错误。
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

    // 机制 2：把连接逻辑页上限压到当前大小，再写超出页数的数据，引擎必须返回 SQLITE_FULL(13)。
    // 该 pragma 不填充真实磁盘；只限制本连接可增长的页面数（sqlite.org/pragma.html#pragma_max_page_count）。
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

    // 失败后：原数据/schema 经产品入口可读，且真实写入路径恢复可用。
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
    // Check the actual rejected aw_assets keys AFTER reopen, BEFORE any successful write.
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
