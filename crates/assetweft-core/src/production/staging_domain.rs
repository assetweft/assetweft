//! Responsibility: fixture proposal values and pure descriptor/lock policy.
//! Dependencies: stable Catalog values, LockMode and Production errors; callers: staging use case/adapter.
//! Invariants: byte-only fixture scope, exact snapshot order and unchanged Candidate encoding.
use super::{
    domain::{Candidate, ComponentSnapshot},
    staging_ports::StagingError,
};
use crate::{
    LockMode,
    catalog::domain::{ComponentRole, FixtureContract},
};
fn invalid(s: &'static str) -> StagingError {
    StagingError::InvalidInput(s)
}
#[derive(Debug, Clone)]
pub struct Proposal {
    pub id: String,
    pub base_version: String,
    pub candidate_version: String,
    pub expected_revision: i64,
    pub editable: Vec<ComponentRole>,
    pub lock_mode: LockMode,
    pub contract: Option<FixtureContract>,
}

pub(crate) fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && id != "."
        && id != ".."
}
pub(crate) fn check_contract(c: &FixtureContract) -> Result<(), StagingError> {
    if c.contract_id != "TEST_FIXTURE_GLB_STRUCTURE_V1"
        || c.max_component_bytes == 0
        || c.max_component_bytes > 16 * 1024 * 1024
        || c.required_quality.len() > 32
        || c.required_quality.iter().any(|s| !id_ok(s))
    {
        return Err(invalid("fixture contract limits"));
    }
    Ok(())
}
pub(crate) fn editable(p: &Proposal) -> Result<Vec<String>, StagingError> {
    if ![&p.id, &p.base_version, &p.candidate_version]
        .iter()
        .all(|v| id_ok(v))
        || p.base_version == p.candidate_version
    {
        return Err(invalid("candidate identity"));
    }
    if p.expected_revision < 0 || p.expected_revision == i64::MAX {
        return Err(invalid("revision overflow"));
    }
    if p.lock_mode != LockMode::Byte {
        return Err(invalid("SEMANTIC NOT_RUN"));
    }
    if p.editable.is_empty() || p.editable.len() > 7 {
        return Err(invalid("editable roles"));
    }
    if let Some(contract) = &p.contract {
        check_contract(contract)?;
    }
    let mut editable = Vec::new();
    for role in &p.editable {
        let role = role.as_str().to_owned();
        if editable.contains(&role) {
            return Err(invalid("duplicate editable role"));
        }
        editable.push(role);
    }
    Ok(editable)
}
pub(crate) fn descriptor(
    p: &Proposal,
    editable: Vec<String>,
    base: Vec<ComponentSnapshot>,
    proposed: Vec<ComponentSnapshot>,
) -> Result<Candidate, StagingError> {
    if base.len() != proposed.len() || editable.iter().any(|r| !base.iter().any(|s| &s.role == r)) {
        return Err(invalid("role set"));
    }
    for before in &base {
        let after = proposed
            .iter()
            .find(|s| s.role == before.role)
            .ok_or_else(|| invalid("missing role"))?;
        if !editable.contains(&before.role) && before != after {
            return Err(invalid("locked component changed"));
        }
    }
    let candidate = Candidate {
        id: p.id.clone(),
        base_version: p.base_version.clone(),
        candidate_version: p.candidate_version.clone(),
        expected_revision: p.expected_revision,
        editable,
        base,
        proposed,
        source: "TEST_FIXTURE".into(),
        scope: "MANAGED_COMPONENT_BYTE_STAGING_ONLY".into(),
        contract: p.contract.clone(),
        not_run: vec![
            "FORMAT_AND_ROLE_VALIDATION".into(),
            "QUALITY".into(),
            "SEMANTIC".into(),
            "AUTHORIZATION_AND_LICENSE".into(),
            "CURRENT_REVISION_ACCEPTANCE".into(),
        ],
    };
    Ok(candidate)
}

/// Validate facts at their original observation point; never promote declaration to rights evidence.
pub(crate) fn component_count(count: usize) -> Result<(), StagingError> {
    if count == 0 || count > 7 {
        return Err(invalid("component count"));
    }
    Ok(())
}
pub(crate) fn component_identity(
    result: &[ComponentSnapshot],
    role: &str,
    version: &str,
) -> Result<(), StagingError> {
    if result.iter().any(|s| s.role == role) {
        return Err(invalid("duplicate role"));
    }
    if !id_ok(version) {
        return Err(invalid("component identity"));
    }
    Ok(())
}
pub(crate) fn component_source(source: &str) -> Result<(), StagingError> {
    if source != "TEST_FIXTURE" {
        return Err(invalid("only declared TEST_FIXTURE staging supported"));
    }
    Ok(())
}
