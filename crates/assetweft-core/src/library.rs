//! Responsibility: SQLite migration chain (schema 1–6) and LibraryError diagnostics.
//! Dependencies: rusqlite, the versioned 001–006 SQL migrations and Catalog type re-exports (M1 objects, M2 workflow/budget).
//! Callers: Application connection setup, concrete owner adapters requiring schema guards and compatibility consumers.
//! Invariants: IDs are caller-supplied, versions are immutable, no state claims a file is
//! ready by itself; managed bytes use storage, references are exact, migrations atomic.

use crate::sqlite::ProtoError;
use rusqlite::{Connection, TransactionBehavior};
use std::{error::Error, fmt};

pub(crate) const CURRENT_SCHEMA: i64 = 6;
const INITIAL_SQL: &str = include_str!("library/001_initial.sql");

/// Compatibility paths refer to the identical Catalog domain types.
pub use crate::catalog::domain::{AssetVersion, ComponentReference, ComponentRole, Presence};

/// Errors at the product boundary; underlying database diagnostics are retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryError {
    Busy(String),
    Constraint(String),
    Io(String),
    Database(String),
    FutureSchema(i64),
    Uninitialized,
    InvalidInput(&'static str),
    UnknownValue(String),
}
impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy(s) => write!(f, "BUSY: {s}"),
            Self::Constraint(s) => write!(f, "CONSTRAINT: {s}"),
            Self::Io(s) => write!(f, "IO: {s}"),
            Self::Database(s) => write!(f, "DATABASE: {s}"),
            Self::FutureSchema(v) => write!(f, "unsupported future library schema {v}"),
            Self::Uninitialized => write!(f, "library schema is not initialized"),
            Self::InvalidInput(s) => write!(f, "invalid {s}"),
            Self::UnknownValue(s) => write!(f, "unknown stored value {s:?}"),
        }
    }
}
impl Error for LibraryError {}
impl From<rusqlite::Error> for LibraryError {
    fn from(err: rusqlite::Error) -> Self {
        let message = err.to_string();
        if let rusqlite::Error::SqliteFailure(code, _) = &err {
            match code.extended_code & 0xff {
                5 | 6 => return Self::Busy(message),
                19 => return Self::Constraint(message),
                10 | 13 | 14 => return Self::Io(message),
                _ => {}
            }
        }
        Self::Database(message)
    }
}
impl LibraryError {
    pub(crate) fn into_open_error(self) -> ProtoError {
        match self {
            Self::Busy(s) => ProtoError::Busy(s),
            Self::Constraint(s) => ProtoError::Constraint(s),
            Self::Io(s) => ProtoError::Io(s),
            other => ProtoError::Other(other.to_string()),
        }
    }
}

// Absence means pre-product-schema v0; a malformed metadata table fails closed.
fn version(conn: &Connection) -> Result<Option<i64>, LibraryError> {
    let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='aw_schema_version')", [], |r| r.get(0))?;
    if !exists {
        return Ok(None);
    }
    let mut stmt = conn.prepare("SELECT singleton, version FROM aw_schema_version")?;
    let mut rows = stmt.query([])?;
    let row = rows
        .next()?
        .ok_or_else(|| LibraryError::Database("missing schema version row".into()))?;
    let singleton: i64 = row.get(0)?;
    let value: i64 = row.get(1)?;
    if singleton != 1 || value < 1 || rows.next()?.is_some() {
        return Err(LibraryError::Database("invalid schema version rows".into()));
    }
    if value > CURRENT_SCHEMA {
        return Err(LibraryError::FutureSchema(value));
    }
    Ok(Some(value))
}

pub(crate) fn check_open(conn: &Connection) -> Result<(), LibraryError> {
    version(conn).map(|_| ())
}
pub(crate) fn require_current(conn: &Connection) -> Result<(), LibraryError> {
    match version(conn)? {
        Some(CURRENT_SCHEMA) => Ok(()),
        Some(_) => Err(LibraryError::Database("unsupported old schema".into())),
        None => Err(LibraryError::Uninitialized),
    }
}

pub(crate) fn initialize(conn: &mut Connection) -> Result<(), LibraryError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if version(&tx)?.is_none() {
        tx.execute_batch("CREATE TABLE aw_schema_version (singleton INTEGER PRIMARY KEY CHECK(singleton = 1), version INTEGER NOT NULL CHECK(version > 0));")?;
        tx.execute_batch(INITIAL_SQL)?;
        tx.execute(
            "INSERT INTO aw_schema_version(singleton, version) VALUES(1, 1)",
            [],
        )?;
    }
    if version(&tx)? == Some(1) {
        tx.execute_batch(include_str!("library/002_objects.sql"))?;
        tx.execute(
            "UPDATE aw_schema_version SET version=2 WHERE singleton=1",
            [],
        )?;
    }
    if version(&tx)? == Some(2) {
        tx.execute_batch(include_str!("library/003_workflow.sql"))?;
        tx.execute(
            "UPDATE aw_schema_version SET version=3 WHERE singleton=1",
            [],
        )?;
    }
    if version(&tx)? == Some(3) {
        tx.execute_batch(include_str!("library/004_budget.sql"))?;
        tx.execute(
            "UPDATE aw_schema_version SET version=4 WHERE singleton=1",
            [],
        )?;
    }
    if version(&tx)? == Some(4) {
        tx.execute_batch(include_str!("library/005_candidates.sql"))?;
        tx.execute(
            "UPDATE aw_schema_version SET version=5 WHERE singleton=1",
            [],
        )?;
    }
    if version(&tx)? == Some(5) {
        tx.execute_batch(include_str!("library/006_feedback.sql"))?;
        tx.execute(
            "UPDATE aw_schema_version SET version=6 WHERE singleton=1",
            [],
        )?;
    }
    require_current(&tx)?;
    tx.commit()?;
    Ok(())
}
