//! Responsibility: legacy validation API and concrete composition.
//! Dependencies: Assurance core and filesystem/GLB adapters.
//! Callers: Application, geometry, rig, CLI and existing tests.
//! Invariants: identical public types and diagnostics; business policy belongs to Assurance.

pub use crate::assurance::file_input::{ValidationError, read_bounded};
pub use crate::assurance::{
    CHECKER_VERSION, CheckItem, CheckStatus, NOT_RUN_ITEMS, RoleRequirement, RoleResult,
    SUPPORTED_FORMAT, SourceClass, SourceDeclaration, UnknownSourceClass, ValidationLimits,
    ValidationReport,
};
use crate::assurance::{application, file_input::FileInput, gltf_checks::GltfChecks};
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
