//! Responsibility: legacy geometry API and concrete composition.
//! Dependencies: Assurance use cases and adapters.
//! Callers: existing library and CLI consumers.
//! Invariants: public values are the same Assurance types.

pub use crate::assurance::geometry_domain::{
    Accessor, BufferView, GeometryReport, LocalTransform, Material, Node, Primitive, Resource,
};
use crate::assurance::{
    ValidationLimits,
    file_input::{FileInput, ValidationError},
    geometry_application::{self, InspectError},
    geometry_gltf::GltfGeometry,
    gltf_checks::GltfChecks,
};
use std::path::Path;
#[derive(Debug)]
pub enum GeometryError {
    Read(ValidationError),
    Invalid(String),
}
/// Inspect a bounded file without resolving resources.
/// # Errors
/// Preserves bounded-read and parser diagnostics.
pub fn inspect_file(
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
pub fn inspect_bytes(bytes: &[u8]) -> Result<GeometryReport, GeometryError> {
    geometry_application::inspect_bytes(&GltfChecks, &GltfGeometry, bytes)
        .map_err(GeometryError::Invalid)
}
