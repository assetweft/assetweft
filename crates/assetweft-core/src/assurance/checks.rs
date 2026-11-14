//! Responsibility: public Assurance check composition and concrete read errors.
//! Dependencies: existing Assurance use cases and private filesystem/GLB adapters.
//! Callers: legacy facades and public check consumers.
//! Invariants: unchanged sequencing, diagnostics and type identity; no business policy here.

use super::{
    GeometryReport, RigReport, RoleRequirement, SourceDeclaration, ValidationLimits,
    ValidationReport, application,
    file_input::{FileInput, ValidationError},
    geometry_application::{self, InspectError},
    geometry_gltf::GltfGeometry,
    gltf_checks::GltfChecks,
    rig_application,
    rig_gltf::GltfRig,
};
use std::path::Path;

/// 验证调用者给定的单个文件。
///
/// # Errors
/// 读取失败或超限时返回 [`ValidationError`]；文件格式失败体现在报告中。
pub fn validate_file(
    path: &Path,
    requirement: RoleRequirement,
    source: SourceDeclaration,
    limits: ValidationLimits,
) -> Result<ValidationReport, ValidationError> {
    application::validate_file(&FileInput(path), &GltfChecks, requirement, source, limits)
}

/// 验证已读取的字节（不读文件、不联网）。
#[must_use]
pub fn validate_bytes(
    bytes: &[u8],
    requirement: RoleRequirement,
    source: SourceDeclaration,
) -> ValidationReport {
    application::validate_bytes(&GltfChecks, bytes, requirement, source)
}

#[derive(Debug)]
pub enum GeometryError {
    Read(ValidationError),
    Invalid(String),
}
/// Inspect a bounded file without resolving resources.
/// # Errors
/// Preserves bounded-read and parser diagnostics.
pub fn inspect_geometry_file(
    path: &Path,
    limits: ValidationLimits,
) -> Result<GeometryReport, GeometryError> {
    geometry_application::inspect_file(&FileInput(path), &GltfChecks, &GltfGeometry, limits)
        .map_err(|e| match e {
            InspectError::Read(e) => GeometryError::Read(e),
            InspectError::Invalid(e) => GeometryError::Invalid(e),
        })
}
/// Inspect resident bytes without resource resolution.
/// # Errors
/// Preserves format and parser diagnostics.
pub fn inspect_geometry_bytes(bytes: &[u8]) -> Result<GeometryReport, GeometryError> {
    geometry_application::inspect_bytes(&GltfChecks, &GltfGeometry, bytes)
        .map_err(GeometryError::Invalid)
}

/// Read only a bounded supplied file.
/// # Errors
/// Returns the original bounded-read error.
pub fn inspect_rig_file(
    path: &Path,
    limits: ValidationLimits,
) -> Result<RigReport, ValidationError> {
    rig_application::inspect_file(&FileInput(path), &GltfChecks, &GltfRig, limits)
}
/// Inspect resident bytes; unsupported numeric scopes remain NOT_RUN.
pub fn inspect_rig_bytes(bytes: &[u8]) -> RigReport {
    rig_application::inspect_bytes(&GltfChecks, &GltfRig, bytes)
}
