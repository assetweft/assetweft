//! Responsibility: legacy Rig API and concrete composition.
//! Dependencies: Assurance core and adapters.
//! Callers: existing consumers and integration tests.
//! Invariants: same public Rig report and read errors.

pub use crate::assurance::rig_domain::RigReport;
use crate::assurance::{
    ValidationLimits,
    file_input::{FileInput, ValidationError},
    gltf_checks::GltfChecks,
    rig_application,
    rig_gltf::GltfRig,
};
use std::path::Path;
/// Read only a bounded supplied file.
/// # Errors
/// Returns the original bounded-read error.
pub fn inspect_file(path: &Path, limits: ValidationLimits) -> Result<RigReport, ValidationError> {
    rig_application::inspect_file(&FileInput(path), &GltfChecks, &GltfRig, limits)
}
/// Inspect resident bytes; unsupported numeric scopes remain NOT_RUN.
pub fn inspect_bytes(bytes: &[u8]) -> RigReport {
    rig_application::inspect_bytes(&GltfChecks, &GltfRig, bytes)
}
