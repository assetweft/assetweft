use super::{
    application::VerifiedAcceptance,
    domain::{Acceptance, AcceptanceReport, Candidate, ComponentSnapshot},
};
use crate::catalog::domain::FixtureContract;

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
pub(crate) trait AcceptanceInputs: super::rigged_simple::InspectOwner {
    fn existing_receipt(&mut self, id: &str) -> Result<Option<Acceptance>, AcceptanceError>;
    fn candidate(&mut self, id: &str) -> Result<Option<Candidate>, AcceptanceError>;
    fn validate_proposed(
        &mut self,
        snapshots: &[ComponentSnapshot],
        contract: &FixtureContract,
    ) -> Result<AcceptanceReport, AcceptanceError>;
    fn remeasure_base(
        &mut self,
        snapshots: &[ComponentSnapshot],
        max_component_bytes: u64,
    ) -> Result<(), AcceptanceError>;
}
pub(crate) trait CommitAcceptance {
    fn commit_acceptance(
        &mut self,
        decision: VerifiedAcceptance,
    ) -> Result<Acceptance, AcceptanceError>;
}
