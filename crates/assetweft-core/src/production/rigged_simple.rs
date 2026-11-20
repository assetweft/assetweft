//! Local structural eligibility for the pinned original and idle/strike derivative.
//! Production compares frozen projections; the adapter measures Catalog-managed bytes.
use super::{
    domain::{AcceptanceReport, Candidate, ComponentEvidence, ComponentSnapshot},
    ports::AcceptanceError,
};
use crate::{
    assurance::rig_blender::RIGGED_SIMPLE, catalog::domain::FixtureContract,
    motion::components::IDLE_STRIKE,
};

pub const STRUCTURE: &str = "RIGGED_SIMPLE_MOTION_STRUCTURE_V1";
pub(crate) const BYTE_LIMIT: u64 = 16 * 1024 * 1024;

pub(crate) fn check_contract(c: &FixtureContract) -> Result<(), AcceptanceError> {
    if c.contract_id != STRUCTURE
        || c.max_component_bytes != BYTE_LIMIT
        || !c.required_quality.is_empty()
    {
        return Err(AcceptanceError::InvalidInput(
            "Rigged Simple structural contract requires fixed byte limit and no quality claim",
        ));
    }
    Ok(())
}

pub(crate) struct CheckedOwner {
    pub asset_id: String,
    pub parts: Vec<ComponentSnapshot>,
    pub structure: ComponentEvidence,
}

pub(crate) trait InspectOwner {
    fn inspect_owner(&mut self, version: &str, pin: &str) -> Result<CheckedOwner, AcceptanceError>;
}

pub(crate) fn check_base(
    port: &mut impl InspectOwner,
    version: &str,
) -> Result<CheckedOwner, AcceptanceError> {
    let owner = port.inspect_owner(version, RIGGED_SIMPLE)?;
    if owner.structure.format_status != "PASS" || owner.structure.role_status != "PASS" {
        return Err(AcceptanceError::InvalidInput(
            "Rigged Simple base GLB structure/role validation",
        ));
    }
    Ok(owner)
}

pub(crate) fn check_pair(
    port: &mut impl InspectOwner,
    c: &Candidate,
) -> Result<AcceptanceReport, AcceptanceError> {
    if c.scope != STRUCTURE || c.editable != ["motion"] || c.source != "PROCEDURAL" {
        return Err(AcceptanceError::InvalidInput("Rigged Simple frozen scope"));
    }
    let base = port.inspect_owner(&c.base_version, RIGGED_SIMPLE)?;
    let proposed = port.inspect_owner(&c.candidate_version, IDLE_STRIKE)?;
    if base.asset_id != proposed.asset_id
        || base.parts != c.base
        || proposed.parts != c.proposed
        || base
            .parts
            .iter()
            .filter(|s| s.role != "motion")
            .ne(proposed.parts.iter().filter(|s| s.role != "motion"))
    {
        return Err(AcceptanceError::InvalidInput(
            "Rigged Simple frozen component mismatch",
        ));
    }
    Ok(AcceptanceReport {
        scope: STRUCTURE.into(),
        source: "PINNED_MANAGED_GLB_PAIR".into(),
        components: vec![base.structure, proposed.structure],
        not_run: [
            "VISUAL_AND_PRODUCT_QUALITY",
            "BLENDER_PLAYBACK",
            "ENGINE_RUNTIME",
            "SEMANTIC",
            "AUTHORIZATION_AND_LICENSE",
            "WORKFLOW_SCHEDULING",
            "PRODUCTION",
        ]
        .map(String::from)
        .into(),
    })
}
