//! Responsibility: register immutable metadata and query exact version/reference snapshots.
//! Dependencies: Catalog domain validation and its purpose-based metadata capability.
//! Callers: compatibility Application; invariant: reject malformed identities before persistence.
use super::{
    domain::{self, AssetVersion, ComponentReference, MetadataError},
    ports::Metadata,
};

pub(crate) fn register_version(
    port: &mut impl Metadata,
    v: &AssetVersion,
) -> Result<(), MetadataError> {
    domain::validate_version(v)?;
    port.register_version(v)
}
pub(crate) fn exact_version(
    port: &mut impl Metadata,
    id: &str,
) -> Result<Option<AssetVersion>, MetadataError> {
    domain::validate_version_id(id)?;
    port.exact_version(id)
}
pub(crate) fn register_reference(
    port: &mut impl Metadata,
    r: &ComponentReference,
) -> Result<(), MetadataError> {
    domain::validate_reference(r)?;
    port.register_reference(r)
}
pub(crate) fn exact_references(
    port: &mut impl Metadata,
    id: &str,
) -> Result<Vec<ComponentReference>, MetadataError> {
    domain::validate_version_id(id)?;
    port.exact_references(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{ComponentRole, Presence};
    struct Memory {
        version: AssetVersion,
        references: Vec<ComponentReference>,
        calls: Vec<&'static str>,
    }
    impl Metadata for Memory {
        fn register_version(&mut self, v: &AssetVersion) -> Result<(), MetadataError> {
            self.calls.push("register version");
            self.version = v.clone();
            Ok(())
        }
        fn exact_version(&mut self, id: &str) -> Result<Option<AssetVersion>, MetadataError> {
            self.calls.push("exact version");
            Ok((self.version.version_id == id).then(|| self.version.clone()))
        }
        fn register_reference(&mut self, r: &ComponentReference) -> Result<(), MetadataError> {
            self.calls.push("register reference");
            self.references.push(r.clone());
            Ok(())
        }
        fn exact_references(&mut self, _: &str) -> Result<Vec<ComponentReference>, MetadataError> {
            self.calls.push("exact references");
            Ok(self.references.clone())
        }
    }
    fn memory() -> Memory {
        Memory {
            version: AssetVersion {
                asset_id: "asset".into(),
                version_id: "version".into(),
                content_hash: "a".repeat(64),
                input_id: "input".into(),
                source: "declared source".into(),
                authorization: "declaration".into(),
                authorized: false,
                license_id: "license".into(),
                usage_scope: "test".into(),
                source_format: "glb".into(),
                contract_version: "1.0.0".into(),
                presence: Presence::Missing,
            },
            references: vec![],
            calls: vec![],
        }
    }
    #[test]
    fn four_use_cases_exchange_exact_business_values_without_io() {
        let mut m = memory();
        let v = m.version.clone();
        register_version(&mut m, &v).unwrap();
        assert_eq!(exact_version(&mut m, "version").unwrap(), Some(v));
        assert_eq!(exact_version(&mut m, "another").unwrap(), None);
        let r = ComponentReference {
            owner_version_id: "version".into(),
            role: ComponentRole::Mesh,
            component_version_id: "mesh-v2".into(),
        };
        register_reference(&mut m, &r).unwrap();
        assert_eq!(exact_references(&mut m, "version").unwrap(), vec![r]);
        assert_eq!(
            m.calls,
            [
                "register version",
                "exact version",
                "exact version",
                "register reference",
                "exact references"
            ]
        );
    }
    #[test]
    fn invalid_metadata_never_reaches_persistence_and_preserves_validation_precedence() {
        let mut m = memory();
        let mut v = m.version.clone();
        v.contract_version = "future".into();
        v.asset_id.clear();
        assert_eq!(
            register_version(&mut m, &v),
            Err(MetadataError::UnknownValue("future".into()))
        );
        v.contract_version = "1.0.0".into();
        assert_eq!(
            register_version(&mut m, &v),
            Err(MetadataError::InvalidInput("identity"))
        );
        assert_eq!(
            exact_version(&mut m, "../bad"),
            Err(MetadataError::InvalidInput("version identity"))
        );
        assert_eq!(
            exact_references(&mut m, "."),
            Err(MetadataError::InvalidInput("version identity"))
        );
        let r = ComponentReference {
            owner_version_id: "owner".into(),
            role: ComponentRole::Mesh,
            component_version_id: "".into(),
        };
        assert_eq!(
            register_reference(&mut m, &r),
            Err(MetadataError::InvalidInput("version identity"))
        );
        assert!(m.calls.is_empty());
    }
}
