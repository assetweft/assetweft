//! Responsibility: acceptance ordering and policy, independent of persistence.
//! Dependencies: Production domain/ports and stable Catalog head values.
//! Callers: compatibility Application; invariant: only this controlled path constructs a commit decision.
use super::{
    domain::{Acceptance, AcceptanceReport, Candidate, report_passes},
    ports::{AcceptanceError, AcceptanceInputs, CommitAcceptance},
};
use crate::catalog::domain::Current;

/// Private fields and no serde/constructor prevent public reports from authorizing acceptance.
pub(crate) struct VerifiedAcceptance {
    candidate: Candidate,
    decision: Decision,
}
enum Decision {
    Terminal(Acceptance),
    Eligible(AcceptanceReport),
}
pub(crate) enum CommitPlan {
    Terminal(Acceptance),
    Advance {
        candidate: Candidate,
        report: AcceptanceReport,
    },
}
impl VerifiedAcceptance {
    pub(crate) fn candidate(&self) -> &Candidate {
        &self.candidate
    }
    /// Evaluate the current head observed inside the atomic commit boundary.
    pub(crate) fn resolve(self, head: Option<Current>) -> CommitPlan {
        let report = match self.decision {
            Decision::Terminal(result) => return CommitPlan::Terminal(result),
            Decision::Eligible(report) => report,
        };
        let candidate = self.candidate;
        match head {
            Some(head) if Some(&head.contract) != candidate.contract.as_ref() => {
                CommitPlan::Terminal(Acceptance::NotRun {
                    candidate,
                    reason: "current contract mismatch".into(),
                    report: Some(report),
                })
            }
            Some(_) => CommitPlan::Advance { candidate, report },
            None => CommitPlan::Terminal(Acceptance::NotRun {
                candidate,
                reason: "current not initialized".into(),
                report: Some(report),
            }),
        }
    }
    pub(crate) fn needs_head(&self) -> bool {
        matches!(self.decision, Decision::Eligible(_))
    }
}

