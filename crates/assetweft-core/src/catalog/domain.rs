//! Responsibility: Catalog fixture head values and their unchanged serde representation.
//! Dependencies: serde only; callers: candidate compatibility facade and Catalog SQLite adapter.
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
