use super::geometry_domain::GeometryFacts;
pub(crate) trait GeometryStructure {
    fn parse(&self, bytes: &[u8]) -> Result<GeometryFacts, String>;
}
