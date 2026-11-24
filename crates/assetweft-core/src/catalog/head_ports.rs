use super::domain::{Current, MetadataError};
pub(crate) trait HeadQuery {
    fn exact_current(&mut self, asset: &str) -> Result<Option<Current>, MetadataError>;
}
