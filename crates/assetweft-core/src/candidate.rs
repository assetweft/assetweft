//! Responsibility: legacy candidate API and concrete assembly for staging/initialization.
//! Dependencies: Production staging use cases/adapters and Catalog initialization adapter.
//! Callers: Application and acceptance adapter; invariants: SAME public types, diagnostics and initialization behavior.
pub use crate::catalog::domain::{Current, FixtureContract};
use crate::library::{self, LibraryError};
pub use crate::production::domain::{
    Acceptance, AcceptanceReport, Candidate, ComponentEvidence, ComponentSnapshot,
};
pub use crate::production::staging_domain::Proposal;
pub(crate) use crate::production::staging_sqlite::{
    check_refs, guard, measured, validate_components,
};
use crate::production::{
    domain::report_passes,
    staging_domain::{check_contract, id_ok},
    staging_sqlite::snapshot,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use std::path::Path;
fn invalid(s: &'static str) -> LibraryError {
    LibraryError::InvalidInput(s)
}
pub(crate) fn propose(
    conn: &mut Connection,
    root: &Path,
    p: &Proposal,
) -> Result<Candidate, LibraryError> {
    crate::production::staging_application::propose(
        &mut crate::production::staging_sqlite::SqliteStaging { conn, root },
        p,
    )
    .map_err(Into::into)
}
pub(crate) fn read(conn: &Connection, id: &str) -> Result<Option<Candidate>, LibraryError> {
    crate::production::staging_application::read(
        &mut crate::production::staging_sqlite::SqliteCandidateRead { conn },
        id,
    )
    .map_err(Into::into)
}
/// Bootstrap only an absent fixture current; never reset an existing head/revision.
pub(crate) fn initialize_current(
    conn: &mut Connection,
    root: &Path,
    version: &str,
    contract: &FixtureContract,
) -> Result<Current, LibraryError> {
    if !id_ok(version) {
        return Err(invalid("version identity"));
    }
    check_contract(contract).map_err(LibraryError::from)?;
    if !contract.required_quality.is_empty() {
        return Err(invalid("required quality NOT_RUN"));
    }
    let owner = library::asset_version(conn, version)?.ok_or_else(|| invalid("base version"))?;
    if owner.source != "TEST_FIXTURE" || !id_ok(&owner.asset_id) {
        return Err(invalid("fixture base source/asset"));
    }
    let snapshots = snapshot(conn, root, version)?;
    let report = validate_components(conn, root, &snapshots, contract)?;
    if !report_passes(&report, snapshots.len()) {
        return Err(invalid("base GLB structure/role validation"));
    }
    let _guard = guard(root)?;
    check_refs(conn, version, &snapshots)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result = crate::catalog::sqlite_store::initialize(&tx, &owner.asset_id, version, contract)?;
    tx.commit()?;
    Ok(result)
}
pub(crate) fn attempt_record(
    conn: &Connection,
    id: &str,
) -> Result<Option<Acceptance>, LibraryError> {
    if !id_ok(id) {
        return Err(invalid("candidate id"));
    }
    library::require_current(conn)?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT receipt FROM aw_candidate_attempts WHERE candidate_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|s| serde_json::from_str(&s).map_err(|_| invalid("stored acceptance receipt")))
        .transpose()
}
