//! Responsibility: legacy geometry compatibility exports.
//! Dependencies: Assurance public checks and values.
//! Callers: existing library and CLI consumers.
//! Invariants: public values are the same Assurance types.

pub use crate::assurance::checks::{
    GeometryError, inspect_geometry_bytes as inspect_bytes, inspect_geometry_file as inspect_file,
};
pub use crate::assurance::geometry_domain::{
    Accessor, BufferView, GeometryReport, LocalTransform, Material, Node, Primitive, Resource,
};
