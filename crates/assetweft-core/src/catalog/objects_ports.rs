use super::{
    domain::AssetVersion,
    objects_domain::{Collection, ObjectPolicyError, StoredObject},
};

pub(crate) trait ManagedObjects {
    type Error: From<ObjectPolicyError>;
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
