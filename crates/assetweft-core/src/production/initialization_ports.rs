//! Responsibility: fixture initialization and acceptance receipt query capabilities.
//! Dependencies: stable domain values and sealed use-case proof; callers: pure use cases/SQLite adapter.
//! Invariants: no database/path/vendor types or caller-issued PASS evidence.
pub(crate) use super::ports::AcceptanceError as InitializationError;
use super::{
    domain::{Acceptance, AcceptanceReport, ComponentSnapshot},
    initialization_application::VerifiedInitialization,
};
pub(crate) use crate::catalog::domain::{Current, FixtureContract};

pub(crate) struct BaseOwner {
    pub asset_id: String,
    pub source: String,
}
pub(crate) trait Initialization {
    fn base_owner(&mut self, version: &str) -> Result<BaseOwner, InitializationError>;
    fn snapshot(&mut self, version: &str) -> Result<Vec<ComponentSnapshot>, InitializationError>;
    fn validate(
        &mut self,
        snapshots: &[ComponentSnapshot],
        contract: &FixtureContract,
    ) -> Result<AcceptanceReport, InitializationError>;
    fn initialize(
        &mut self,
        proof: VerifiedInitialization<'_>,
    ) -> Result<Current, InitializationError>;
}
pub(crate) trait AttemptRead {
    fn receipt(&mut self, id: &str) -> Result<Option<Acceptance>, InitializationError>;
}
