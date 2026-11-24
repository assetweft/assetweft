use crate::storage::StorageError;
use gltf::{
    Semantic,
    accessor::{DataType, Dimensions},
    json,
};
use serde_json::Value;

const INVALID: StorageError = StorageError::Invalid("malformed GLB geometry");
const PROFILE: StorageError = StorageError::Invalid("unsupported static GLB preview profile");
const EDGE: usize = 128;
const MAX_INDICES: usize = 768;

fn has_extensions(value: &Value) -> bool {
    match value {
        Value::Object(fields) => {
            fields.contains_key("extensions") || fields.values().any(has_extensions)
        }
        Value::Array(items) => items.iter().any(has_extensions),
        _ => false,
    }
}

pub(super) fn thumbnail(bytes: &[u8]) -> Result<Vec<u8>, StorageError> {
    // gltf 1.4.1 subtracts the header size before checking its declared length.
    if bytes.len() < 12 || bytes.get(8..12) != Some(&(bytes.len() as u32).to_le_bytes()) {
        return Err(INVALID);
    }
    let glb = gltf::binary::Glb::from_slice(bytes).map_err(|_| INVALID)?;
    let bin = glb.bin.ok_or(PROFILE)?;
    if 28 + glb.json.len() + bin.len() != bytes.len()
        || !glb.json.len().is_multiple_of(4)
        || !bin.len().is_multiple_of(4)
    {
        return Err(INVALID);
    }
    let value: Value = serde_json::from_slice(&glb.json).map_err(|_| INVALID)?;
    if has_extensions(&value) {
        return Err(PROFILE);
    }
    let root: json::Root = serde_json::from_value(value).map_err(|_| INVALID)?;
    if root.asset.version != "2.0"
        || root
            .asset
            .min_version
            .as_deref()
            .is_some_and(|v| v != "2.0")
        || root.scene.is_none()
        || root.scenes.len() != 1
        || root.scenes[0].nodes.len() != 1
        || root.nodes.len() != 1
        || root.nodes[0].matrix.is_some()
        || root.nodes[0].children.is_some()
        || root.meshes.len() != 1
        || root.meshes[0].primitives.len() != 1
        || root.buffers.len() != 1
        || root.buffer_views.len() != 2
        || root.accessors.len() != 2
        || root.materials.len() != 1
        || !root.animations.is_empty()
        || !root.skins.is_empty()
        || !root.images.is_empty()
        || !root.textures.is_empty()
        || !root.cameras.is_empty()
        || !root.extensions_used.is_empty()
        || !root.extensions_required.is_empty()
        || root.nodes[0].weights.is_some()
        || root.meshes[0].weights.is_some()
    {
        return Err(PROFILE);
    }
    // gltf 1.4.1's POSITION bounds validation indexes this accessor directly.
    if root.meshes[0].primitives[0]
        .attributes
        .values()
        .any(|a| a.value() >= root.accessors.len())
    {
        return Err(INVALID);
    }
    let doc = gltf::Document::from_json(root).map_err(|_| INVALID)?;
    let node = doc
        .default_scene()
        .ok_or(INVALID)?
        .nodes()
        .next()
        .ok_or(INVALID)?;
    let mesh = node.mesh().ok_or(PROFILE)?;
    let primitive = mesh.primitives().next().ok_or(INVALID)?;
    if primitive.mode() != gltf::mesh::Mode::Triangles
        || primitive.attributes().count() != 1
        || primitive.morph_targets().next().is_some()
    {
        return Err(PROFILE);
    }
    let color = flat_color(primitive.material())?;
    let buffer = doc.buffers().next().ok_or(INVALID)?;
    if !matches!(buffer.source(), gltf::buffer::Source::Bin)
        || buffer.length() > bin.len()
        || bin.len() - buffer.length() > 3
        || bin[buffer.length()..].iter().any(|&b| b != 0)
    {
        return Err(PROFILE);
    }
    let pos = primitive.get(&Semantic::Positions).ok_or(INVALID)?;
    let idx = primitive.indices().ok_or(PROFILE)?;
    if (pos.data_type(), pos.dimensions()) != (DataType::F32, Dimensions::Vec3)
        || (idx.data_type(), idx.dimensions()) != (DataType::U16, Dimensions::Scalar)
        || !idx.count().is_multiple_of(3)
    {
        return Err(PROFILE);
    }
    if pos
        .view()
        .zip(idx.view())
        .is_some_and(|(p, i)| p.index() == i.index())
    {
        return Err(INVALID);
    }
    for (a, target) in [
        (&pos, gltf::buffer::Target::ArrayBuffer),
        (&idx, gltf::buffer::Target::ElementArrayBuffer),
    ] {
        let view = a.view().ok_or(PROFILE)?;
        if a.sparse().is_some()
            || a.normalized()
            || doc.as_json().buffer_views[view.index()]
                .byte_stride
                .is_some()
            || view.target().is_some_and(|t| t != target)
            || !(3..=MAX_INDICES).contains(&a.count())
        {
            return Err(PROFILE);
        }
        if view
            .offset()
            .checked_add(view.length())
            .is_none_or(|end| end > buffer.length())
            || a.offset()
                .checked_add(a.count() * a.size())
                .is_none_or(|end| end > view.length())
            || !(view.offset() + a.offset()).is_multiple_of(a.data_type().size())
            || !a.offset().is_multiple_of(a.data_type().size())
        {
            return Err(INVALID);
        }
    }
    let transform = node.transform();
    if let gltf::scene::Transform::Decomposed { rotation, .. } = &transform
        && (rotation.iter().map(|v| v * v).sum::<f32>() - 1.).abs() > 0.0001
    {
        return Err(INVALID);
    }
    let matrix = transform.matrix();
    if !matrix.iter().flatten().all(|v| v.is_finite()) {
        return Err(INVALID);
    }
    let reader = primitive.reader(|_| Some(&bin[..buffer.length()]));
    let mut positions = Vec::with_capacity(pos.count());
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in reader.read_positions().ok_or(INVALID)? {
        if !p.iter().all(|v| v.is_finite()) {
            return Err(INVALID);
        }
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
        // glTF 2.0.1 node translation cancels in centered framing; omitting it preserves small triangles at large f32 offsets.
        let axis = |i: usize| {
            (0..3)
                .map(|j| f64::from(matrix[j][i]) * f64::from(p[j]))
                .sum::<f64>()
        };
        positions.push([axis(0), axis(1)]);
    }
    let bounds = primitive.bounding_box();
    if bounds.min != min || bounds.max != max {
        return Err(INVALID);
    }
    let mut vertices = Vec::with_capacity(idx.count());
    for index in reader.read_indices().ok_or(INVALID)?.into_u32() {
        vertices.push(*positions.get(index as usize).ok_or(INVALID)?);
    }
    draw(&mut vertices, color)
}

