//! Responsibility: narrow capabilities required by the acceptance use case.
//! Dependencies: Production values/verified command and stable Catalog contract.
//! Callers: use case and single-database adapter; no vendor errors or transaction handles.
use super::{
    application::VerifiedAcceptance,
    domain::{Acceptance, AcceptanceReport, Candidate, ComponentSnapshot},
};
use crate::catalog::domain::FixtureContract;

/// Stable failures retain the compatibility facade's existing categories and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AcceptanceError {
    Busy(String),
    Constraint(String),
    Io(String),
    Database(String),
    FutureSchema(i64),
    Uninitialized,
    InvalidInput(&'static str),
    UnknownValue(String),
}
pub(crate) trait AcceptanceInputs {
    fn existing_receipt(&mut self, id: &str) -> Result<Option<Acceptance>, AcceptanceError>;
    fn candidate(&mut self, id: &str) -> Result<Option<Candidate>, AcceptanceError>;
    fn validate_proposed(
        &mut self,
        snapshots: &[ComponentSnapshot],
        contract: &FixtureContract,
    ) -> Result<AcceptanceReport, AcceptanceError>;
    fn remeasure_base(&mut self, snapshots: &[ComponentSnapshot]) -> Result<(), AcceptanceError>;
}
/// One atomic acceptance operation, never a replayable generic transaction callback.
pub(crate) trait CommitAcceptance {
    fn commit_acceptance(
        &mut self,
        decision: VerifiedAcceptance,
    ) -> Result<Acceptance, AcceptanceError>;
}
