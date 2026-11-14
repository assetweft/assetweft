//! Responsibility: managed-object commit/read/protect/collect use cases.
//! Dependencies: Catalog domain and purpose-owned ManagedObjects only; callers: concrete composition.
//! Invariants: metadata/limits before byte work; verified staging before atomic publication; invalid hash never calls retention.
use super::{
    domain::{self, AssetVersion},
    objects_domain::{self as policy, Collection, ObjectPolicyError, StoredObject},
    objects_ports::ManagedObjects,
};

pub(crate) fn commit<P: ManagedObjects>(
    port: &mut P,
    version: &AssetVersion,
    expected_len: u64,
    max_bytes: u64,
) -> Result<StoredObject, P::Error> {
    domain::validate_version(version).map_err(ObjectPolicyError::Metadata)?;
    policy::limits(expected_len, max_bytes)?;
    let staged = port.stage(version, expected_len)?;
    port.publish(version, staged)
}
pub(crate) fn managed<P: ManagedObjects>(
    port: &mut P,
    id: &str,
) -> Result<Option<StoredObject>, P::Error> {
    port.managed(id)
}
pub(crate) fn protect<P: ManagedObjects>(
    port: &mut P,
    hash: &str,
    backup_pin: bool,
    unsynced_unique: bool,
) -> Result<(), P::Error> {
    validate_hash(hash)?;
    port.protect(hash, backup_pin, unsynced_unique)
}
pub(crate) fn collect<P: ManagedObjects>(port: &mut P, hash: &str) -> Result<Collection, P::Error> {
    validate_hash(hash)?;
    port.collect(hash)
}
fn validate_hash(hash: &str) -> Result<(), ObjectPolicyError> {
    if !policy::hash_ok(hash) {
        return Err(ObjectPolicyError::Invalid("hash"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::Presence;
    #[derive(Default)]
    struct Memory {
        calls: Vec<&'static str>,
        fail_stage: bool,
    }
    impl ManagedObjects for Memory {
        type Error = ObjectPolicyError;
        type Staged = StoredObject;
        fn stage(&mut self, v: &AssetVersion, len: u64) -> Result<StoredObject, Self::Error> {
            self.calls.push("stage");
            if self.fail_stage {
                return Err(ObjectPolicyError::Corrupt);
            }
            Ok(StoredObject {
                content_hash: v.content_hash.clone(),
                byte_length: len,
            })
        }
        fn publish(
            &mut self,
            _: &AssetVersion,
            staged: StoredObject,
        ) -> Result<StoredObject, Self::Error> {
            self.calls.push("publish");
            Ok(staged)
        }
        fn managed(&mut self, _: &str) -> Result<Option<StoredObject>, Self::Error> {
            self.calls.push("managed");
            Ok(None)
        }
        fn protect(&mut self, _: &str, pin: bool, unique: bool) -> Result<(), Self::Error> {
            assert!(pin);
            assert!(!unique);
            self.calls.push("protect");
            Ok(())
        }
        fn collect(&mut self, _: &str) -> Result<Collection, Self::Error> {
            self.calls.push("collect");
            Ok(Collection::Protected)
        }
    }
    fn version() -> AssetVersion {
        AssetVersion {
            asset_id: "a".into(),
            version_id: "v".into(),
            content_hash: "a".repeat(64),
            input_id: "i".into(),
            source: "s".into(),
            authorization: "d".into(),
            authorized: false,
            license_id: "l".into(),
            usage_scope: "local".into(),
            source_format: "BIN".into(),
            contract_version: "1.0.0".into(),
            presence: Presence::Unknown,
        }
    }
    #[test]
    fn validated_staging_precedes_publication_and_failure_stops_registration() {
        let mut p = Memory::default();
        assert_eq!(commit(&mut p, &version(), 7, 8).unwrap().byte_length, 7);
        assert_eq!(p.calls, ["stage", "publish"]);
        p.calls.clear();
        p.fail_stage = true;
        assert_eq!(
            commit(&mut p, &version(), 7, 8),
            Err(ObjectPolicyError::Corrupt)
        );
        assert_eq!(p.calls, ["stage"]);
    }
    #[test]
    fn metadata_and_limit_rejection_precede_all_byte_work() {
        let mut p = Memory::default();
        let mut v = version();
        v.asset_id.clear();
        assert!(matches!(
            commit(&mut p, &v, 9, 8),
            Err(ObjectPolicyError::Metadata(_))
        ));
        assert_eq!(
            commit(&mut p, &version(), 9, 8),
            Err(ObjectPolicyError::Invalid("byte limits"))
        );
        assert_eq!(
            commit(&mut p, &version(), 0, u64::MAX),
            Err(ObjectPolicyError::Invalid("byte limits"))
        );
        assert!(p.calls.is_empty());
    }
    #[test]
    fn invalid_hash_never_calls_retention_and_metadata_only_read_stays_absent() {
        let mut p = Memory::default();
        assert_eq!(
            collect(&mut p, "bad"),
            Err(ObjectPolicyError::Invalid("hash"))
        );
        assert_eq!(
            protect(&mut p, "bad", true, false),
            Err(ObjectPolicyError::Invalid("hash"))
        );
        assert!(p.calls.is_empty());
        assert_eq!(managed(&mut p, "metadata-only").unwrap(), None);
        protect(&mut p, &"a".repeat(64), true, false).unwrap();
        assert_eq!(
            collect(&mut p, &"a".repeat(64)).unwrap(),
            Collection::Protected
        );
        assert_eq!(p.calls, ["managed", "protect", "collect"]);
    }
    #[test]
    fn streamed_input_and_transaction_snapshot_policy_keep_original_outcomes() {
        assert_eq!(
            policy::complete(9, 8),
            Err(ObjectPolicyError::Invalid("excess input"))
        );
        assert_eq!(
            policy::complete(7, 8),
            Err(ObjectPolicyError::Invalid("incomplete input"))
        );
        assert_eq!(policy::complete(8, 8), Ok(()));
        assert_eq!(
            policy::matching_hash("a", "b"),
            Err(ObjectPolicyError::Corrupt)
        );
        assert_eq!(policy::collection_policy(true), Some(Collection::Protected));
        assert_eq!(policy::collection_policy(false), None);
    }
}
