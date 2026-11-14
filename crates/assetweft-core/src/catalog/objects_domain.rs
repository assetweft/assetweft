//! Responsibility: managed object values and pure bounded-input/retention policy.
//! Dependencies: Catalog metadata domain only; callers: object use cases and CAS adapter.
//! Invariants: hash is exact lowercase SHA256; any retained reference prevents collection.
use super::domain::MetadataError;

/// A verified managed object, independent of source/license metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredObject {
    pub content_hash: String,
    pub byte_length: u64,
}
/// Explicit result of a requested single-object collection; no background deletion exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collection {
    Protected,
    Removed,
    Absent,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ObjectPolicyError {
    Metadata(MetadataError),
    Invalid(&'static str),
    Corrupt,
}
pub(crate) fn hash_ok(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn limits(expected: u64, max: u64) -> Result<(), ObjectPolicyError> {
    if expected > max || max > i64::MAX as u64 {
        return Err(ObjectPolicyError::Invalid("byte limits"));
    }
    Ok(())
}
pub(crate) fn complete(written: u64, expected: u64) -> Result<(), ObjectPolicyError> {
    if written > expected {
        return Err(ObjectPolicyError::Invalid("excess input"));
    }
    if written != expected {
        return Err(ObjectPolicyError::Invalid("incomplete input"));
    }
    Ok(())
}
pub(crate) fn matching_hash(actual: &str, expected: &str) -> Result<(), ObjectPolicyError> {
    if actual != expected {
        return Err(ObjectPolicyError::Corrupt);
    }
    Ok(())
}
/// Called with the current retained-reference snapshot inside the leased transaction.
pub(crate) fn collection_policy(protected: bool) -> Option<Collection> {
    protected.then_some(Collection::Protected)
}
