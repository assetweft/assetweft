//! Responsibility: bounded Rig inspection orchestration.
//! Dependencies: domain and purpose capabilities.
//! Callers: legacy Rig composition.
//! Invariants: invalid format never implies numeric PASS.

use super::{
    application,
    domain::{CheckStatus, RoleRequirement, SourceClass, SourceDeclaration, ValidationLimits},
    ports::{BoundedInput, StructureCheck},
    rig_domain::RigReport,
    rig_ports::RigNumeric,
};
pub(crate) fn inspect_file<I: BoundedInput, C: StructureCheck, N: RigNumeric>(
    input: &I,
    checker: &C,
    numeric: &N,
    limits: ValidationLimits,
) -> Result<RigReport, I::Error> {
    let bytes = input.read(limits)?;
    Ok(inspect_bytes(checker, numeric, &bytes))
}
pub(crate) fn inspect_bytes<C: StructureCheck, N: RigNumeric>(
    checker: &C,
    numeric: &N,
    bytes: &[u8],
) -> RigReport {
    use CheckStatus::{NotRun, Pass};
    let mut r = RigReport {
        validation: application::validate_bytes(
            checker,
            bytes,
            RoleRequirement::StaticMesh,
            SourceDeclaration::declared(SourceClass::Unknown),
        ),
        checks: vec![],
        bound_nodes: vec![],
        not_run: &[
            "pose_mesh_deformation",
            "joint_common_root_scene_membership",
            "rest_transforms",
            "semantic_digest",
            "F0",
            "AI_generation",
            "visual",
            "engine",
        ],
    };
    if r.validation.format_status != Pass {
        r.add(
            "rig.numeric",
            NotRun,
            "format/references rejected; see validation checks for field paths",
        );
        return r;
    }
    numeric.inspect(bytes, &mut r);
    r
}

#[cfg(test)]
mod tests {
    use super::super::domain::CheckedBytes;
    use super::*;
    struct Reject;
    impl StructureCheck for Reject {
        fn check(&self, bytes: &[u8]) -> CheckedBytes {
            assert_eq!(bytes, b"exact");
            CheckedBytes {
                file_sha256: "hash".into(),
                bytes_read: 5,
                structure: Err("exact format detail".into()),
            }
        }
    }
    struct Never;
    impl RigNumeric for Never {
        fn inspect(&self, _: &[u8], _: &mut RigReport) {
            panic!("numeric capability must not run after format failure")
        }
    }
    #[test]
    fn rejected_format_leaves_numeric_not_run_and_preserves_validation() {
        let r = inspect_bytes(&Reject, &Never, b"exact");
        assert_eq!(r.validation.format_status, CheckStatus::Fail);
        assert_eq!(r.validation.checks[0].detail, "exact format detail");
        assert_eq!(r.checks.len(), 1);
        assert_eq!(r.checks[0].status, CheckStatus::NotRun);
        assert!(r.bound_nodes.is_empty());
    }
}
