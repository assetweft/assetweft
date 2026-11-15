//! Responsibility: proposal/read use cases and sealed internally produced descriptor.
//! Dependencies: staging domain and semantic capability only; callers: Application and the Production acceptance adapter.
//! Invariants: input policy precedes IO, exact owner checks precede measurement, lock policy precedes freeze, a supplied id may only name its own descriptor.
use super::{
    domain::Candidate,
    staging_domain::{self, Proposal},
    staging_ports::{Staging, StagingError},
};
/// Only this module produces a descriptor eligible for persistence. This is not acceptance.
pub(crate) struct FrozenProposal {
    candidate: Candidate,
}
impl FrozenProposal {
    pub(crate) fn into_candidate(self) -> Candidate {
        self.candidate
    }
}
pub(crate) fn propose(port: &mut impl Staging, p: &Proposal) -> Result<Candidate, StagingError> {
    let editable = staging_domain::editable(p)?;
    let max_component_bytes = p.contract.as_ref().map(|c| c.max_component_bytes);
    let owners = port.owners(&p.base_version, &p.candidate_version)?;
    if owners.base_source != "TEST_FIXTURE" || owners.proposed_source != "TEST_FIXTURE" {
        return Err(StagingError::InvalidInput(
            "only declared TEST_FIXTURE staging supported",
        ));
    }
    if owners.base_asset != owners.proposed_asset {
        return Err(StagingError::InvalidInput("candidate asset"));
    }
    let base = port.snapshot(&p.base_version, max_component_bytes)?;
    let proposed = port.snapshot(&p.candidate_version, max_component_bytes)?;
    let candidate = staging_domain::descriptor(p, editable, base, proposed)?;
    port.freeze(FrozenProposal { candidate })
}
pub(crate) fn read(
    port: &mut impl super::staging_ports::CandidateRead,
    id: &str,
) -> Result<Option<Candidate>, StagingError> {
    if !staging_domain::id_ok(id) {
        return Err(StagingError::InvalidInput("candidate id"));
    }
    match port.candidate(id)? {
        // The descriptor is the only identity later used for measurement, Catalog
        // advance and receipt rows. A restored row whose embedded id differs from its
        // lookup key cannot be trusted, so fail before any measurement, lease or write.
        Some(candidate) if candidate.id != id => {
            Err(StagingError::InvalidInput("candidate identity"))
        }
        candidate => Ok(candidate),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        domain::ComponentSnapshot,
        staging_ports::{CandidateRead, Owners},
    };
    use super::*;
    use crate::{LockMode, catalog::domain::ComponentRole};
    struct Port {
        calls: Vec<String>,
        limits: Vec<Option<u64>>,
        changed: bool,
        source: String,
        stored: Option<Candidate>,
    }
    impl Staging for Port {
        fn owners(&mut self, _: &str, _: &str) -> Result<Owners, StagingError> {
            self.calls.push("owners".into());
            Ok(Owners {
                base_source: self.source.clone(),
                proposed_source: "TEST_FIXTURE".into(),
                base_asset: "asset".into(),
                proposed_asset: "asset".into(),
            })
        }
        fn snapshot(
            &mut self,
            owner: &str,
            max_component_bytes: Option<u64>,
        ) -> Result<Vec<ComponentSnapshot>, StagingError> {
            self.calls.push(owner.into());
            self.limits.push(max_component_bytes);
            Ok(vec![
                ComponentSnapshot {
                    role: "mesh".into(),
                    version: "mesh".into(),
                    measured_sha256: "a".repeat(64),
                },
                ComponentSnapshot {
                    role: "rig".into(),
                    version: if self.changed && owner == "next" {
                        "changed"
                    } else {
                        "rig"
                    }
                    .into(),
                    measured_sha256: "b".repeat(64),
                },
            ])
        }
        fn freeze(&mut self, p: FrozenProposal) -> Result<Candidate, StagingError> {
            self.calls.push("freeze".into());
            Ok(p.into_candidate())
        }
    }
    impl CandidateRead for Port {
        fn candidate(&mut self, _: &str) -> Result<Option<Candidate>, StagingError> {
            self.calls.push("read".into());
            Ok(self.stored.clone())
        }
    }
    fn proposal() -> Proposal {
        Proposal {
            id: "id".into(),
            base_version: "base".into(),
            candidate_version: "next".into(),
            expected_revision: 0,
            editable: vec![ComponentRole::Mesh],
            lock_mode: LockMode::Byte,
            contract: None,
        }
    }
    fn port() -> Port {
        Port {
            calls: vec![],
            limits: vec![],
            changed: false,
            source: "TEST_FIXTURE".into(),
            stored: None,
        }
    }
    #[test]
    fn policy_errors_precede_external_reads() {
        let mut p = proposal();
        let mut io = port();
        p.expected_revision = i64::MAX;
        assert_eq!(
            propose(&mut io, &p),
            Err(StagingError::InvalidInput("revision overflow"))
        );
        assert!(io.calls.is_empty());
        assert_eq!(
            read(&mut io, "../bad"),
            Err(StagingError::InvalidInput("candidate id"))
        );
        assert!(io.calls.is_empty());
    }
    #[test]
    fn source_scope_precedes_measurement_and_locked_change_never_freezes() {
        let mut io = port();
        io.source = "PRODUCTION".into();
        assert_eq!(
            propose(&mut io, &proposal()),
            Err(StagingError::InvalidInput(
                "only declared TEST_FIXTURE staging supported"
            ))
        );
        assert_eq!(io.calls, ["owners"]);
        let mut io = port();
        io.changed = true;
        assert_eq!(
            propose(&mut io, &proposal()),
            Err(StagingError::InvalidInput("locked component changed"))
        );
        assert_eq!(io.calls, ["owners", "base", "next"]);
    }
    #[test]
    fn exact_descriptor_is_produced_before_freeze_with_no_acceptance_claim() {
        let mut io = port();
        let c = propose(&mut io, &proposal()).unwrap();
        assert_eq!(io.calls, ["owners", "base", "next", "freeze"]);
        assert_eq!(io.limits, [None, None]);
        assert_eq!(
            c.base.iter().map(|s| s.role.as_str()).collect::<Vec<_>>(),
            ["mesh", "rig"]
        );
        assert_eq!(c.editable, ["mesh"]);
        assert_eq!(c.scope, "MANAGED_COMPONENT_BYTE_STAGING_ONLY");
        assert_eq!(
            c.not_run,
            [
                "FORMAT_AND_ROLE_VALIDATION",
                "QUALITY",
                "SEMANTIC",
                "AUTHORIZATION_AND_LICENSE",
                "CURRENT_REVISION_ACCEPTANCE"
            ]
        );
        assert_eq!(read(&mut io, "id").unwrap(), None);
        assert_eq!(io.calls.last().unwrap(), "read");
    }
    #[test]
    fn declared_contract_bound_is_forwarded_to_both_snapshots() {
        let mut io = port();
        let mut p = proposal();
        p.contract = Some(crate::catalog::domain::FixtureContract {
            contract_id: "TEST_FIXTURE_GLB_STRUCTURE_V1".into(),
            max_component_bytes: 777,
            required_quality: vec![],
        });
        propose(&mut io, &p).unwrap();
        assert_eq!(io.limits, [Some(777), Some(777)]);
    }
    #[test]
    fn stored_descriptor_identity_must_equal_the_lookup_key() {
        let mut io = port();
        let mut mismatched = propose(&mut port(), &proposal()).unwrap();
        mismatched.id = "other".into();
        io.stored = Some(mismatched);
        assert_eq!(
            read(&mut io, "id"),
            Err(StagingError::InvalidInput("candidate identity"))
        );
        assert_eq!(io.calls, ["read"]);
        let mut io = port();
        let matched = propose(&mut port(), &proposal()).unwrap();
        io.stored = Some(matched.clone());
        assert_eq!(read(&mut io, "id").unwrap(), Some(matched));
        assert_eq!(io.calls, ["read"]);
    }
}
