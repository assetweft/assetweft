//! Responsibility: purpose-owned managed-object publication and retention capability.
//! Dependencies: Catalog values only; callers: pure managed-object use cases.
//! Invariants: publish atomically registers under the shared retention lease; collect evaluates current policy there.
use super::{
    domain::AssetVersion,
    objects_domain::{Collection, ObjectPolicyError, StoredObject},
};

pub(crate) trait ManagedObjects {
    // Opaque adapter failure preserves diagnostics without making core inspect SDK errors.
    type Error: From<ObjectPolicyError>;
    // A bounded verified staging receipt, never an IO handle interpreted by core.
    type Staged;
    fn stage(
        &mut self,
        version: &AssetVersion,
        expected_len: u64,
    ) -> Result<Self::Staged, Self::Error>;
    fn publish(
        &mut self,
        version: &AssetVersion,
        staged: Self::Staged,
    ) -> Result<StoredObject, Self::Error>;
    fn managed(&mut self, id: &str) -> Result<Option<StoredObject>, Self::Error>;
    fn protect(
        &mut self,
        hash: &str,
        backup_pin: bool,
        unsynced_unique: bool,
    ) -> Result<(), Self::Error>;
    fn collect(&mut self, hash: &str) -> Result<Collection, Self::Error>;
}