pub(crate) fn attempt(
    ports: &mut (impl AcceptanceInputs + CommitAcceptance),
    id: &str,
) -> Result<Acceptance, AcceptanceError> {
    // A durable receipt wins even when subsequent bytes/validation would now fail.
    if let Some(old) = ports.existing_receipt(id)? {
        return Ok(old);
    }
    let candidate = ports
        .candidate(id)?
        .ok_or(AcceptanceError::InvalidInput("missing candidate"))?;
    let pending = match &candidate.contract {
        None => Some("no predeclared GLB structure contract"),
        Some(c) if !c.required_quality.is_empty() => Some("required quality NOT_RUN"),
        _ => None,
    };
    let decision = if let Some(reason) = pending {
        Decision::Terminal(Acceptance::NotRun {
            candidate: candidate.clone(),
            reason: reason.into(),
            report: None,
        })
    } else {
        let report = ports.validate_proposed(
            &candidate.proposed,
            candidate.contract.as_ref().expect("checked contract"),
        )?;
        ports.remeasure_base(&candidate.base)?;
        if report.components.len() != candidate.proposed.len() {
            Decision::Terminal(Acceptance::NotRun {
                candidate: candidate.clone(),
                reason: "unsupported component role/source/format NOT_RUN".into(),
                report: Some(report),
            })
        } else if !report_passes(&report, candidate.proposed.len()) {
            Decision::Terminal(Acceptance::Rejected {
                candidate: candidate.clone(),
                report,
            })
        } else {
            Decision::Eligible(report)
        }
    };
    ports.commit_acceptance(VerifiedAcceptance {
        candidate,
        decision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::domain::FixtureContract;
    use crate::production::domain::ComponentEvidence;

    struct Ports {
        candidate: Candidate,
        receipt: Option<Acceptance>,
        report: AcceptanceReport,
        base_error: Option<AcceptanceError>,
        calls: Vec<&'static str>,
        head: Option<Current>,
    }
    impl AcceptanceInputs for Ports {
        fn existing_receipt(&mut self, _: &str) -> Result<Option<Acceptance>, AcceptanceError> {
            self.calls.push("receipt");
            Ok(self.receipt.clone())
        }
        fn candidate(&mut self, _: &str) -> Result<Option<Candidate>, AcceptanceError> {
            self.calls.push("candidate");
            Ok(Some(self.candidate.clone()))
        }
        fn validate_proposed(
            &mut self,
            _: &[super::super::domain::ComponentSnapshot],
            _: &FixtureContract,
        ) -> Result<AcceptanceReport, AcceptanceError> {
            self.calls.push("validate");
            Ok(self.report.clone())
        }
        fn remeasure_base(
            &mut self,
            _: &[super::super::domain::ComponentSnapshot],
        ) -> Result<(), AcceptanceError> {
            self.calls.push("base");
            match &self.base_error {
                Some(e) => Err(e.clone()),
                None => Ok(()),
            }
        }
    }
    // This substitute proves ordering/policy only, never SQLite atomicity or CAS.
    impl CommitAcceptance for Ports {
        fn commit_acceptance(
            &mut self,
            decision: VerifiedAcceptance,
        ) -> Result<Acceptance, AcceptanceError> {
            self.calls.push("commit");
            match decision.resolve(self.head.clone()) {
                CommitPlan::Terminal(result) => Ok(result),
                CommitPlan::Advance { .. } => panic!("unexpected advance in orchestration test"),
            }
        }
    }
    fn ports() -> Ports {
        let contract = FixtureContract {
            contract_id: "TEST_FIXTURE_GLB_STRUCTURE_V1".into(),
            max_component_bytes: 1024,
            required_quality: vec![],
        };
        let snapshot = super::super::domain::ComponentSnapshot {
            role: "mesh".into(),
            version: "mesh-v1".into(),
            measured_sha256: "a".repeat(64),
        };
        Ports {
            candidate: Candidate {
                id: "candidate".into(),
                base_version: "base".into(),
                candidate_version: "next".into(),
                expected_revision: 0,
                editable: vec!["mesh".into()],
                base: vec![snapshot.clone()],
                proposed: vec![snapshot],
                source: "TEST_FIXTURE".into(),
                scope: "MANAGED_COMPONENT_BYTE_STAGING_ONLY".into(),
                not_run: vec![],
                contract: Some(contract),
            },
            receipt: None,
            report: AcceptanceReport {
                scope: "TEST_FIXTURE_GLB_STRUCTURE_V1".into(),
                source: "TEST_FIXTURE_DECLARATION_UNVERIFIED".into(),
                components: vec![ComponentEvidence {
                    role: "mesh".into(),
                    version_id: "mesh-v1".into(),
                    sha256: "a".repeat(64),
                    checker_version: "test".into(),
                    requirement: "STATIC_MESH".into(),
                    format_status: "PASS".into(),
                    role_status: "PASS".into(),
                    not_run: vec![],
                }],
                not_run: vec!["QUALITY".into()],
            },
            base_error: None,
            calls: vec![],
            head: None,
        }
    }
    #[test]
    fn durable_receipt_precedes_all_new_validation() {
        let mut p = ports();
        let old = Acceptance::NotRun {
            candidate: p.candidate.clone(),
            reason: "stored".into(),
            report: None,
        };
        p.receipt = Some(old.clone());
        p.base_error = Some(AcceptanceError::Io("corrupt now".into()));
        assert_eq!(attempt(&mut p, "candidate").unwrap(), old);
        assert_eq!(p.calls, ["receipt"]);
    }
    #[test]
    fn pending_contract_or_quality_skips_measurement_but_commits_receipt() {
        for quality in [false, true] {
            let mut p = ports();
            if quality {
                p.candidate
                    .contract
                    .as_mut()
                    .unwrap()
                    .required_quality
                    .push("visual".into());
            } else {
                p.candidate.contract = None;
            }
            assert!(matches!(
                attempt(&mut p, "candidate").unwrap(),
                Acceptance::NotRun { report: None, .. }
            ));
            assert_eq!(p.calls, ["receipt", "candidate", "commit"]);
        }
    }
    #[test]
    fn base_remeasurement_failure_prevents_commit_even_after_passing_report() {
        let mut p = ports();
        let failure = AcceptanceError::InvalidInput("base byte drift");
        p.base_error = Some(failure.clone());
        assert_eq!(attempt(&mut p, "candidate"), Err(failure));
        assert_eq!(p.calls, ["receipt", "candidate", "validate", "base"]);
    }
    #[test]
    fn unsupported_and_rejected_reports_remain_distinct() {
        let mut p = ports();
        p.report.components.clear();
        assert!(matches!(
            attempt(&mut p, "candidate").unwrap(),
            Acceptance::NotRun {
                report: Some(_),
                ..
            }
        ));
        let mut p = ports();
        p.report.components[0].role_status = "FAIL".into();
        assert!(matches!(
            attempt(&mut p, "candidate").unwrap(),
            Acceptance::Rejected { .. }
        ));
        assert_eq!(
            p.calls,
            ["receipt", "candidate", "validate", "base", "commit"]
        );
    }
    #[test]
    fn validated_candidate_requires_current_and_exact_contract() {
        let mut p = ports();
        assert!(
            matches!(attempt(&mut p, "candidate").unwrap(), Acceptance::NotRun { reason, .. } if reason == "current not initialized")
        );
        let mut p = ports();
        let mut contract = p.candidate.contract.clone().unwrap();
        contract.max_component_bytes += 1;
        p.head = Some(Current {
            asset_id: "asset".into(),
            version_id: "base".into(),
            revision: 0,
            contract,
        });
        assert!(
            matches!(attempt(&mut p, "candidate").unwrap(), Acceptance::NotRun { reason, .. } if reason == "current contract mismatch")
        );
    }
}
