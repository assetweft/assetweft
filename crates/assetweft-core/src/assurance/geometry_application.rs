//! Responsibility: geometry bounded inspection use case.
//! Dependencies: domain and purpose capabilities.
//! Callers: legacy geometry composition.
//! Invariants: read then format guard then parse then report.

use super::{
    application,
    domain::{CheckStatus, RoleRequirement, SourceClass, SourceDeclaration, ValidationLimits},
    geometry_domain::{self, GeometryReport},
    geometry_ports::GeometryStructure,
    ports::{BoundedInput, StructureCheck},
};
pub(crate) enum InspectError<E> {
    Read(E),
    Invalid(String),
}
pub(crate) fn inspect_file<I: BoundedInput, C: StructureCheck, G: GeometryStructure>(
    input: &I,
    checker: &C,
    geometry: &G,
    limits: ValidationLimits,
) -> Result<GeometryReport, InspectError<I::Error>> {
    let bytes = input.read(limits).map_err(InspectError::Read)?;
    inspect_bytes(checker, geometry, &bytes).map_err(InspectError::Invalid)
}
pub(crate) fn inspect_bytes<C: StructureCheck, G: GeometryStructure>(
    checker: &C,
    geometry: &G,
    bytes: &[u8],
) -> Result<GeometryReport, String> {
    let checked = application::validate_bytes(
        checker,
        bytes,
        RoleRequirement::StaticMesh,
        SourceDeclaration::declared(SourceClass::Unknown),
    );
    if checked.format_status != CheckStatus::Pass {
        return Err(checked
            .checks
            .iter()
            .map(|c| c.detail.as_str())
            .collect::<Vec<_>>()
            .join("; "));
    }
    geometry.parse(bytes).map(geometry_domain::report)
}

#[cfg(test)]
mod tests {
    use super::super::domain::CheckedBytes;
    use super::*;
    struct Input;
    impl BoundedInput for Input {
        type Error = &'static str;
        fn read(&self, _: ValidationLimits) -> Result<Vec<u8>, Self::Error> {
            Err("exact read payload")
        }
    }
    struct Reject;
    impl StructureCheck for Reject {
        fn check(&self, bytes: &[u8]) -> CheckedBytes {
            assert_eq!(bytes, b"exact");
            CheckedBytes {
                file_sha256: "hash".into(),
                bytes_read: 5,
                structure: Err("format field path".into()),
            }
        }
    }
    struct Never;
    impl GeometryStructure for Never {
        fn parse(&self, _: &[u8]) -> Result<geometry_domain::GeometryFacts, String> {
            panic!("format/read failure must prevent parser")
        }
    }
    #[test]
    fn read_failure_keeps_payload_and_stops_before_format() {
        match inspect_file(&Input, &Reject, &Never, ValidationLimits { max_bytes: 5 }) {
            Err(InspectError::Read(e)) => assert_eq!(e, "exact read payload"),
            _ => panic!("read error changed"),
        }
    }
    #[test]
    fn format_failure_preserves_diagnostics_and_stops_structure_parser() {
        let err = inspect_bytes(&Reject, &Never, b"exact").unwrap_err();
        assert!(err.contains("format field path"));
    }
}
