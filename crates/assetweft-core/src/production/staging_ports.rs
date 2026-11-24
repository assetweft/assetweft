pub(crate) use super::ports::AcceptanceError as StagingError;
use super::{
    domain::{Candidate, ComponentSnapshot},
    staging_application::FrozenProposal,
};
pub(crate) struct Owners {
    pub base_source: String,
    pub proposed_source: String,
    pub base_asset: String,
    pub proposed_asset: String,
}
pub(crate) trait Staging {
    fn owners(&mut self, base: &str, proposed: &str) -> Result<Owners, StagingError>;
    fn snapshot(
        &mut self,
        owner: &str,
        max_component_bytes: Option<u64>,
    ) -> Result<Vec<ComponentSnapshot>, StagingError>;
    fn freeze(&mut self, proposal: FrozenProposal) -> Result<Candidate, StagingError>;
}
pub(crate) trait CandidateRead {
    fn list(&mut self, limit: u16) -> Result<Vec<(String, Candidate)>, StagingError>;
    fn candidate(&mut self, id: &str) -> Result<Option<Candidate>, StagingError>;
}

pub(crate) trait RiggedSimpleStaging: Staging {
    fn rigged_simple_snapshot(
        &mut self,
        owner: &str,
        expected_sha256: &str,
    ) -> Result<Vec<ComponentSnapshot>, StagingError>;
}
