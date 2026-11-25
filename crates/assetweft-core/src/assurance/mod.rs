pub mod checks;
pub mod rig_blender;

mod action_presentation_domain;
pub(crate) mod application;
mod audio_connector_domain;
mod audio_domain;
mod card_layout_domain;
mod cloth_domain;
mod determinism_domain;
mod domain;
mod face_attachment_domain;
pub(crate) mod file_input;
pub(crate) mod gltf_checks;
mod ports;

pub use action_presentation_domain::{ActionPresentationReport, ActionPresentationViolation};
pub use audio_connector_domain::{AudioConnectorReport, AudioProvenance, Linkage, TrackLicensing};
pub use audio_domain::{AudioLoopReport, AudioReport};
pub use card_layout_domain::{CardLayoutReport, FittedBox, LayoutViolation, TemplateKind};
pub use cloth_domain::{ClothGarmentReport, ClothReport, ClothViolation, GarmentBinding};
pub use face_attachment_domain::{AttachmentResult, FaceAttachmentReport};
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

mod static_kit_domain;
pub use static_kit_domain::{KitKind, OptimizationRecord, Resources, StaticKitReport, Violation};

mod vfx_domain;
pub use vfx_domain::{
    AuthoringState, BlendMode, FlipbookFacts, FlipbookGrid, VfxMode, VfxParameter, VfxReport,
    VfxTiming, VfxViolation,
};
