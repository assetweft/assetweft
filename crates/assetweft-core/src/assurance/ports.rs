//! Responsibility: capabilities required by bounded validation.
//! Dependencies: Assurance domain.
//! Callers: application use cases and concrete adapters.
//! Invariants: no paths, filesystem handles or parser SDK types cross these capabilities.

use super::domain::{CheckedBytes, ValidationLimits};

pub(crate) trait BoundedInput {
    type Error;
    fn read(&self, limits: ValidationLimits) -> Result<Vec<u8>, Self::Error>;
}

pub(crate) trait StructureCheck {
    fn check(&self, bytes: &[u8]) -> CheckedBytes;
}
