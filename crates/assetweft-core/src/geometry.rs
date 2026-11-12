//! 职责: 检查单个 GLB 的静态几何引用并返回结构摘要，不读取外部资源。
//! 依赖: validation 的有界读取与容器检查、gltf 1.4.1、std。
//! 调用方: 核心库消费者、CLI inspect JSON 与 geometry 集成测试。
//! 不变量: 输入声明未知不等于标准约定；解析不代表渲染成功；外部 URI 不解析。

use crate::validation::{
    self, CheckStatus, RoleRequirement, SourceClass, SourceDeclaration, ValidationError,
    ValidationLimits,
};
use std::path::Path;

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
#[derive(Debug)]
pub enum GeometryError {
    Read(ValidationError),
    Invalid(String),
}

/// 检查调用者指定的单个 GLB。外部 URI（包括 data URI）仅枚举，不解码或读取。
///
/// # Errors
/// 读取超限、结构无效或引用无效时返回错误；错误来自既有验证器或解析器。
pub fn inspect_file(
    path: &Path,
    limits: ValidationLimits,
) -> Result<GeometryReport, GeometryError> {
    let bytes = validation::read_bounded(path, limits).map_err(GeometryError::Read)?;
    inspect_bytes(&bytes)
}

/// 检查内存中的 GLB 字节，不适用于无上限文件读取。
///
/// # Errors
/// 容器、JSON 或内部索引不合法时返回错误及解析器诊断。
pub fn inspect_bytes(bytes: &[u8]) -> Result<GeometryReport, GeometryError> {
    let checked = validation::validate_bytes(
        bytes,
        RoleRequirement::StaticMesh,
        SourceDeclaration::declared(SourceClass::Unknown),
    );
    if checked.format_status != CheckStatus::Pass {
        return Err(GeometryError::Invalid(
            checked
                .checks
                .iter()
                .map(|c| c.detail.as_str())
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    let gltf = gltf::Gltf::from_slice(bytes).map_err(|e| GeometryError::Invalid(e.to_string()))?;
    let d = &gltf.document;
    let nodes = d
        .nodes()
        .map(|n| Node {
            index: n.index(),
            mesh: n.mesh().map(|m| m.index()),
            children: n.children().map(|c| c.index()).collect(),
            transform: match n.transform() {
                gltf::scene::Transform::Matrix { matrix } => LocalTransform::Matrix(matrix),
                gltf::scene::Transform::Decomposed {
                    translation,
                    rotation,
                    scale,
                } => LocalTransform::Trs {
                    translation,
                    rotation,
                    scale,
                },
            },
        })
        .collect();
    let primitives = d
        .meshes()
        .flat_map(|m| {
            let mesh = m.index();
            m.primitives().map(move |p| Primitive {
                mesh,
                primitive: p.index(),
                material: p.material().index(),
                attributes: p
                    .attributes()
                    .map(|(s, a)| (format!("{s:?}"), a.index()))
                    .collect(),
                indices: p.indices().map(|a| a.index()),
            })
        })
        .collect();
    let materials = d
        .materials()
        .map(|m| {
            let p = m.pbr_metallic_roughness();
            let mut textures = Vec::new();
            if let Some(t) = p.base_color_texture() {
                textures.push(("base_color", t.texture().index()));
            }
            if let Some(t) = p.metallic_roughness_texture() {
                textures.push(("metallic_roughness", t.texture().index()));
            }
            if let Some(t) = m.normal_texture() {
                textures.push(("normal", t.texture().index()));
            }
            if let Some(t) = m.occlusion_texture() {
                textures.push(("occlusion", t.texture().index()));
            }
            if let Some(t) = m.emissive_texture() {
                textures.push(("emissive", t.texture().index()));
            }
            Material {
                index: m.index().expect("document material has index"),
                textures,
            }
        })
        .collect();
    let buffers = d
        .buffers()
        .map(|b| match b.source() {
            gltf::buffer::Source::Bin => Resource::Embedded {
                available: gltf.blob.as_ref().is_some_and(|v| v.len() >= b.length()),
            },
            gltf::buffer::Source::Uri(uri) => Resource::UriUnresolved(uri.to_owned()),
        })
        .collect();
    let images = d
        .images()
        .map(|i| match i.source() {
            gltf::image::Source::Uri { uri, .. } => Resource::UriUnresolved(uri.to_owned()),
            gltf::image::Source::View { view, .. } => Resource::BufferView {
                buffer: view.buffer().index(),
                offset: view.offset(),
                length: view.length(),
            },
        })
        .collect();
    Ok(GeometryReport {
        input_units: "UNKNOWN",
        input_coordinates: "UNKNOWN",
        format_convention: "glTF 2.0: right-handed, +Y up, +Z forward, meters",
        nodes,
        primitives,
        materials,
        textures: d
            .textures()
            .map(|t| (t.index(), t.source().index()))
            .collect(),
        buffers,
        buffer_views: d
            .views()
            .map(|v| {
                let b = v.buffer();
                BufferView {
                    buffer: b.index(),
                    offset: v.offset(),
                    length: v.length(),
                    within_declared_buffer: v
                        .offset()
                        .checked_add(v.length())
                        .is_some_and(|end| end <= b.length()),
                }
            })
            .collect(),
        accessors: d
            .accessors()
            .map(|a| Accessor {
                view: a.view().map(|v| v.index()),
                offset: a.offset(),
                sparse: a.sparse().is_some(),
            })
            .collect(),
        images,
        extensions_used: d.extensions_used().map(str::to_owned).collect(),
        extensions_required: d.extensions_required().map(str::to_owned).collect(),
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
    })
}
