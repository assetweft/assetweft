//! Responsibility: legacy candidate public API with SAME owner value re-exports.
//! Dependencies: Catalog and Production domain values.
//! Callers: public compatibility consumers and Application; invariant: SAME public item identity.
pub use crate::catalog::domain::{Current, FixtureContract};
pub use crate::production::domain::{
    Acceptance, AcceptanceReport, Candidate, ComponentEvidence, ComponentSnapshot,
};
pub use crate::production::rigged_simple::STRUCTURE as RIGGED_SIMPLE_MOTION_STRUCTURE_V1;
pub use crate::production::staging_domain::Proposal;
