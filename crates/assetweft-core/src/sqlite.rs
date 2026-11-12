//! 职责: AW-004:A 本地 SQLite 文件持久化原型——打开/配置真实磁盘文件、短事务、外键约束、
//!        两独立连接的有限写竞争与错误分类。只覆盖文件持久化，不含同步平台/对象 GC/21 表。
//! 依赖: rusqlite 0.42.0（`bundled` → libsqlite3-sys 0.40.0 → SQLite 3.54.1，MIT）；Rust std。
//!        crate 版本/许可/MSRV 经 crates.io 官方 API 核对；SQLite 修复状态经 sqlite.org 官方
//!        changes.html 与 wal.html#walresetbug 核对（2026-11-12）。
//! 调用方: crates/assetweft-core/tests/sqlite_prototype.rs（外部集成测试）与将来 CLI/验证入口；
//!        C1 通过 `cargo test --workspace --locked` 间接调用。
//! 不变量: 每个连接显式启用并读回验证 `foreign_keys=1`；`busy_timeout` 有限(200ms)；
//!        只有 SQLITE_BUSY/LOCKED 才有限重试(上限 3 次、固定本地退避)，约束/IO/其它错误立即
//!        原样返回，绝不吞成 busy；事务内不等待网络/推理/渲染；只用真实磁盘文件（含中文+空格路径），
//!        不用纯内存库冒充文件证据；不启用 WAL（无依据声称必须，且 WAL 是官方 WAL-reset 竞态的
//!        触发条件）；不建服务进程、不需管理员/sudo；未知费用 unknown，不填 0。

use std::fmt;
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, TransactionBehavior, params};

/// 单连接 SQLite 级有限忙等待（`sqlite3_busy_timeout`），不是无限循环。
pub const BUSY_TIMEOUT: Duration = Duration::from_millis(200);
/// 应用级有限重试上限；只有 `Busy` 才进入下一次尝试。
pub const MAX_BUSY_ATTEMPTS: u32 = 3;
/// 两次尝试之间的本地固定退避；不涉及网络等待。
pub const BUSY_BACKOFF: Duration = Duration::from_millis(25);

/// 每连接必要配置的**实测读回值**（不是我们声明的值）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionConfig {
    pub sqlite_version: String,
    pub source_id: String,
    pub compile_options: Vec<String>,
    pub journal_mode: String,
    pub foreign_keys: i64,
    pub busy_timeout_ms: i64,
}

/// 错误分类：`Busy` 是唯一允许有限重试的类别；`Constraint`/`Io`/`Other` 一律直接返回。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtoError {
    Busy(String),
    Constraint(String),
    Io(String),
    Other(String),
}

impl ProtoError {
    /// 稳定的类别名，供测试与报告做失败匹配。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Busy(_) => "BUSY",
            Self::Constraint(_) => "CONSTRAINT",
            Self::Io(_) => "IO",
            Self::Other(_) => "OTHER",
        }
    }

    /// 原始 SQLite/rusqlite 错误信息（不改写、不吞掉）。
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::Busy(m) | Self::Constraint(m) | Self::Io(m) | Self::Other(m) => m,
        }
    }
}

impl fmt::Display for ProtoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind(), self.message())
    }
}

impl std::error::Error for ProtoError {}

/// 按 SQLite 主结果码分类；用 `extended_code & 0xff`，避免依赖枚举变体名漂移。
/// 5=SQLITE_BUSY, 6=SQLITE_LOCKED, 19=SQLITE_CONSTRAINT, 10=SQLITE_IOERR,
/// 13=SQLITE_FULL, 14=SQLITE_CANTOPEN。
fn classify(err: rusqlite::Error) -> ProtoError {
    let message = err.to_string();
    if let rusqlite::Error::SqliteFailure(ffi_err, _) = &err {
        match ffi_err.extended_code & 0xff {
            5 | 6 => return ProtoError::Busy(message),
            19 => return ProtoError::Constraint(message),
            10 | 13 | 14 => return ProtoError::Io(message),
            _ => {}
        }
    }
    ProtoError::Other(message)
}

/// 读回 SQLite runtime 版本、source id、构建选项、journal_mode 与两个必要配置实测值。
fn read_config(conn: &Connection) -> Result<ConnectionConfig, ProtoError> {
    let sqlite_version: String = conn
        .query_row("SELECT sqlite_version()", [], |row| row.get(0))
        .map_err(classify)?;
    let source_id: String = conn
        .query_row("SELECT sqlite_source_id()", [], |row| row.get(0))
        .map_err(classify)?;
    let compile_options = {
        let mut stmt = conn.prepare("PRAGMA compile_options").map_err(classify)?;
        stmt.query_map([], |row| row.get::<_, String>(0))
            .map_err(classify)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(classify)?
    };
    let journal_mode: String = conn
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .map_err(classify)?;
    let foreign_keys: i64 = conn
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .map_err(classify)?;
    let busy_timeout_ms: i64 = conn
        .pragma_query_value(None, "busy_timeout", |row| row.get(0))
        .map_err(classify)?;
    Ok(ConnectionConfig {
        sqlite_version,
        source_id,
        compile_options,
        journal_mode,
        foreign_keys,
        busy_timeout_ms,
    })
}

