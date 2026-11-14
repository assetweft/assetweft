//! Responsibility: legacy validation compatibility exports.
//! Dependencies: Assurance public checks and values.
//! Callers: Application, geometry, rig, CLI and existing tests.
//! Invariants: identical public types and diagnostics; business policy belongs to Assurance.

pub use crate::assurance::checks::{validate_bytes, validate_file};
pub use crate::assurance::file_input::{ValidationError, read_bounded};
pub use crate::assurance::{
    CHECKER_VERSION, CheckItem, CheckStatus, NOT_RUN_ITEMS, RoleRequirement, RoleResult,
    SUPPORTED_FORMAT, SourceClass, SourceDeclaration, UnknownSourceClass, ValidationLimits,
    ValidationReport,
};
