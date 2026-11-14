//! Responsibility: geometry public values and structure report policy.
//! Dependencies: serde and plain facts.
//! Callers: geometry application and SDK adapter.
//! Invariants: no numeric geometry or resource resolution.

/// 节点局部变换；矩阵保留原始值，避免不可靠的分解。
#[derive(Debug, PartialEq, serde::Serialize)]
pub enum LocalTransform {
    Matrix([[f32; 4]; 4]),
    Trs {
        translation: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    },
}
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Node {
    pub index: usize,
    pub mesh: Option<usize>,
    pub children: Vec<usize>,
    pub transform: LocalTransform,
}
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Primitive {
    pub mesh: usize,
    pub primitive: usize,
    pub material: Option<usize>,
    pub attributes: Vec<(String, usize)>,
    pub indices: Option<usize>,
}
#[derive(Debug, PartialEq, serde::Serialize)]
pub enum Resource {
    Embedded {
        available: bool,
    },
    UriUnresolved(String),
    BufferView {
        buffer: usize,
        offset: usize,
        length: usize,
    },
}
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct BufferView {
    pub buffer: usize,
    pub offset: usize,
    pub length: usize,
    pub within_declared_buffer: bool,
}
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Accessor {
    pub view: Option<usize>,
    pub offset: usize,
    pub sparse: bool,
}
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Material {
    pub index: usize,
    pub textures: Vec<(&'static str, usize)>,
}
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct GeometryReport {
    pub input_units: &'static str,
    pub input_coordinates: &'static str,
    pub format_convention: &'static str,
    pub nodes: Vec<Node>,
    pub primitives: Vec<Primitive>,
    pub materials: Vec<Material>,
    pub textures: Vec<(usize, usize)>,
    pub buffers: Vec<Resource>,
    pub buffer_views: Vec<BufferView>,
    pub accessors: Vec<Accessor>,
    pub images: Vec<Resource>,
    pub extensions_used: Vec<String>,
    pub extensions_required: Vec<String>,
    pub not_run: &'static [&'static str],
}

pub(crate) struct GeometryFacts {
    pub nodes: Vec<Node>,
    pub primitives: Vec<Primitive>,
    pub materials: Vec<Material>,
    pub textures: Vec<(usize, usize)>,
    pub buffers: Vec<Resource>,
    pub buffer_views: Vec<BufferView>,
    pub accessors: Vec<Accessor>,
    pub images: Vec<Resource>,
    pub extensions_used: Vec<String>,
    pub extensions_required: Vec<String>,
}

pub(crate) fn report(f: GeometryFacts) -> GeometryReport {
    GeometryReport {
        input_units: "UNKNOWN",
        input_coordinates: "UNKNOWN",
        format_convention: "glTF 2.0: right-handed, +Y up, +Z forward, meters",
        nodes: f.nodes,
        primitives: f.primitives,
        materials: f.materials,
        textures: f.textures,
        buffers: f.buffers,
        buffer_views: f.buffer_views,
        accessors: f.accessors,
        images: f.images,
        extensions_used: f.extensions_used,
        extensions_required: f.extensions_required,
        not_run: &[
            "external resource resolution",
            "extension semantics",
            "geometry values",
            "sparse accessor reference details",
            "rendering",
            "engine",
            "visual",
            "physical scale verification",
            "rig and deformation",
        ],
    }
}
