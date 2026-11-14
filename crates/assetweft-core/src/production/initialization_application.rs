//! Responsibility: fixture bootstrap policy/orchestration and acceptance receipt read use case.
//! Dependencies: Production policy/values and own semantic ports; callers: legacy composition facade.
//! Invariants: identity→contract→quality→owner→snapshot→real validation→sealed commit; no IO types.
use super::{
    domain::{Acceptance, ComponentSnapshot, report_passes},
    initialization_ports::{
        AttemptRead, Current, FixtureContract, Initialization, InitializationError,
    },
    staging_domain::{check_contract, id_ok},
};

/// Only this use case constructs the proof, after measured snapshot validation.
/// It is neither cloneable nor deserializable and is consumed by the adapter once.
pub(crate) struct VerifiedInitialization<'a> {
    asset_id: String,
    version: &'a str,
    contract: &'a FixtureContract,
    snapshots: Vec<ComponentSnapshot>,
}
impl<'a> VerifiedInitialization<'a> {
    pub(crate) fn into_parts(
        self,
    ) -> (String, &'a str, &'a FixtureContract, Vec<ComponentSnapshot>) {
        (self.asset_id, self.version, self.contract, self.snapshots)
    }
}
pub(crate) fn initialize(
    port: &mut impl Initialization,
    version: &str,
    contract: &FixtureContract,
) -> Result<Current, InitializationError> {
    if !id_ok(version) {
        return Err(InitializationError::InvalidInput("version identity"));
    }
    check_contract(contract)?;
    if !contract.required_quality.is_empty() {
        return Err(InitializationError::InvalidInput(
            "required quality NOT_RUN",
        ));
    }
    let owner = port.base_owner(version)?;
    if owner.source != "TEST_FIXTURE" || !id_ok(&owner.asset_id) {
        return Err(InitializationError::InvalidInput(
            "fixture base source/asset",
        ));
    }
    let snapshots = port.snapshot(version)?;
    let report = port.validate(&snapshots, contract)?;
    if !report_passes(&report, snapshots.len()) {
        return Err(InitializationError::InvalidInput(
            "base GLB structure/role validation",
        ));
    }
    port.initialize(VerifiedInitialization {
        asset_id: owner.asset_id,
        version,
        contract,
        snapshots,
    })
}
pub(crate) fn read(
    port: &mut impl AttemptRead,
    id: &str,
) -> Result<Option<Acceptance>, InitializationError> {
    if !id_ok(id) {
        return Err(InitializationError::InvalidInput("candidate id"));
    }
    port.receipt(id)
}

#[cfg(test)]
mod tests {
    use super::super::{
        domain::{AcceptanceReport, ComponentEvidence},
        initialization_ports::BaseOwner,
    };
    use super::*;

