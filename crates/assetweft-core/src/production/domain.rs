//! Responsibility: Production candidate descriptors and acceptance wire values.
//! Dependencies: serde and stable Catalog values; callers: acceptance use case and compatibility facade.
//! Invariants: existing fields, derives and wire representation remain unchanged.
use crate::catalog::domain::{Current, FixtureContract};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentSnapshot {
    pub role: String,
    pub version: String,
    pub measured_sha256: String,
}

/// Claims stay in the referenced immutable M1 version; this is only byte evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub base_version: String,
    pub candidate_version: String,
    pub expected_revision: i64,
    pub editable: Vec<String>,
    pub base: Vec<ComponentSnapshot>,
    pub proposed: Vec<ComponentSnapshot>,
    pub source: String,
    pub scope: String,
    pub not_run: Vec<String>,
    #[serde(default)]
    pub contract: Option<FixtureContract>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentEvidence {
    pub role: String,
    pub version_id: String,
    pub sha256: String,
    pub checker_version: String,
    pub requirement: String,
    pub format_status: String,
    pub role_status: String,
    pub not_run: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceReport {
    pub scope: String,
    pub source: String,
    pub components: Vec<ComponentEvidence>,
    pub not_run: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Acceptance {
    NotRun {
        candidate: Candidate,
        reason: String,
        report: Option<AcceptanceReport>,
    },
    Rejected {
        candidate: Candidate,
        report: AcceptanceReport,
    },
    Accepted {
        candidate: Candidate,
        current: Current,
        report: AcceptanceReport,
    },
    Conflict {
        candidate: Candidate,
        current: Current,
        report: AcceptanceReport,
    },
}

pub(crate) fn report_passes(report: &AcceptanceReport, count: usize) -> bool {
    report.components.len() == count
        && report
            .components
            .iter()
            .all(|r| r.format_status == "PASS" && r.role_status == "PASS")
}
