//! Responsibility: legacy candidate API and concrete assembly for staging, initialization and receipt reads.
//! Dependencies: Production use cases and concrete SQLite/CAS adapters.
//! Callers: Application and acceptance adapter; invariants: SAME public types, diagnostics and initialization behavior.
pub use crate::catalog::domain::{Current, FixtureContract};
use crate::library::LibraryError;
pub use crate::production::domain::{
    Acceptance, AcceptanceReport, Candidate, ComponentEvidence, ComponentSnapshot,
};
pub use crate::production::staging_domain::Proposal;
pub(crate) use crate::production::staging_sqlite::{
    check_refs, guard, measured, validate_components,
};
use rusqlite::Connection;
use std::path::Path;
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
    crate::production::initialization_application::initialize(
        &mut crate::production::initialization_sqlite::SqliteInitialization { conn, root },
        version,
        contract,
    )
    .map_err(Into::into)
}

pub(crate) fn attempt_record(
    conn: &Connection,
    id: &str,
) -> Result<Option<Acceptance>, LibraryError> {
    crate::production::initialization_application::read(
        &mut crate::production::initialization_sqlite::SqliteAttemptRead { conn },
        id,
    )
    .map_err(Into::into)
}
