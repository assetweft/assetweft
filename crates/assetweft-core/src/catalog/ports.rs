//! Responsibility: capabilities for immutable metadata registration and exact snapshot reads.
//! Dependencies: Catalog domain values only; callers: Catalog use cases and SQLite adapter.
//! Invariants: no managed-byte readiness, connection, transaction or vendor error crosses the ports.
use super::domain::{AssetVersion, ComponentReference, MetadataError};

pub(crate) trait Metadata {
    fn register_version(&mut self, version: &AssetVersion) -> Result<(), MetadataError>;
    fn exact_version(&mut self, id: &str) -> Result<Option<AssetVersion>, MetadataError>;
    fn register_reference(&mut self, reference: &ComponentReference) -> Result<(), MetadataError>;
    fn exact_references(&mut self, owner: &str) -> Result<Vec<ComponentReference>, MetadataError>;
}
