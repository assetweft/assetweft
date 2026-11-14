//! Responsibility: semantic durable workflow capability owned by Production.
//! Dependencies: domain values only; callers: use cases and persistence adapter.
//! Invariants: claim/finish/cancel are atomic; claim returns only after commit.
use super::workflow_domain::{Outcome, PolicyError, Step, StepRecord};

pub(crate) trait WorkflowQuery {
    type Error: From<PolicyError>;
    fn read(&self, run: &str) -> Result<Vec<StepRecord>, Self::Error>;
    fn is_cancelled(&self, run: &str) -> Result<bool, Self::Error>;
}
/// Each write completes its own atomic transaction before returning.
pub(crate) trait WorkflowRuns: WorkflowQuery {
    fn create(&mut self, run: &str, steps: &[Step]) -> Result<(), Self::Error>;
    fn claim(&mut self, run: &str) -> Result<Option<String>, Self::Error>;
    fn finish(&mut self, run: &str, id: &str, outcome: &Outcome) -> Result<(), Self::Error>;
    fn cancel(&mut self, run: &str) -> Result<(), Self::Error>;
}