/// 打开**真实磁盘文件**并逐连接配置；返回读回的实测配置。
/// 不启用 WAL：本原型没有依据要求 WAL，且 WAL 是官方 WAL-reset 竞态的触发条件。
///
/// # Errors
/// 打开/配置失败，或 `foreign_keys` 读回不为 1 时返回对应分类错误。
pub fn open_and_configure(path: &Path) -> Result<(Connection, ConnectionConfig), ProtoError> {
    let conn = Connection::open(path).map_err(classify)?;
    conn.busy_timeout(BUSY_TIMEOUT).map_err(classify)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(classify)?;
    let config = read_config(&conn)?;
    if config.foreign_keys != 1 {
        return Err(ProtoError::Other(
            "foreign_keys did not read back as 1 after configure".to_owned(),
        ));
    }
    Ok((conn, config))
}

/// 只为实测外键/短事务准备的最小 fixture schema（两张表），不预建 21 表。
///
/// # Errors
/// DDL 失败时返回分类错误。
pub fn init_fixture_schema(conn: &Connection) -> Result<(), ProtoError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS parent (id INTEGER PRIMARY KEY, label TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS child (
             id INTEGER PRIMARY KEY,
             parent_id INTEGER NOT NULL REFERENCES parent(id),
             note TEXT NOT NULL
         );",
    )
    .map_err(classify)
}

/// 一次短事务写入父+子；失败由 `Transaction` 的 Drop 回滚，不吞错误。
///
/// # Errors
/// 事务/插入/提交任一失败即返回分类错误。
pub fn insert_pair(
    conn: &mut Connection,
    parent_id: i64,
    child_id: i64,
    label: &str,
    note: &str,
) -> Result<(), ProtoError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(classify)?;
    tx.execute(
        "INSERT INTO parent(id, label) VALUES (?1, ?2)",
        params![parent_id, label],
    )
    .map_err(classify)?;
    tx.execute(
        "INSERT INTO child(id, parent_id, note) VALUES (?1, ?2, ?3)",
        params![child_id, parent_id, note],
    )
    .map_err(classify)?;
    tx.commit().map_err(classify)
}

/// 在指定连接上执行一次短事务写父行（用于两连接写竞争实测）。
///
/// # Errors
/// 事务/插入/提交任一失败即返回分类错误；持锁冲突会得到 `ProtoError::Busy`。
pub fn insert_parent_short_tx(
    conn: &mut Connection,
    id: i64,
    label: &str,
) -> Result<(), ProtoError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(classify)?;
    tx.execute(
        "INSERT INTO parent(id, label) VALUES (?1, ?2)",
        params![id, label],
    )
    .map_err(classify)?;
    tx.commit().map_err(classify)
}

/// 故意写入孤立子行，验证外键**真实拒错**；约束错误不得被当作 busy 重试。
///
/// # Errors
/// 期望返回 `ProtoError::Constraint`；任何成功都意味着 `foreign_keys` 未生效（缺陷）。
pub fn insert_orphan_child(
    conn: &mut Connection,
    child_id: i64,
    parent_id: i64,
) -> Result<(), ProtoError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(classify)?;
    tx.execute(
        "INSERT INTO child(id, parent_id, note) VALUES (?1, ?2, 'orphan')",
        params![child_id, parent_id],
    )
    .map_err(classify)?;
    tx.commit().map_err(classify)
}

/// 有限写竞争辅助：只有 `Busy` 才重试，次数与退避均有硬上限；
/// `Constraint`/`Io`/`Other` 立即原样返回，绝不吞成 busy。
/// 事务内不做任何网络/推理/渲染等待；退避仅为本地 `sleep`。
///
/// # Errors
/// 最后一次仍为 Busy，或遇非 Busy 错误时，返回该**真实**分类错误。
pub fn write_with_finite_retry<F>(mut operation: F) -> Result<(), ProtoError>
where
    F: FnMut() -> Result<(), ProtoError>,
{
    for attempt in 1..=MAX_BUSY_ATTEMPTS {
        match operation() {
            Ok(()) => return Ok(()),
            Err(classified) => {
                if matches!(classified, ProtoError::Busy(_)) && attempt < MAX_BUSY_ATTEMPTS {
                    std::thread::sleep(BUSY_BACKOFF);
                    continue;
                }
                return Err(classified);
            }
        }
    }
    Err(ProtoError::Other(
        "finite retry loop ended without a result".to_owned(),
    ))
}
