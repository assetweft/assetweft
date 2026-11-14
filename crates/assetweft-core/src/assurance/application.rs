//! Responsibility: bounded input validation orchestration.
//! Dependencies: domain and purpose-owned ports.
//! Callers: legacy validation composition.
//! Invariants: read failure prevents parsing; report policy remains pure.

use super::domain::{self, RoleRequirement, SourceDeclaration, ValidationLimits, ValidationReport};
use super::ports::{BoundedInput, StructureCheck};

pub(crate) fn validate_file<I: BoundedInput, C: StructureCheck>(
    input: &I,
    checker: &C,
    requirement: RoleRequirement,
    source: SourceDeclaration,
    limits: ValidationLimits,
) -> Result<ValidationReport, I::Error> {
    let bytes = input.read(limits)?;
    Ok(validate_bytes(checker, &bytes, requirement, source))
}

pub(crate) fn validate_bytes<C: StructureCheck>(
    checker: &C,
    bytes: &[u8],
    requirement: RoleRequirement,
    source: SourceDeclaration,
) -> ValidationReport {
    domain::report(checker.check(bytes), requirement, source)
}

#[cfg(test)]
mod tests {
    use super::super::domain::{
        CheckStatus, CheckedBytes, ComponentFacts, NOT_RUN_ITEMS, SourceClass,
    };
    use super::*;
    use std::cell::RefCell;

    struct Input<'a>(
        &'a RefCell<Vec<&'static str>>,
        Result<Vec<u8>, &'static str>,
    );
    impl BoundedInput for Input<'_> {
        type Error = &'static str;
        fn read(&self, limits: ValidationLimits) -> Result<Vec<u8>, Self::Error> {
            assert_eq!(limits.max_bytes, 7);
            self.0.borrow_mut().push("read");
            self.1.clone()
        }
    }
    struct Checker<'a>(
        &'a RefCell<Vec<&'static str>>,
        Result<ComponentFacts, String>,
    );
    impl StructureCheck for Checker<'_> {
        fn check(&self, bytes: &[u8]) -> CheckedBytes {
            assert_eq!(bytes, b"exact");
            self.0.borrow_mut().push("check");
            CheckedBytes {
                file_sha256: "digest".to_owned(),
                bytes_read: bytes.len(),
                structure: self.1.clone(),
            }
        }
    }
    fn facts(bound: bool) -> ComponentFacts {
        ComponentFacts {
            mesh_count: 1,
            skin_count: 1,
            animation_count: 1,
            has_bound_skinned_mesh: bound,
            has_joint_animation: bound,
        }
    }
    #[test]
    fn input_error_retains_identity_and_prevents_check() {
        let calls = RefCell::new(Vec::new());
        let result = validate_file(
            &Input(&calls, Err("original input failure")),
            &Checker(&calls, Ok(facts(true))),
            RoleRequirement::StaticMesh,
            SourceDeclaration::declared(SourceClass::Unknown),
            ValidationLimits { max_bytes: 7 },
        );
        assert_eq!(result, Err("original input failure"));
        assert_eq!(*calls.borrow(), ["read"]);
    }
    #[test]
    fn parse_failure_keeps_role_and_unsupported_checks_not_run() {
        let calls = RefCell::new(Vec::new());
        let report = validate_file(
            &Input(&calls, Ok(b"exact".to_vec())),
            &Checker(&calls, Err("original parse detail".to_owned())),
            RoleRequirement::AnimatedCharacter,
            SourceDeclaration::declared(SourceClass::AiGenerated),
            ValidationLimits { max_bytes: 7 },
        )
        .unwrap();
        assert_eq!(*calls.borrow(), ["read", "check"]);
        assert_eq!(report.format_status, CheckStatus::Fail);
        assert_eq!(report.checks[0].detail, "original parse detail");
        assert_eq!(report.role.status, CheckStatus::NotRun);
        assert_eq!(report.role.satisfied, None);
        assert!(report.role.missing.is_empty());
        assert_eq!(report.not_run, NOT_RUN_ITEMS);
        assert!(!report.source.verified_by_checker());
    }
    #[test]
    fn unbound_objects_cannot_satisfy_rig_or_motion() {
        let calls = RefCell::new(Vec::new());
        let report = validate_bytes(
            &Checker(&calls, Ok(facts(false))),
            b"exact",
            RoleRequirement::AnimatedCharacter,
            SourceDeclaration::declared(SourceClass::TestFixture),
        );
        assert_eq!(report.format_status, CheckStatus::Pass);
        assert_eq!(report.role.present, ["mesh"]);
        assert_eq!(report.role.missing, ["skin", "animation"]);
        assert_eq!(report.role.status, CheckStatus::Fail);
        assert_eq!(report.checks[2].status, CheckStatus::NotRun);
        assert_eq!(report.checks[3].status, CheckStatus::NotRun);
    }
    #[test]
    fn bound_role_success_does_not_verify_quality_or_rights() {
        let calls = RefCell::new(Vec::new());
        let report = validate_bytes(
            &Checker(&calls, Ok(facts(true))),
            b"exact",
            RoleRequirement::AnimatedCharacter,
            SourceDeclaration::declared(SourceClass::AiGenerated),
        );
        assert_eq!(report.role.status, CheckStatus::Pass);
        assert_eq!(report.role.satisfied, Some(true));
        assert_eq!(report.not_run, NOT_RUN_ITEMS);
        assert!(!report.source.verified_by_checker());
        assert_eq!(report.file_sha256, "digest");
        assert_eq!(report.bytes_read, 5);
    }
}
