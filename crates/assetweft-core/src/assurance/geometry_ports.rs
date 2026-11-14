//! Responsibility: geometry structure inspection capability.
//! Dependencies: plain geometry facts.
//! Callers: geometry application and GLB adapter.
//! Invariants: no SDK or concrete errors in core.

use super::geometry_domain::GeometryFacts;
pub(crate) trait GeometryStructure {
    fn parse(&self, bytes: &[u8]) -> Result<GeometryFacts, String>;
}
