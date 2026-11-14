//! Responsibility: translate SDK structure into geometry facts.
//! Dependencies: gltf and geometry values.
//! Callers: geometry application capability.
//! Invariants: URI identities remain unresolved and unchanged.

use super::{geometry_domain::*, geometry_ports::GeometryStructure};
pub(crate) struct GltfGeometry;
impl GeometryStructure for GltfGeometry {
    fn parse(&self, bytes: &[u8]) -> Result<GeometryFacts, String> {
        let gltf = gltf::Gltf::from_slice(bytes).map_err(|e| e.to_string())?;
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
        Ok(GeometryFacts {
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
        })
    }
}
