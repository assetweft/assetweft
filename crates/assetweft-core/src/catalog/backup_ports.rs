use super::backup_domain::{Manifest, Object, PolicyError};
pub(crate) trait ProtectedBackup {
    type Error: From<PolicyError>;
    fn snapshot(&mut self) -> Result<(Object, Vec<Object>), Self::Error>;
    fn copy_objects(&mut self, objects: &[Object]) -> Result<(), Self::Error>;
    fn publish(&mut self, manifest: &Manifest) -> Result<(), Self::Error>;
}
pub(crate) trait FreshRestore {
    type Error: From<PolicyError>;
    fn read_manifest(&mut self) -> Result<Manifest, Self::Error>;
    fn verify_snapshot(&mut self, manifest: &Manifest) -> Result<(), Self::Error>;
    fn copy_snapshot(&mut self, manifest: &Manifest) -> Result<(), Self::Error>;
    fn copy_objects(&mut self, objects: &[Object]) -> Result<(), Self::Error>;
    fn publish_restored(&mut self) -> Result<(), Self::Error>;
}
