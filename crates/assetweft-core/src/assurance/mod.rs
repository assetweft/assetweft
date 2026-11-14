//! Responsibility: Assurance bounded validation ownership.
//! Dependencies: pure core and private concrete adapters.
//! Callers: legacy validation facade.
//! Invariants: no new support or authority claims.

pub(crate) mod application;
mod domain;
pub(crate) mod file_input;
pub(crate) mod gltf_checks;
mod ports;

pub use domain::{
    CHECKER_VERSION, CheckItem, CheckStatus, NOT_RUN_ITEMS, RoleRequirement, RoleResult,
    SUPPORTED_FORMAT, SourceClass, SourceDeclaration, UnknownSourceClass, ValidationLimits,
    ValidationReport,
};
