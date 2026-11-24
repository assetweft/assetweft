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
pub(crate) trait Initialization: super::rigged_simple::InspectOwner {
    fn base_owner(&mut self, version: &str) -> Result<BaseOwner, InitializationError>;
    fn snapshot(
        &mut self,
        version: &str,
        max_component_bytes: u64,
    ) -> Result<Vec<ComponentSnapshot>, InitializationError>;
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