    struct Port {
        calls: Vec<String>,
        source: &'static str,
        asset: &'static str,
        fail: &'static str,
        status: &'static str,
        missing: bool,
    }
    impl Port {
        fn call(&mut self, name: &str) -> Result<(), InitializationError> {
            self.calls.push(name.into());
            if self.fail == name {
                Err(InitializationError::Io("observation failed".into()))
            } else {
                Ok(())
            }
        }
    }
    fn port() -> Port {
        Port {
            calls: vec![],
            source: "TEST_FIXTURE",
            asset: "asset",
            fail: "",
            status: "PASS",
            missing: false,
        }
    }
    fn contract() -> FixtureContract {
        FixtureContract {
            contract_id: "TEST_FIXTURE_GLB_STRUCTURE_V1".into(),
            max_component_bytes: 1024,
            required_quality: vec![],
        }
    }
    fn snapshots() -> Vec<ComponentSnapshot> {
        vec![ComponentSnapshot {
            role: "mesh".into(),
            version: "component".into(),
            measured_sha256: "a".repeat(64),
        }]
    }
    impl Initialization for Port {
        fn base_owner(&mut self, version: &str) -> Result<BaseOwner, InitializationError> {
            assert_eq!(version, "base");
            self.call("owner")?;
            Ok(BaseOwner {
                asset_id: self.asset.into(),
                source: self.source.into(),
            })
        }
        fn snapshot(
            &mut self,
            version: &str,
        ) -> Result<Vec<ComponentSnapshot>, InitializationError> {
            assert_eq!(version, "base");
            self.call("snapshot")?;
            Ok(snapshots())
        }
        fn validate(
            &mut self,
            values: &[ComponentSnapshot],
            c: &FixtureContract,
        ) -> Result<AcceptanceReport, InitializationError> {
            assert_eq!(values, snapshots());
            assert_eq!(c, &contract());
            self.call("validate")?;
            Ok(AcceptanceReport {
                scope: "fixture".into(),
                source: "declared".into(),
                not_run: vec!["QUALITY".into()],
                components: if self.missing {
                    vec![]
                } else {
                    vec![ComponentEvidence {
                        role: "mesh".into(),
                        version_id: "component".into(),
                        sha256: "a".repeat(64),
                        checker_version: "test".into(),
                        requirement: "mesh".into(),
                        format_status: "PASS".into(),
                        role_status: self.status.into(),
                        not_run: vec![],
                    }]
                },
            })
        }
        fn initialize(
            &mut self,
            proof: VerifiedInitialization<'_>,
        ) -> Result<Current, InitializationError> {
            let (asset_id, version, c, measured) = proof.into_parts();
            assert_eq!(asset_id, "asset");
            assert_eq!(version, "base");
            assert_eq!(c, &contract());
            assert_eq!(measured, snapshots());
            self.call("commit")?;
            Ok(Current {
                asset_id,
                version_id: version.into(),
                revision: 0,
                contract: c.clone(),
            })
        }
    }
    impl AttemptRead for Port {
        fn receipt(&mut self, id: &str) -> Result<Option<Acceptance>, InitializationError> {
            assert_eq!(id, "candidate");
            self.call("receipt")?;
            Ok(None)
        }
    }
    #[test]
    fn identity_contract_and_quality_errors_precede_all_io_in_original_order() {
        let mut io = port();
        let mut c = contract();
        c.contract_id = "unsupported".into();
        c.required_quality.push("quality".into());
        assert_eq!(
            initialize(&mut io, "../bad", &c),
            Err(InitializationError::InvalidInput("version identity"))
        );
        assert_eq!(
            initialize(&mut io, "base", &c),
            Err(InitializationError::InvalidInput("fixture contract limits"))
        );
        c.contract_id = contract().contract_id;
        assert_eq!(
            initialize(&mut io, "base", &c),
            Err(InitializationError::InvalidInput(
                "required quality NOT_RUN"
            ))
        );
        assert!(io.calls.is_empty());
    }
    #[test]
    fn owner_scope_and_asset_identity_precede_snapshot_validation() {
        for (source, asset) in [("PRODUCTION", "asset"), ("TEST_FIXTURE", "../bad")] {
            let mut io = port();
            io.source = source;
            io.asset = asset;
            assert_eq!(
                initialize(&mut io, "base", &contract()),
                Err(InitializationError::InvalidInput(
                    "fixture base source/asset"
                ))
            );
            assert_eq!(io.calls, ["owner"]);
        }
    }
    #[test]
    fn observation_and_commit_errors_propagate_without_later_calls_or_replay() {
        let order = ["owner", "snapshot", "validate", "commit"];
        for (index, fail) in order.iter().enumerate() {
            let mut io = port();
            io.fail = fail;
            assert_eq!(
                initialize(&mut io, "base", &contract()),
                Err(InitializationError::Io("observation failed".into()))
            );
            assert_eq!(io.calls, order[..=index]);
        }
    }
    #[test]
    fn failed_or_missing_component_evidence_cannot_issue_commit_proof() {
        for (status, missing) in [("FAIL", false), ("NOT_RUN", false), ("PASS", true)] {
            let mut io = port();
            io.status = status;
            io.missing = missing;
            assert_eq!(
                initialize(&mut io, "base", &contract()),
                Err(InitializationError::InvalidInput(
                    "base GLB structure/role validation"
                ))
            );
            assert_eq!(io.calls, ["owner", "snapshot", "validate"]);
        }
    }
    #[test]
    fn valid_measured_result_commits_once_and_preserves_current_value() {
        let mut io = port();
        assert_eq!(
            initialize(&mut io, "base", &contract()).unwrap(),
            Current {
                asset_id: "asset".into(),
                version_id: "base".into(),
                revision: 0,
                contract: contract()
            }
        );
        assert_eq!(io.calls, ["owner", "snapshot", "validate", "commit"]);
    }
    #[test]
    fn receipt_identity_precedes_query_and_query_error_is_preserved() {
        let mut io = port();
        assert_eq!(
            read(&mut io, "../bad"),
            Err(InitializationError::InvalidInput("candidate id"))
        );
        assert!(io.calls.is_empty());
        assert_eq!(read(&mut io, "candidate").unwrap(), None);
        io.fail = "receipt";
        assert_eq!(
            read(&mut io, "candidate"),
            Err(InitializationError::Io("observation failed".into()))
        );
        assert_eq!(io.calls, ["receipt", "receipt"]);
    }
}
