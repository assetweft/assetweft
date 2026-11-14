//! Responsibility: proposal/read use cases and sealed internally produced descriptor.
//! Dependencies: staging domain and semantic capability only; callers: Application and the Production acceptance adapter.
//! Invariants: input policy precedes IO, exact owner checks precede measurement, lock policy precedes freeze.
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
    let owners = port.owners(&p.base_version, &p.candidate_version)?;
    if owners.base_source != "TEST_FIXTURE" || owners.proposed_source != "TEST_FIXTURE" {
        return Err(StagingError::InvalidInput(
            "only declared TEST_FIXTURE staging supported",
        ));
    }
    if owners.base_asset != owners.proposed_asset {
        return Err(StagingError::InvalidInput("candidate asset"));
    }
    let base = port.snapshot(&p.base_version)?;
    let proposed = port.snapshot(&p.candidate_version)?;
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
    port.candidate(id)
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
        changed: bool,
        source: String,
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
        fn snapshot(&mut self, owner: &str) -> Result<Vec<ComponentSnapshot>, StagingError> {
            self.calls.push(owner.into());
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
            Ok(None)
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
            changed: false,
            source: "TEST_FIXTURE".into(),
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
}
