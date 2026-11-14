//! Responsibility: real SQLite/CAS fixture initialization and persisted receipt query.
//! Dependencies: Production semantic ports, bounded staging IO, Catalog borrowed head adapter.
//! Callers: Application and acceptance adapter; invariants: validation outside transaction, lease→refs→IMMEDIATE→commit; no replay.
use crate::{
    library::{self, LibraryError},
    production::{
        domain::{Acceptance, AcceptanceReport, ComponentSnapshot},
        initialization_application::VerifiedInitialization,
        initialization_ports::{
            AttemptRead, BaseOwner, Current, FixtureContract, Initialization, InitializationError,
        },
        staging_sqlite::{check_refs, guard, snapshot, validate_components},
    },
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use std::path::Path;

pub(crate) struct SqliteInitialization<'a> {
    pub conn: &'a mut Connection,
    pub root: &'a Path,
}
impl Initialization for SqliteInitialization<'_> {
    fn base_owner(&mut self, version: &str) -> Result<BaseOwner, InitializationError> {
        let owner = crate::catalog::sqlite_metadata::asset_version(self.conn, version)?
            .ok_or(InitializationError::InvalidInput("base version"))?;
        Ok(BaseOwner {
            asset_id: owner.asset_id,
            source: owner.source,
        })
    }
    fn snapshot(&mut self, version: &str) -> Result<Vec<ComponentSnapshot>, InitializationError> {
        snapshot(self.conn, self.root, version).map_err(Into::into)
    }
    fn validate(
        &mut self,
        snapshots: &[ComponentSnapshot],
        contract: &FixtureContract,
    ) -> Result<AcceptanceReport, InitializationError> {
        validate_components(self.conn, self.root, snapshots, contract).map_err(Into::into)
    }
    fn initialize(
        &mut self,
        proof: VerifiedInitialization<'_>,
    ) -> Result<Current, InitializationError> {
        initialize(self.conn, self.root, proof).map_err(Into::into)
    }
}
fn initialize(
    conn: &mut Connection,
    root: &Path,
    proof: VerifiedInitialization<'_>,
) -> Result<Current, LibraryError> {
    let (asset_id, version, contract, snapshots) = proof.into_parts();
    let _guard = guard(root)?;
    check_refs(conn, version, &snapshots)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result = crate::catalog::sqlite_store::initialize(&tx, &asset_id, version, contract)?;
    tx.commit()?;
    Ok(result)
}
pub(crate) struct SqliteAttemptRead<'a> {
    pub conn: &'a Connection,
}
impl AttemptRead for SqliteAttemptRead<'_> {
    fn receipt(&mut self, id: &str) -> Result<Option<Acceptance>, InitializationError> {
        receipt(self.conn, id).map_err(Into::into)
    }
}
fn receipt(conn: &Connection, id: &str) -> Result<Option<Acceptance>, LibraryError> {
    library::require_current(conn)?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT receipt FROM aw_candidate_attempts WHERE candidate_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|s| {
        serde_json::from_str(&s)
            .map_err(|_| LibraryError::InvalidInput("stored acceptance receipt"))
    })
    .transpose()
}
