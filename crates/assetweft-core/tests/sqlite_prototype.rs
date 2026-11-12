//! 职责: AW-004:A 外部集成测试——真实磁盘文件、中文+空格路径、关闭重开、每连接外键、
//!        约束拒错、两独立连接有限写竞争，以及 runtime/构建选项的机器可读实测行。
//! 依赖: assetweft-core::sqlite、rusqlite 0.42.0、Rust std；无服务进程/管理员/网络调用。
//! 调用方: `cargo test --workspace --locked`（C1 的 test 步骤）与本卡红绿对照实验。
//! 不变量: 每个测试自建并回收自己的临时目录；不用 `:memory:` 冒充文件证据；
//!        断言均为合同/缺陷型，不更新 golden、不弱化断言、不吞真实错误。

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use assetweft_core::sqlite::{
    ProtoError, init_fixture_schema, insert_orphan_child, insert_pair, insert_parent_short_tx,
    open_and_configure, write_with_finite_retry,
};
use rusqlite::TransactionBehavior;

/// 自建临时目录并在 Drop 时只回收自己的目录（不碰历史日志/真实数据）。
struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn temp_dir() -> TempDirGuard {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before unix epoch")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("aw004-资产库 原型-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("create own temp dir");
    TempDirGuard(dir)
}

fn db_path(guard: &TempDirGuard, name: &str) -> PathBuf {
    guard.0.join(name)
}

fn json_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// 机器可读的 runtime 实测行；由跑测脚本抽取为 work/aw004-sqlite/runtime-info.json。
#[test]
fn runtime_facts_machine_line() {
    let guard = temp_dir();
    let path = db_path(&guard, "事实 库.sqlite");
    let (_conn, cfg) = open_and_configure(&path).expect("open for runtime facts");
    let options: Vec<String> = cfg
        .compile_options
        .iter()
        .map(|s| format!("\"{}\"", json_escape(s)))
        .collect();
    let line = format!(
        "{{\"sqlite_version\":\"{}\",\"source_id\":\"{}\",\"journal_mode\":\"{}\",\"foreign_keys\":{},\"busy_timeout_ms\":{},\"compile_options_count\":{},\"compile_options\":[{}]}}",
        json_escape(&cfg.sqlite_version),
        json_escape(&cfg.source_id),
        json_escape(&cfg.journal_mode),
        cfg.foreign_keys,
        cfg.busy_timeout_ms,
        cfg.compile_options.len(),
        options.join(","),
    );
    println!("AW004_RUNTIME_INFO={line}");
}

/// 真实磁盘文件（中文+空格路径）创建/写/关/重开/读；每连接 foreign_keys=1；未启用 WAL。
#[test]
fn chinese_space_path_real_file_roundtrip_and_reopen() {
    let guard = temp_dir();
    let path = db_path(&guard, "库 文件.sqlite");
    assert!(!path.exists(), "file must not pre-exist");
    {
        let (mut conn, cfg) = open_and_configure(&path).expect("open first connection");
        assert_eq!(
            cfg.foreign_keys, 1,
            "foreign_keys must be ON per connection"
        );
        assert!(
            !cfg.journal_mode.eq_ignore_ascii_case("wal"),
            "WAL must not be enabled by this prototype"
        );
        assert!(
            cfg.busy_timeout_ms > 0,
            "busy_timeout must be finite and positive"
        );
        assert!(!cfg.sqlite_version.is_empty());
        assert!(!cfg.compile_options.is_empty());
        assert!(path.exists(), "real disk file must exist");
        init_fixture_schema(&conn).expect("fixture schema");
        insert_pair(&mut conn, 1, 1, "父 记录", "子 记录").expect("short transaction insert");
    } // 第一连接在此关闭
    {
        let (conn2, cfg2) = open_and_configure(&path).expect("reopen same real file");
        assert_eq!(
            cfg2.foreign_keys, 1,
            "reopened connection must configure itself"
        );
        let label: String = conn2
            .query_row("SELECT label FROM parent WHERE id = 1", [], |row| {
                row.get(0)
            })
            .expect("read parent after reopen");
        let note: String = conn2
            .query_row("SELECT note FROM child WHERE id = 1", [], |row| row.get(0))
            .expect("read child after reopen");
        assert_eq!(label, "父 记录");
        assert_eq!(note, "子 记录");
    }
}

/// 缺陷型回归：外键必须真实拒错，且约束错误只能尝试一次、绝不被当作 busy 重试。
#[test]
fn foreign_key_really_rejects_orphan_and_is_not_retried_as_busy() {
    let guard = temp_dir();
    let path = db_path(&guard, "外键 测试.sqlite");
    let (mut conn, cfg) = open_and_configure(&path).expect("open");
    assert_eq!(cfg.foreign_keys, 1);
    init_fixture_schema(&conn).expect("fixture schema");
    insert_pair(&mut conn, 10, 10, "父", "子").expect("valid parent+child pair");

    let mut attempts = 0;
    let err: ProtoError = write_with_finite_retry(|| {
        attempts += 1;
        insert_orphan_child(&mut conn, 11, 999)
    })
    .expect_err("orphan child must be rejected by foreign key");
    assert_eq!(
        err.kind(),
        "CONSTRAINT",
        "constraint error must not be classified as BUSY: {err:?}"
    );
    assert_eq!(attempts, 1, "constraint error must not be retried as busy");
    let orphan_count: i64 = conn
        .query_row("SELECT count(*) FROM child WHERE id = 11", [], |row| {
            row.get(0)
        })
        .expect("count orphan rows");
    assert_eq!(
        orphan_count, 0,
        "failed transaction must leave no partial row"
    );
}

/// 两独立连接对同一真实文件的写竞争：第二连接有限忙等待/退避后以真实 BUSY 错误结束；
/// 锁释放后可写；不吞 IO/约束错误、不无限循环、事务内不等待网络。
#[test]
fn two_connections_finite_busy_then_released_write() {
    let guard = temp_dir();
    let path = db_path(&guard, "竞争 库.sqlite");
    let (mut a, _) = open_and_configure(&path).expect("connection A");
    init_fixture_schema(&a).expect("fixture schema on A");
    let (mut b, _) = open_and_configure(&path).expect("connection B on same real file");

    let tx_a = a
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .expect("A begins IMMEDIATE write transaction");
    tx_a.execute("INSERT INTO parent(id, label) VALUES (1, 'from-A')", [])
        .expect("A holds the write lock");

    let busy_err = write_with_finite_retry(|| insert_parent_short_tx(&mut b, 2, "from-B"))
        .expect_err("B must end with a real BUSY error while A holds the lock");
    assert_eq!(
        busy_err.kind(),
        "BUSY",
        "expected a real BUSY error, got {busy_err:?}"
    );
    assert!(
        !busy_err.message().is_empty(),
        "BUSY error must carry the original SQLite message"
    );
    eprintln!("AW004_BUSY_EVIDENCE={}", busy_err.message());

    tx_a.commit().expect("A commits and releases the lock");

    write_with_finite_retry(|| insert_parent_short_tx(&mut b, 2, "from-B"))
        .expect("B can write after the lock is released");

    let count: i64 = b
        .query_row("SELECT count(*) FROM parent", [], |row| row.get(0))
        .expect("count parent rows after release");
    assert_eq!(count, 2, "both A and B rows must be present");
}
