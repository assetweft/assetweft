use std::fmt;
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, TransactionBehavior, params};

/// 单连接 SQLite 级有限忙等待（`sqlite3_busy_timeout`），不是无限循环。
pub const BUSY_TIMEOUT: Duration = Duration::from_millis(200);
pub const MAX_BUSY_ATTEMPTS: u32 = 3;
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
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Busy(_) => "BUSY",
            Self::Constraint(_) => "CONSTRAINT",
            Self::Io(_) => "IO",
            Self::Other(_) => "OTHER",
        }
    }

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
/// Uses SQLite's rollback journal; the WAL-reset race is specific to WAL mode.
pub fn open_and_configure(path: &Path) -> Result<(Connection, ConnectionConfig), ProtoError> {
    configure(Connection::open(path).map_err(classify)?)
}

pub(crate) fn open_read_only(path: &Path) -> Result<(Connection, ConnectionConfig), ProtoError> {
    configure(
        Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(classify)?,
    )
}

fn configure(conn: Connection) -> Result<(Connection, ConnectionConfig), ProtoError> {
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

pub(crate) mod acceptance;
