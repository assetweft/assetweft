pub mod checks;
pub mod rig_blender;

pub(crate) mod application;
mod determinism_domain;
mod domain;
pub(crate) mod file_input;
pub(crate) mod gltf_checks;
mod ports;

pub use file_input::{ValidationError, read_bounded};

pub use determinism_domain::{
    DeterminismError, QUANTIZATION_SCALE_1E6, canonical_f32, canonical_f64, canonical_matrix_4x4,
    canonical_quaternion, canonical_semantic_bytes, canonical_vec3, quantize_1e6,
    quantize_1e6_units, semantic_sha256_hex,
};
pub use domain::{
    CHECKER_VERSION, CheckItem, CheckStatus, NOT_RUN_ITEMS, RoleRequirement, RoleResult,
    SUPPORTED_FORMAT, SourceClass, SourceDeclaration, UnknownSourceClass, ValidationLimits,
    ValidationReport,
};
pub(crate) mod geometry_application;
pub(crate) mod geometry_domain;
pub(crate) mod geometry_gltf;
mod geometry_ports;
pub(crate) mod rig_application;
pub(crate) mod rig_domain;
pub(crate) mod rig_gltf;
mod rig_ports;
pub use geometry_domain::{
    Accessor, BufferView, GeometryReport, LocalTransform, Material, Node, Primitive, Resource,
};
pub use rig_domain::RigReport;

mod sprite_domain;
pub use sprite_domain::{Looping, SpriteFrame, SpriteReport};