fn flat_color(material: gltf::Material<'_>) -> Result<[u8; 4], StorageError> {
    let pbr = material.pbr_metallic_roughness();
    let factor = pbr.base_color_factor();
    if !material.double_sided()
        || material.alpha_mode() != gltf::material::AlphaMode::Opaque
        || material.emissive_factor() != [0.; 3]
        || pbr.metallic_factor() != 0.
        || pbr.roughness_factor() != 1.
        || !factor.iter().all(|v| (0.0..=1.).contains(v))
    {
        return Err(PROFILE);
    }
    let srgb = |v: f32| {
        let c = if v <= 0.0031308 {
            12.92 * v
        } else {
            1.055 * v.powf(1. / 2.4) - 0.055
        };
        (255. * c).round() as u8
    };
    Ok([srgb(factor[0]), srgb(factor[1]), srgb(factor[2]), 255])
}

fn draw(vertices: &mut [[f64; 2]], color: [u8; 4]) -> Result<Vec<u8>, StorageError> {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for point in vertices.iter() {
        for i in 0..2 {
            min[i] = min[i].min(point[i]);
            max[i] = max[i].max(point[i]);
        }
    }
    let extent = (max[0] - min[0]).max(max[1] - min[1]);
    if extent == 0. {
        return Err(PROFILE);
    }
    for point in vertices.iter_mut() {
        for i in 0..2 {
            let centered = (point[i] - min[i]) - (max[i] - min[i]) * 0.5;
            point[i] = 64. + if i == 0 { 112. } else { -112. } * centered / extent;
        }
    }
    let edge = |a: [f64; 2], b: [f64; 2], p: [f64; 2]| {
        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
    };
    let mut pixels = vec![0u8; EDGE * EDGE * 4];
    for &[a, b, c] in vertices.as_chunks::<3>().0 {
        let area = edge(a, b, c);
        if area == 0. {
            continue;
        }
        for y in 0..EDGE {
            for x in 0..EDGE {
                let p = [x as f64 + 0.5, y as f64 + 0.5];
                if [edge(a, b, p), edge(b, c, p), edge(c, a, p)]
                    .iter()
                    .all(|&v| v * area.signum() >= 0.)
                {
                    let at = 4 * (y * EDGE + x);
                    pixels[at..at + 4].copy_from_slice(&color);
                }
            }
        }
    }
    if !pixels.chunks_exact(4).any(|p| p[3] != 0) {
        return Err(PROFILE);
    }
    let encode = |_| StorageError::Invalid("PNG encoding failed");
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, EDGE as u32, EDGE as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_source_srgb(png::SrgbRenderingIntent::RelativeColorimetric);
    let mut writer = encoder.write_header().map_err(encode)?;
    writer.write_image_data(&pixels).map_err(encode)?;
    writer.finish().map_err(encode)?;
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn refuses_unreadable_or_unrepresented_geometry() {
        let original = include_bytes!("../../../../examples/godot-static/triangle.glb");
        let mut glb = gltf::binary::Glb::from_slice(original).unwrap();
        let fixture: Value = serde_json::from_slice(&glb.json).unwrap();
        for (parent, key, value) in [
            ("/buffers/0", "uri", json!("external.bin")),
            ("/nodes/0", "children", json!([0])),
            ("/materials/0", "alphaMode", json!("BLEND")),
            (
                "/materials/0",
                "extensions",
                json!({"KHR_materials_unlit": {}}),
            ),
            ("/meshes/0/primitives/0", "mode", json!(1)),
            ("/meshes/0/primitives/0/attributes", "POSITION", json!(9)),
            ("/meshes/0/primitives/0/attributes", "COLOR_0", json!(0)),
            ("/accessors/0", "count", json!(0)),
            ("/accessors/1", "count", json!(771)),
            ("/bufferViews/0", "byteOffset", json!(u64::MAX)),
            ("/nodes/0", "scale", json!([0, 0, 0])),
        ] {
            let mut doc = fixture.clone();
            doc.pointer_mut(parent).unwrap()[key] = value;
            glb.json = serde_json::to_vec(&doc).unwrap().into();
            assert!(thumbnail(&glb.to_vec().unwrap()).is_err(), "{parent}/{key}");
        }
    }
}
