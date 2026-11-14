//! Responsibility: concrete exact head SQL and stored-contract decoding.
//! Dependencies: rusqlite and library schema infrastructure; callers: query composition.
//! Invariants: original SQL, schema and decode order; no write or transaction lifecycle.
use super::{
    domain::{Current, MetadataError},
    head_ports::HeadQuery,
};
use crate::library::{self, LibraryError};
use rusqlite::{Connection, OptionalExtension};
pub(crate) struct SqliteHead<'a>(pub(crate) &'a Connection);
impl HeadQuery for SqliteHead<'_> {
    fn exact_current(&mut self, asset: &str) -> Result<Option<Current>, MetadataError> {
        read(self.0, asset).map_err(Into::into)
    }
}
fn invalid(s: &'static str) -> LibraryError {
    LibraryError::InvalidInput(s)
}
fn read(conn: &Connection, asset: &str) -> Result<Option<Current>, LibraryError> {
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
