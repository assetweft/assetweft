//! Responsibility: M1 local metadata, references and versioned SQLite migration.
//! Dependencies: configured application connection, rusqlite and the versioned 001–006 SQL migrations (M1 objects, M2 workflow/budget).
//! Callers: Application's typed entry points. No connection or SQL is exposed here.
//! Invariants: IDs are caller-supplied, versions are immutable, no state claims a file is
//! ready by itself; managed bytes use storage, references are exact, migrations atomic.

use crate::sqlite::ProtoError;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{error::Error, fmt};

pub(crate) const CURRENT_SCHEMA: i64 = 6;
const INITIAL_SQL: &str = include_str!("library/001_initial.sql");

/// File availability known to this metadata-only repository. No ready state is issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// Presence has not been checked.
    Unknown,
    /// The referenced file is missing.
    Missing,
}
impl Presence {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "UNKNOWN",
            Self::Missing => "MISSING",
        }
    }
    fn parse(value: &str) -> Result<Self, LibraryError> {
        match value {
            "UNKNOWN" => Ok(Self::Unknown),
            "MISSING" => Ok(Self::Missing),
            _ => Err(LibraryError::UnknownValue(value.to_owned())),
        }
    }
}

/// AW-003 component role vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentRole {
    Mesh,
    Materials,
    Rig,
    Motion,
    Vfx,
    Audio,
    Dialogue,
}
impl ComponentRole {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Mesh => "mesh",
            Self::Materials => "materials",
            Self::Rig => "rig",
            Self::Motion => "motion",
            Self::Vfx => "vfx",
            Self::Audio => "audio",
            Self::Dialogue => "dialogue",
        }
    }
    fn parse(value: &str) -> Result<Self, LibraryError> {
        match value {
            "mesh" => Ok(Self::Mesh),
            "materials" => Ok(Self::Materials),
            "rig" => Ok(Self::Rig),
            "motion" => Ok(Self::Motion),
            "vfx" => Ok(Self::Vfx),
            "audio" => Ok(Self::Audio),
            "dialogue" => Ok(Self::Dialogue),
            _ => Err(LibraryError::UnknownValue(value.to_owned())),
        }
    }
}

/// Immutable metadata; recording this does not commit or validate object bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetVersion {
    pub asset_id: String,
    pub version_id: String,
    pub content_hash: String,
    pub input_id: String,
    pub source: String,
    pub authorization: String,
    pub authorized: bool,
    pub license_id: String,
    pub usage_scope: String,
    pub source_format: String,
    /// Version of AW-CONTRACT-001; independent of this database's schema version.
    pub contract_version: String,
    pub presence: Presence,
}

/// An explicit reference to a component version, never an implicit latest pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentReference {
    pub owner_version_id: String,
    pub role: ComponentRole,
    pub component_version_id: String,
}

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
fn valid_id(value: &str) -> bool {
    !value.is_empty() && !value.contains(['/', '\\', '\0']) && value != "." && value != ".."
}
pub(crate) fn validate(v: &AssetVersion) -> Result<(), LibraryError> {
    if v.contract_version != "1.0.0" {
        return Err(LibraryError::UnknownValue(v.contract_version.clone()));
    }
    if !valid_id(&v.asset_id) || !valid_id(&v.version_id) || !valid_id(&v.input_id) {
        return Err(LibraryError::InvalidInput("identity"));
    }
    if v.content_hash.len() != 64
        || !v
            .content_hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(LibraryError::InvalidInput("content hash"));
    }
    if [
        &v.source,
        &v.authorization,
        &v.license_id,
        &v.usage_scope,
        &v.source_format,
    ]
    .iter()
    .any(|v| v.is_empty())
    {
        return Err(LibraryError::InvalidInput("source or license"));
    }
    Ok(())
}
pub(crate) fn add_asset_version(
    conn: &mut Connection,
    v: &AssetVersion,
) -> Result<(), LibraryError> {
    validate(v)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_current(&tx)?;
    insert_version(&tx, v)?;
    tx.commit()?;
    Ok(())
}
pub(crate) fn asset_version(
    conn: &mut Connection,
    id: &str,
) -> Result<Option<AssetVersion>, LibraryError> {
    if !valid_id(id) {
        return Err(LibraryError::InvalidInput("version identity"));
    }
    let tx = conn.transaction()?;
    require_current(&tx)?;
    let result = tx.query_row("SELECT asset_id,version_id,content_hash,input_id,source,authorization,authorized,license_id,usage_scope,source_format,contract_version,presence FROM aw_asset_versions WHERE version_id=?1", [id], |r| {
        Ok((AssetVersion { asset_id:r.get(0)?, version_id:r.get(1)?, content_hash:r.get(2)?, input_id:r.get(3)?, source:r.get(4)?, authorization:r.get(5)?, authorized:r.get(6)?, license_id:r.get(7)?, usage_scope:r.get(8)?, source_format:r.get(9)?, contract_version:r.get(10)?, presence: Presence::Unknown }, r.get::<_, String>(11)?))
    }).optional()?;
    let result = result
        .map(|(mut v, state)| {
            if v.contract_version != "1.0.0" {
                return Err(LibraryError::UnknownValue(v.contract_version));
            }
            v.presence = Presence::parse(&state)?;
            Ok::<_, LibraryError>(v)
        })
        .transpose()?;
    tx.commit()?;
    Ok(result)
}
pub(crate) fn add_component_reference(
    conn: &mut Connection,
    r: &ComponentReference,
) -> Result<(), LibraryError> {
    if !valid_id(&r.owner_version_id) || !valid_id(&r.component_version_id) {
        return Err(LibraryError::InvalidInput("version identity"));
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_current(&tx)?;
    tx.execute("INSERT INTO aw_component_references(owner_version_id,role,component_version_id) VALUES(?1,?2,?3)", params![r.owner_version_id, r.role.as_str(), r.component_version_id])?;
    tx.commit()?;
    Ok(())
}
pub(crate) fn component_references(
    conn: &mut Connection,
    id: &str,
) -> Result<Vec<ComponentReference>, LibraryError> {
    if !valid_id(id) {
        return Err(LibraryError::InvalidInput("version identity"));
    }
    let tx = conn.transaction()?;
    require_current(&tx)?;
    let rows = {
        let mut stmt = tx.prepare("SELECT owner_version_id,role,component_version_id FROM aw_component_references WHERE owner_version_id=?1 ORDER BY role,component_version_id")?;
        let mapped = stmt.query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut output = Vec::new();
        for row in mapped {
            let (owner_version_id, role, component_version_id) = row?;
            output.push(ComponentReference {
                owner_version_id,
                role: ComponentRole::parse(&role)?,
                component_version_id,
            });
        }
        output
    };
    tx.commit()?;
    Ok(rows)
}

// Shared insertion in the caller's checked short transaction.
pub(crate) fn insert_version(tx: &Connection, v: &AssetVersion) -> Result<(), LibraryError> {
    tx.execute(
        "INSERT INTO aw_assets(asset_id) VALUES (?1) ON CONFLICT(asset_id) DO NOTHING",
        [&v.asset_id],
    )?;
    tx.execute("INSERT INTO aw_asset_versions(version_id,asset_id,content_hash,input_id,source,authorization,authorized,license_id,usage_scope,source_format,contract_version,presence) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)", params![v.version_id, v.asset_id, v.content_hash, v.input_id, v.source, v.authorization, v.authorized, v.license_id, v.usage_scope, v.source_format, v.contract_version, v.presence.as_str()])?;
    Ok(())
}
