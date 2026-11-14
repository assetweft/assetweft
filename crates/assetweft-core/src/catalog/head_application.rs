//! Responsibility: exact current-head query and identity policy.
//! Dependencies: Catalog values and HeadQuery only; callers: composition and borrowed adapters.
//! Invariants: identity rejection precedes schema/query errors; current value is unchanged.
use super::{
    domain::{Current, MetadataError},
    head_ports::HeadQuery,
};
fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && id != "."
        && id != ".."
}
pub(crate) fn current<P: HeadQuery>(
    port: &mut P,
    asset: &str,
) -> Result<Option<Current>, MetadataError> {
    if !id_ok(asset) {
        return Err(MetadataError::InvalidInput("asset identity"));
    }
    port.exact_current(asset)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Query(Vec<String>);
    impl HeadQuery for Query {
        fn exact_current(&mut self, asset: &str) -> Result<Option<Current>, MetadataError> {
            self.0.push(asset.into());
            Err(MetadataError::Uninitialized)
        }
    }
    #[test]
    fn identity_precedes_query_and_query_error_is_preserved() {
        let mut query = Query(vec![]);
        for id in ["", ".", "..", "a/b", "a b", &"x".repeat(257)] {
            assert_eq!(
                current(&mut query, id),
                Err(MetadataError::InvalidInput("asset identity"))
            );
        }
        assert!(query.0.is_empty());
        assert_eq!(
            current(&mut query, "asset-1.a_b"),
            Err(MetadataError::Uninitialized)
        );
        assert_eq!(query.0, ["asset-1.a_b"]);
    }
}
