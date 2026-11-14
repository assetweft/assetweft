//! Responsibility: borrowed head writes and compatibility exact-query composition.
//! Dependencies: Catalog query use case/adapter, library errors and rusqlite.
//! Callers: candidate fixture initialization and acceptance orchestration.
//! Invariants: writes borrow caller transaction; no lease acquisition or commit here.
use super::domain::{Current, FixtureContract};
use crate::library::LibraryError;
use rusqlite::{Connection, Transaction, params};
fn invalid(s: &'static str) -> LibraryError {
    LibraryError::InvalidInput(s)
}
pub(crate) fn current(conn: &Connection, asset: &str) -> Result<Option<Current>, LibraryError> {
    super::head_application::current(&mut super::head_sqlite::SqliteHead(conn), asset)
        .map_err(Into::into)
}
/// Register a checked fixture head without resetting an existing head.
pub(crate) fn initialize(
    tx: &Transaction<'_>,
    asset: &str,
    version: &str,
    contract: &FixtureContract,
) -> Result<Current, LibraryError> {
    let encoded = serde_json::to_string(contract).map_err(|_| invalid("contract encoding"))?;
    if let Some(existing) = current(tx, asset)? {
        if existing.version_id != version
            || existing.revision != 0
            || &existing.contract != contract
        {
            return Err(invalid("current already initialized"));
        }
        return Ok(existing);
    }
    tx.execute("INSERT INTO aw_candidate_current(asset_id,version_id,revision,contract) VALUES(?1,?2,0,?3)",params![asset,version,encoded])?;
    let result = current(tx, asset)?.ok_or_else(|| invalid("current missing"))?;
    Ok(result)
}
/// Compare-and-swap within the caller's Immediate receipt transaction.
pub(crate) fn advance(
    tx: &Transaction<'_>,
    asset: &str,
    base: &str,
    expected_revision: i64,
    version: &str,
) -> Result<(usize, Current), LibraryError> {
    let next = expected_revision
        .checked_add(1)
        .ok_or_else(|| invalid("revision overflow"))?;
    let changed = tx.execute("UPDATE aw_candidate_current SET version_id=?1,revision=?2 WHERE asset_id=?3 AND version_id=?4 AND revision=?5", params![version,next,asset,base,expected_revision])?;
    let head = current(tx, asset)?.ok_or_else(|| invalid("current missing"))?;
    Ok((changed, head))
}
