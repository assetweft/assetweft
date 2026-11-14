//! Responsibility: pure protected snapshot values and bounded integrity policy.
//! Dependencies: serde and duration values; callers: backup use cases and adapters.
//! Invariants: format 1/schema 6 only; registered identities never imply available bytes.
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Operation limits. Deadline/cancellation are cooperative between bounded IO chunks;
/// callers must return promptly from callbacks. OS filesystem stalls are not preempted.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_bytes: u64,
    pub max_objects: usize,
    pub timeout: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024 * 1024,
            max_objects: 100_000,
            timeout: Duration::from_secs(120),
        }
    }
}
/// Progress is also a cancellation boundary; returning false cancels without a manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Snapshot,
    Bytes(u64),
    ObjectsReady,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Object {
    pub hash: String,
    pub bytes: u64,
}
/// Database digest binds all asset/version identities, references, history and metadata.
/// Object list includes all registered objects (also retained orphans), not external metadata.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub schema: i64,
    pub producer: String,
    pub database: Object,
    pub objects: Vec<Object>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PolicyError {
    Invalid(&'static str),
    Limit,
}
pub(crate) fn hash_ok(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn validate_manifest(m: &Manifest, limits: Limits) -> Result<(), PolicyError> {
    if m.format != 1 || m.schema != 6 || m.objects.len() > limits.max_objects {
        return Err(PolicyError::Invalid("unsupported format/schema or count"));
    }
    Ok(())
}
pub(crate) fn object_identity(hash: &str, bytes: i64) -> Result<(), PolicyError> {
    if !hash_ok(hash) || bytes < 0 {
        return Err(PolicyError::Invalid("object identity"));
    }
    Ok(())
}
pub(crate) fn account_bytes(current: u64, next: u64, max: u64) -> Result<u64, PolicyError> {
    let total = current.checked_add(next).ok_or(PolicyError::Limit)?;
    if total > max {
        return Err(PolicyError::Limit);
    }
    Ok(total)
}
