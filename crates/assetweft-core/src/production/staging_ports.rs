//! Responsibility: purpose-owned exact snapshot and atomic candidate freezing capability.
//! Dependencies: stable domain values only; callers: staging use cases and SQLite/CAS adapter.
//! Invariants: no connection, path, SQL row or caller-supplied acceptance evidence crosses this port.
pub(crate) use super::ports::AcceptanceError as StagingError;
use super::{
    domain::{Candidate, ComponentSnapshot},
    staging_application::FrozenProposal,
};
/// Facts about the two exact immutable Catalog owners, read in request order.
pub(crate) struct Owners {
    pub base_source: String,
    pub proposed_source: String,
    pub base_asset: String,
    pub proposed_asset: String,
}
pub(crate) trait Staging {
    fn owners(&mut self, base: &str, proposed: &str) -> Result<Owners, StagingError>;
    /// `max_component_bytes` is the use-case-validated declared bound; it is checked
    /// against the actual regular file before CAS hashing. `None` retains the ceiling.
    fn snapshot(
        &mut self,
        owner: &str,
        max_component_bytes: Option<u64>,
    ) -> Result<Vec<ComponentSnapshot>, StagingError>;
    fn freeze(&mut self, proposal: FrozenProposal) -> Result<Candidate, StagingError>;
}
pub(crate) trait CandidateRead {
    fn candidate(&mut self, id: &str) -> Result<Option<Candidate>, StagingError>;
}

pub(crate) trait RiggedSimpleStaging: Staging {
    fn rigged_simple_snapshot(
        &mut self,
        owner: &str,
        expected_sha256: &str,
    ) -> Result<Vec<ComponentSnapshot>, StagingError>;
}
