//! Responsibility: single-database atomic Production acceptance adapter.
//! Dependencies: Production byte/receipt IO, Catalog SQLite adapter, storage lease and rusqlite.
//! Callers: compatibility Application; invariant: lease before IMMEDIATE, CAS and receipt in one commit, no replay.
use crate::catalog::domain::FixtureContract;
use crate::{
    catalog::sqlite_store,
    library::{self, LibraryError},
    production::{
        application::{CommitPlan, VerifiedAcceptance},
        domain::{Acceptance, AcceptanceReport, Candidate, ComponentSnapshot},
        ports::{AcceptanceError, AcceptanceInputs, CommitAcceptance},
    },
};
use rusqlite::{Connection, TransactionBehavior, params};
use std::path::Path;

pub(crate) struct SqliteAcceptance<'a> {
    pub(crate) conn: &'a mut Connection,
    pub(crate) root: &'a Path,
}
fn invalid(s: &'static str) -> LibraryError {
    LibraryError::InvalidInput(s)
}
fn record(conn: &Connection, id: &str, result: &Acceptance) -> Result<(), LibraryError> {
    let json = serde_json::to_string(result).map_err(|_| invalid("acceptance encoding"))?;
    conn.execute(
        "INSERT INTO aw_candidate_attempts(candidate_id,receipt) VALUES(?1,?2)",
        params![id, json],
    )?;
    Ok(())
}

impl AcceptanceInputs for SqliteAcceptance<'_> {
    fn existing_receipt(&mut self, id: &str) -> Result<Option<Acceptance>, AcceptanceError> {
        crate::production::initialization_application::read(
            &mut crate::production::initialization_sqlite::SqliteAttemptRead { conn: self.conn },
            id,
        )
        .map_err(LibraryError::from)
        .map_err(Into::into)
    }
    fn candidate(&mut self, id: &str) -> Result<Option<Candidate>, AcceptanceError> {
        crate::production::staging_application::read(
            &mut crate::production::staging_sqlite::SqliteCandidateRead { conn: self.conn },
            id,
        )
        .map_err(LibraryError::from)
        .map_err(Into::into)
    }
    fn validate_proposed(
        &mut self,
        snapshots: &[ComponentSnapshot],
        contract: &FixtureContract,
    ) -> Result<AcceptanceReport, AcceptanceError> {
        crate::production::staging_sqlite::validate_components(
            self.conn, self.root, snapshots, contract,
        )
        .map_err(Into::into)
    }
    fn remeasure_base(&mut self, snapshots: &[ComponentSnapshot]) -> Result<(), AcceptanceError> {
        for s in snapshots {
            if crate::production::staging_sqlite::measured(self.conn, self.root, &s.version)?
                .content_hash
                != s.measured_sha256
            {
                return Err(AcceptanceError::InvalidInput("base byte drift"));
            }
        }
        Ok(())
    }
}
impl CommitAcceptance for SqliteAcceptance<'_> {
    fn commit_acceptance(
        &mut self,
        decision: VerifiedAcceptance,
    ) -> Result<Acceptance, AcceptanceError> {
        self.commit(decision).map_err(Into::into)
    }
}
impl SqliteAcceptance<'_> {
    fn commit(&mut self, decision: VerifiedAcceptance) -> Result<Acceptance, LibraryError> {
        let candidate = decision.candidate();
        let id = candidate.id.clone();
        let _guard = crate::production::staging_sqlite::guard(self.root)?;
        crate::production::staging_sqlite::check_refs(
            self.conn,
            &candidate.base_version,
            &candidate.base,
        )?;
        crate::production::staging_sqlite::check_refs(
            self.conn,
            &candidate.candidate_version,
            &candidate.proposed,
        )?;
        let owner =
            crate::catalog::sqlite_metadata::asset_version(self.conn, &candidate.base_version)?
                .ok_or_else(|| invalid("base version"))?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        library::require_current(&tx)?;
        if let Some(old) = crate::production::initialization_application::read(
            &mut crate::production::initialization_sqlite::SqliteAttemptRead { conn: &tx },
            &id,
        )
        .map_err(LibraryError::from)?
        {
            return Ok(old);
        }
        let head = if decision.needs_head() {
            sqlite_store::current(&tx, &owner.asset_id)?
        } else {
            None
        };
        let result = match decision.resolve(head) {
            CommitPlan::Terminal(result) => result,
            CommitPlan::Advance { candidate, report } => {
                let (changed, current) = sqlite_store::advance(
                    &tx,
                    &owner.asset_id,
                    &candidate.base_version,
                    candidate.expected_revision,
                    &candidate.candidate_version,
                )?;
                if changed == 1 {
                    Acceptance::Accepted {
                        candidate,
                        current,
                        report,
                    }
                } else {
                    Acceptance::Conflict {
                        candidate,
                        current,
                        report,
                    }
                }
            }
        };
        record(&tx, &id, &result)?;
        tx.commit()?;
        Ok(result)
    }
}

impl From<LibraryError> for AcceptanceError {
    fn from(error: LibraryError) -> Self {
        match error {
            LibraryError::Busy(value) => Self::Busy(value),
            LibraryError::Constraint(value) => Self::Constraint(value),
            LibraryError::Io(value) => Self::Io(value),
            LibraryError::Database(value) => Self::Database(value),
            LibraryError::FutureSchema(value) => Self::FutureSchema(value),
            LibraryError::InvalidInput(value) => Self::InvalidInput(value),
            LibraryError::UnknownValue(value) => Self::UnknownValue(value),
            LibraryError::Uninitialized => Self::Uninitialized,
        }
    }
}

impl From<AcceptanceError> for LibraryError {
    fn from(error: AcceptanceError) -> Self {
        match error {
            AcceptanceError::Busy(value) => Self::Busy(value),
            AcceptanceError::Constraint(value) => Self::Constraint(value),
            AcceptanceError::Io(value) => Self::Io(value),
            AcceptanceError::Database(value) => Self::Database(value),
            AcceptanceError::FutureSchema(value) => Self::FutureSchema(value),
            AcceptanceError::InvalidInput(value) => Self::InvalidInput(value),
            AcceptanceError::UnknownValue(value) => Self::UnknownValue(value),
            AcceptanceError::Uninitialized => Self::Uninitialized,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compatibility_error_categories_and_diagnostics_round_trip() {
        for error in [
            LibraryError::Busy("busy detail".into()),
            LibraryError::Constraint("constraint detail".into()),
            LibraryError::Io("io detail".into()),
            LibraryError::Database("database detail".into()),
            LibraryError::FutureSchema(99),
            LibraryError::Uninitialized,
            LibraryError::InvalidInput("revision overflow"),
            LibraryError::UnknownValue("stored value".into()),
        ] {
            assert_eq!(
                LibraryError::from(AcceptanceError::from(error.clone())),
                error
            );
        }
    }
}
