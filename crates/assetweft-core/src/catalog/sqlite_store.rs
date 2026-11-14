//! Responsibility: sole runtime SQL owner of aw_candidate_current.
//! Dependencies: Catalog values, library schema guard/errors and rusqlite.
//! Callers: Application reads and candidate fixture acceptance orchestration.
//! Invariants: writes borrow caller transaction; no lease acquisition or commit here.
use super::domain::{Current, FixtureContract};
use crate::library::{self, LibraryError};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
fn invalid(s: &'static str) -> LibraryError {
    LibraryError::InvalidInput(s)
}
fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && id != "."
        && id != ".."
}
pub(crate) fn current(conn: &Connection, asset: &str) -> Result<Option<Current>, LibraryError> {
    if !id_ok(asset) {
        return Err(invalid("asset identity"));
    }
    library::require_current(conn)?;
    let raw: Option<(String, i64, String)> = conn
        .query_row(
            "SELECT version_id,revision,contract FROM aw_candidate_current WHERE asset_id=?1",
            [asset],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    raw.map(|(version_id, revision, contract)| {
        Ok(Current {
            asset_id: asset.into(),
            version_id,
            revision,
            contract: serde_json::from_str(&contract).map_err(|_| invalid("stored contract"))?,
        })
    })
    .transpose()
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
