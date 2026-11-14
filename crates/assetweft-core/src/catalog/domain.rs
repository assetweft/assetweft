//! Responsibility: Catalog immutable metadata, exact references and fixture head values and their unchanged serde representation.
//! Dependencies: serde and std only; callers: candidate compatibility facade and Catalog SQLite adapter.
//! Invariants: fixture-only contract; no Production or persistence dependency.
use serde::{Deserialize, Serialize};

/// Predeclared fixture-only contract. Required quality items cannot be satisfied here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureContract {
    /// Must be TEST_FIXTURE_GLB_STRUCTURE_V1; never a production acceptance contract.
    pub contract_id: String,
    pub max_component_bytes: u64,
    pub required_quality: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Current {
    pub asset_id: String,
    pub version_id: String,
    pub revision: i64,
    pub contract: FixtureContract,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MetadataError {
    Busy(String),
    Constraint(String),
    Io(String),
    Database(String),
    FutureSchema(i64),
    Uninitialized,
    InvalidInput(&'static str),
    UnknownValue(String),
}

/// File availability known to this metadata-only repository. No ready state is issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// Presence has not been checked.
    Unknown,
    /// The referenced file is missing.
    Missing,
}
impl Presence {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "UNKNOWN",
            Self::Missing => "MISSING",
        }
    }
    pub(crate) fn parse(value: &str) -> Result<Self, MetadataError> {
        match value {
            "UNKNOWN" => Ok(Self::Unknown),
            "MISSING" => Ok(Self::Missing),
            _ => Err(MetadataError::UnknownValue(value.to_owned())),
        }
    }
}

/// AW-003 component role vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentRole {
    Mesh,
    Materials,
    Rig,
    Motion,
    Vfx,
    Audio,
    Dialogue,
}
impl ComponentRole {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Mesh => "mesh",
            Self::Materials => "materials",
            Self::Rig => "rig",
            Self::Motion => "motion",
            Self::Vfx => "vfx",
            Self::Audio => "audio",
            Self::Dialogue => "dialogue",
        }
    }
    pub(crate) fn parse(value: &str) -> Result<Self, MetadataError> {
        match value {
            "mesh" => Ok(Self::Mesh),
            "materials" => Ok(Self::Materials),
            "rig" => Ok(Self::Rig),
            "motion" => Ok(Self::Motion),
            "vfx" => Ok(Self::Vfx),
            "audio" => Ok(Self::Audio),
            "dialogue" => Ok(Self::Dialogue),
            _ => Err(MetadataError::UnknownValue(value.to_owned())),
        }
    }
}

/// Immutable metadata; recording this does not commit or validate object bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetVersion {
    pub asset_id: String,
    pub version_id: String,
    pub content_hash: String,
    pub input_id: String,
    pub source: String,
    pub authorization: String,
    pub authorized: bool,
    pub license_id: String,
    pub usage_scope: String,
    pub source_format: String,
    /// Version of AW-CONTRACT-001; independent of this database's schema version.
    pub contract_version: String,
    pub presence: Presence,
}

/// An explicit reference to a component version, never an implicit latest pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentReference {
    pub owner_version_id: String,
    pub role: ComponentRole,
    pub component_version_id: String,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && !value.contains(['/', '\\', '\0']) && value != "." && value != ".."
}
pub(crate) fn validate_version(v: &AssetVersion) -> Result<(), MetadataError> {
    if v.contract_version != "1.0.0" {
        return Err(MetadataError::UnknownValue(v.contract_version.clone()));
    }
    if !valid_id(&v.asset_id) || !valid_id(&v.version_id) || !valid_id(&v.input_id) {
        return Err(MetadataError::InvalidInput("identity"));
    }
    if v.content_hash.len() != 64
        || !v
            .content_hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(MetadataError::InvalidInput("content hash"));
    }
    if [
        &v.source,
        &v.authorization,
        &v.license_id,
        &v.usage_scope,
        &v.source_format,
    ]
    .iter()
    .any(|v| v.is_empty())
    {
        return Err(MetadataError::InvalidInput("source or license"));
    }
    Ok(())
}
pub(crate) fn validate_version_id(id: &str) -> Result<(), MetadataError> {
    if !valid_id(id) {
        return Err(MetadataError::InvalidInput("version identity"));
    }
    Ok(())
}
pub(crate) fn validate_reference(r: &ComponentReference) -> Result<(), MetadataError> {
    validate_version_id(&r.owner_version_id)?;
    validate_version_id(&r.component_version_id)
}

// Exact-version feedback values and pure input policy.
pub const MAX_REASON_BYTES: usize = 2048;
pub const MAX_HISTORY_LIMIT: u16 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Accept,
    Reject,
}
impl Decision {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "ACCEPT",
            Self::Reject => "REJECT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feedback {
    pub feedback_id: String,
    pub asset_id: String,
    pub version_id: String,
    pub decision: Decision,
    /// None means no reason; otherwise 1–2048 UTF-8 bytes, nonblank, no NUL.
    pub reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackRecord {
    pub sequence: i64,
    pub feedback: Feedback,
}
/// Newest first, scoped to an asset and optionally an exact version.
/// Continue with before_sequence = last returned sequence; new appends do not shift pages.
#[derive(Debug, Clone, Copy)]
pub struct HistoryQuery<'a> {
    pub asset_id: &'a str,
    pub version_id: Option<&'a str>,
    pub before_sequence: Option<i64>,
    pub limit: u16,
}
fn identity(id: &str) -> bool {
    !id.trim().is_empty()
        && id.len() <= 256
        && !id.contains(['/', '\\', '\0'])
        && id != "."
        && id != ".."
}
pub(crate) fn validate_feedback(f: &Feedback) -> Result<(), MetadataError> {
    if !identity(&f.feedback_id) || !identity(&f.asset_id) || !identity(&f.version_id) {
        return Err(MetadataError::InvalidInput("feedback identity"));
    }
    if f.reason
        .as_ref()
        .is_some_and(|r| r.trim().is_empty() || r.len() > MAX_REASON_BYTES || r.contains('\0'))
    {
        return Err(MetadataError::InvalidInput("feedback reason"));
    }
    Ok(())
}
pub(crate) fn validate_history(q: HistoryQuery<'_>) -> Result<(), MetadataError> {
    if !identity(q.asset_id) || q.version_id.is_some_and(|id| !identity(id)) {
        return Err(MetadataError::InvalidInput("feedback query identity"));
    }
    if q.limit == 0 || q.limit > MAX_HISTORY_LIMIT || q.before_sequence.is_some_and(|s| s <= 0) {
        return Err(MetadataError::InvalidInput("feedback query bounds"));
    }
    Ok(())
}
