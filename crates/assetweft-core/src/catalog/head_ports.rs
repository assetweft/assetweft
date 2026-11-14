//! Responsibility: exact fixture head query capability.
//! Dependencies: Catalog values only; callers: query use case and SQLite adapter.
//! Invariants: no mutation or SQL counts in this capability.
use super::domain::{Current, MetadataError};
pub(crate) trait HeadQuery {
    fn exact_current(&mut self, asset: &str) -> Result<Option<Current>, MetadataError>;
}
