use super::domain::{AssetVersion, ComponentReference, MetadataError};

pub(crate) trait Metadata {
    fn register_version(&mut self, version: &AssetVersion) -> Result<(), MetadataError>;
    fn exact_version(&mut self, id: &str) -> Result<Option<AssetVersion>, MetadataError>;
    fn register_reference(&mut self, reference: &ComponentReference) -> Result<(), MetadataError>;
    fn exact_references(&mut self, owner: &str) -> Result<Vec<ComponentReference>, MetadataError>;
}

pub(crate) trait Feedback {
    fn append(
        &mut self,
        feedback: &super::domain::Feedback,
    ) -> Result<super::domain::FeedbackRecord, MetadataError>;
    fn history(
        &mut self,
        query: super::domain::HistoryQuery<'_>,
    ) -> Result<Vec<super::domain::FeedbackRecord>, MetadataError>;
    fn by_id(
        &mut self,
        asset_id: &str,
        version_id: &str,
        feedback_id: &str,
    ) -> Result<Option<super::domain::FeedbackRecord>, MetadataError>;
}
