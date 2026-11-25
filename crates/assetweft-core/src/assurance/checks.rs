use super::audio_domain::{AudioFormat, AudioSpec};
use super::{
    GeometryReport, RigReport, RoleRequirement, SourceDeclaration, ValidationLimits,
    ValidationReport, application,
    file_input::{FileInput, ValidationError},
    geometry_application::{self, InspectError},
    geometry_gltf::GltfGeometry,
    gltf_checks::GltfChecks,
    rig_application,
    rig_gltf::GltfRig,
};
use std::path::Path;

#[derive(Debug)]
pub struct AudioInputError {
    pub field: &'static str,
    pub detail: String,
}

pub fn inspect_audio_file(
    path: &Path,
    spec: Option<&Path>,
    limits: ValidationLimits,
) -> Result<super::AudioReport, AudioInputError> {
    let invalid = |field, detail| AudioInputError { field, detail };
    let bytes = super::read_bounded(path, limits).map_err(|e| invalid("file", e.to_string()))?;
    let (format, data, frames) = wav_payload(&bytes).map_err(|e| invalid("file", e.into()))?;
    let spec = if let Some(path) = spec {
        let bytes =
            super::read_bounded(path, limits).map_err(|e| invalid("spec", e.to_string()))?;
        let spec: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| invalid("spec", e.to_string()))?;
        if !spec.is_object()
            || spec
                .get("loop_region")
                .is_some_and(|region| !region.is_null() && !region.is_object())
        {
            return Err(invalid(
                "spec",
                "spec and loop_region must be JSON objects".into(),
            ));
        }
        let spec: AudioSpec =
            serde_json::from_slice(&bytes).map_err(|e| invalid("spec", e.to_string()))?;
        spec.validate(frames)
            .map_err(|e| invalid("spec", e.into()))?;
        Some(spec)
    } else {
        None
    };
    format
        .inspect(data, frames, spec)
        .map_err(|e| invalid("file", e.into()))
}

fn wav_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn wav_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn wav_format(bytes: &[u8]) -> Result<(AudioFormat, usize), &'static str> {
    if !(bytes.len() == 16 || (bytes.len() == 18 && bytes[16..] == [0, 0])) {
        return Err("unsupported or truncated WAV fmt chunk");
    }
    let channels = wav_u16(&bytes[2..]);
    let sample_rate_hz = wav_u32(&bytes[4..]);
    let bits_per_sample = wav_u16(&bytes[14..]);
    let decode: fn(&[u8]) -> f64 = match (wav_u16(bytes), bits_per_sample) {
        (1, 8) => |sample| (f64::from(sample[0]) - 128.0) / 128.0,
        (1, 16) => |sample| f64::from(i16::from_le_bytes([sample[0], sample[1]])) / 32_768.0,
        (1, 24) => |sample| {
            let signed = i32::from_le_bytes([0, sample[0], sample[1], sample[2]]) >> 8;
            f64::from(signed) / 8_388_608.0
        },
        (3, 32) => |sample| f64::from(f32::from_bits(wav_u32(sample))),
        _ => return Err("unsupported WAV codec or sample width"),
    };
    let block_align = u32::from(channels) * u32::from(bits_per_sample / 8);
    if channels == 0
        || sample_rate_hz == 0
        || u32::from(wav_u16(&bytes[12..])) != block_align
        || sample_rate_hz.checked_mul(block_align) != Some(wav_u32(&bytes[8..]))
    {
        return Err("invalid WAV channel count, sample rate, block alignment or byte rate");
    }
    Ok((
        AudioFormat {
            channels,
            sample_rate_hz,
            bits_per_sample,
            decode,
        },
        block_align as usize,
    ))
}

fn wav_payload(bytes: &[u8]) -> Result<(AudioFormat, &[u8], u64), &'static str> {
    if bytes.len() < 12
        || &bytes[..4] != b"RIFF"
        || &bytes[8..12] != b"WAVE"
        || wav_u32(&bytes[4..]) as usize != bytes.len() - 8
    {
        return Err("invalid RIFF/WAVE header or container size");
    }
    let mut rest = &bytes[12..];
    let mut format = None;
    let mut data = None;
    while !rest.is_empty() {
        let (header, payload) = rest
            .split_at_checked(8)
            .ok_or("truncated WAV chunk header")?;
        let size = wav_u32(&header[4..]) as usize;
        let (chunk, tail) = payload
            .split_at_checked(size)
            .ok_or("truncated WAV chunk")?;
        rest = tail;
        if !size.is_multiple_of(2) {
            let Some((&0, tail)) = rest.split_first() else {
                return Err("missing or nonzero WAV chunk padding");
            };
            rest = tail;
        }
        match &header[..4] {
            b"fmt " if format.is_none() => format = Some(wav_format(chunk)?),
            b"data" if format.is_some() && data.is_none() => data = Some(chunk),
            b"fmt " | b"data" => return Err("duplicate or out-of-order WAV fmt/data chunk"),
            _ => {}
        }
    }
    let (format, block_align) = format.ok_or("missing WAV fmt chunk")?;
    let data = data.ok_or("missing WAV data chunk")?;
    if data.is_empty() || !data.len().is_multiple_of(block_align) {
        return Err("WAV data must contain one or more complete sample frames");
    }
    Ok((format, data, (data.len() / block_align) as u64))
}

/// 验证调用者给定的单个文件。
///
/// # Errors
/// 读取失败或超限时返回 [`ValidationError`]；文件格式失败体现在报告中。
pub fn validate_file(
    path: &Path,
    requirement: RoleRequirement,
    source: SourceDeclaration,
    limits: ValidationLimits,
) -> Result<ValidationReport, ValidationError> {
    application::validate_file(&FileInput(path), &GltfChecks, requirement, source, limits)
}

/// 验证已读取的字节（不读文件、不联网）。
#[must_use]
pub fn validate_bytes(
    bytes: &[u8],
    requirement: RoleRequirement,
    source: SourceDeclaration,
) -> ValidationReport {
    application::validate_bytes(&GltfChecks, bytes, requirement, source)
}

#[derive(Debug)]
pub enum GeometryError {
    Read(ValidationError),
    Invalid(String),
}
/// Inspect a bounded file without resolving resources.
pub fn inspect_geometry_file(
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
pub fn inspect_geometry_bytes(bytes: &[u8]) -> Result<GeometryReport, GeometryError> {
    geometry_application::inspect_bytes(&GltfChecks, &GltfGeometry, bytes)
        .map_err(GeometryError::Invalid)
}

pub fn inspect_rig_file(
    path: &Path,
    limits: ValidationLimits,
) -> Result<RigReport, ValidationError> {
    rig_application::inspect_file(&FileInput(path), &GltfChecks, &GltfRig, limits)
}
/// Inspect resident bytes; unsupported numeric scopes remain NOT_RUN.
pub fn inspect_rig_bytes(bytes: &[u8]) -> RigReport {
    rig_application::inspect_bytes(&GltfChecks, &GltfRig, bytes)
}

#[derive(Debug)]
pub enum SpriteError {
    InvalidInput { field: &'static str, detail: String },
    Contract(&'static str),
}

pub fn inspect_sprite_file(
    path: &Path,
    spec: &Path,
    limits: ValidationLimits,
) -> Result<super::SpriteReport, SpriteError> {
    let invalid = |field, detail| SpriteError::InvalidInput { field, detail };
    let bytes = super::read_bounded(path, limits).map_err(|e| invalid("file", e.to_string()))?;
    let spec = super::read_bounded(spec, limits).map_err(|e| invalid("spec", e.to_string()))?;
    let manifest: super::sprite_domain::Manifest =
        serde_json::from_slice(&spec).map_err(|e| invalid("spec", e.to_string()))?;
    let (width, height, has_alpha_channel, rgba) =
        decode_sprite_png(&bytes).map_err(|e| invalid("file", e))?;
    manifest
        .inspect(width, height, has_alpha_channel, &rgba)
        .map_err(SpriteError::Contract)
}

#[derive(Debug)]
pub enum CardLayoutError {
    InvalidInput { field: &'static str, detail: String },
    Contract(super::LayoutViolation),
}

pub fn check_card_template_file(
    template: &Path,
    illustration: Option<&Path>,
    limits: ValidationLimits,
) -> Result<super::CardLayoutReport, CardLayoutError> {
    use super::card_layout_domain::{Spec, sha256_hex};

    let invalid = |field, detail| CardLayoutError::InvalidInput { field, detail };
    let template =
        super::read_bounded(template, limits).map_err(|e| invalid("template", e.to_string()))?;
    let spec: Spec =
        serde_json::from_slice(&template).map_err(|e| invalid("template", e.to_string()))?;
    let illustration = illustration
        .map(|path| {
            let bytes = super::read_bounded(path, limits)
                .map_err(|e| invalid("illustration", e.to_string()))?;
            let (width, height, _, _) =
                decode_sprite_png(&bytes).map_err(|e| invalid("illustration", e))?;
            Ok((width, height, sha256_hex(&bytes)))
        })
        .transpose()?;
    spec.check(illustration).map_err(CardLayoutError::Contract)
}

pub fn inspect_action_presentation_file(
    spec: &Path,
    limits: ValidationLimits,
) -> Result<super::ActionPresentationReport, String> {
    let bytes = super::read_bounded(spec, limits).map_err(|e| e.to_string())?;
    let spec: super::action_presentation_domain::Spec =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    spec.inspect()
}

#[derive(Debug)]
pub struct VfxInputError {
    pub field: &'static str,
    pub detail: String,
}

pub fn inspect_vfx_file(
    spec: &Path,
    flipbook: Option<&Path>,
    limits: ValidationLimits,
) -> Result<super::VfxReport, VfxInputError> {
    use super::card_layout_domain::sha256_hex;
    use super::vfx_domain::{FlipbookFacts, Spec};

    let invalid = |field, detail| VfxInputError { field, detail };
    let bytes = super::read_bounded(spec, limits).map_err(|e| invalid("spec", e.to_string()))?;
    let spec: Spec = serde_json::from_slice(&bytes).map_err(|e| invalid("spec", e.to_string()))?;
    let atlas = flipbook
        .map(|path| {
            let bytes = super::read_bounded(path, limits)
                .map_err(|e| invalid("flipbook", e.to_string()))?;
            let (width, height, has_alpha_channel, rgba) =
                decode_sprite_png(&bytes).map_err(|e| invalid("flipbook", e))?;
            let facts = FlipbookFacts {
                width,
                height,
                has_alpha_channel,
                transparent_pixel_count: rgba
                    .chunks_exact(4)
                    .filter(|pixel| pixel[3] < 255)
                    .count(),
            };
            Ok((facts, sha256_hex(&bytes)))
        })
        .transpose()?;
    spec.inspect(sha256_hex(&bytes), atlas)
        .map_err(|e| invalid("spec", e.to_string()))
}

#[derive(Debug)]
pub struct StaticKitInputError {
    pub field: &'static str,
    pub detail: String,
}

pub fn inspect_static_kit_file(
    file: &Path,
    spec: &Path,
    limits: ValidationLimits,
) -> Result<super::StaticKitReport, StaticKitInputError> {
    let invalid = |field, detail| StaticKitInputError { field, detail };
    let bytes = super::read_bounded(file, limits).map_err(|e| invalid("file", e.to_string()))?;
    let spec = super::read_bounded(spec, limits).map_err(|e| invalid("spec", e.to_string()))?;
    let spec: super::static_kit_domain::Spec =
        serde_json::from_slice(&spec).map_err(|e| invalid("spec", e.to_string()))?;
    let contract = spec.validate().map_err(|e| invalid("spec", e))?;
    let facts = static_kit_facts(&bytes).map_err(|e| invalid("file", e))?;
    Ok(contract.inspect(facts))
}

fn static_kit_facts(bytes: &[u8]) -> Result<super::static_kit_domain::Facts, String> {
    use super::static_kit_domain::{Facts, Resources, Topology, canonical_bounds, point_units};
    use gltf::{Semantic, buffer::Source, material::AlphaMode, mesh::Mode};
    use std::collections::{BTreeMap, BTreeSet};

    const MAX_MEASURED_ENTRIES: usize = 2_000_000;
    let (gltf, _) = super::gltf_checks::parse_glb(bytes)?;
    let bin = gltf.blob.as_deref().unwrap_or_default();
    let document = &gltf.document;
    for buffer in document.buffers() {
        if buffer.index() != 0
            || !matches!(buffer.source(), Source::Bin)
            || buffer.length() > bin.len()
        {
            return Err("static kit requires an embedded BIN buffer".into());
        }
    }
    for view in document.views() {
        if view
            .offset()
            .checked_add(view.length())
            .is_none_or(|end| end > view.buffer().length())
        {
            return Err(format!(
                "buffer view {} exceeds the declared buffer",
                view.index()
            ));
        }
    }
    let mut image_views = BTreeSet::new();
    let mut resources = Resources::default();
    for image in &document.as_json().images {
        let (Some(index), Some(mime), None) = (image.buffer_view, &image.mime_type, &image.uri)
        else {
            return Err("static kit requires embedded images".into());
        };
        let view = document
            .views()
            .nth(index.value())
            .expect("validated view reference");
        if !matches!(mime.0.as_str(), "image/png" | "image/jpeg" | "image/webp")
            || view.length() == 0
            || view.stride().is_some()
            || view.target().is_some()
        {
            return Err("invalid embedded image declaration".into());
        }
        if image_views.insert(view.index()) {
            resources.texture_bytes += view.length() as u64;
        }
    }

    let mut primitive_count = 0;
    let mut materials = BTreeMap::new();
    let (mut boundary, mut non_manifold, mut degenerate) = (0, 0, 0);
    let mut meshes = Vec::new();
    let mut remaining = MAX_MEASURED_ENTRIES;
    for mesh in document.meshes() {
        let mut topology = Topology::default();
        let mut mesh_positions = Vec::new();
        for primitive in mesh.primitives() {
            let position = primitive
                .get(&Semantic::Positions)
                .ok_or("missing POSITION")?;
            if position.data_type() != gltf::accessor::DataType::F32
                || position.dimensions() != gltf::accessor::Dimensions::Vec3
                || position.normalized()
            {
                return Err("POSITION must be an unnormalized FLOAT VEC3".into());
            }
            check_dense_accessor(&position)?;
            let count = if let Some(index) = primitive.indices() {
                if !matches!(
                    index.data_type(),
                    gltf::accessor::DataType::U8
                        | gltf::accessor::DataType::U16
                        | gltf::accessor::DataType::U32
                ) || index.dimensions() != gltf::accessor::Dimensions::Scalar
                    || index.normalized()
                {
                    return Err("indices must be unnormalized unsigned scalars".into());
                }
                check_dense_accessor(&index)?;
                if index.view().and_then(|view| view.stride()).is_some() {
                    return Err("indices must be tightly packed".into());
                }
                index.count()
            } else {
                position.count()
            };
            remaining = remaining
                .checked_sub(position.count())
                .and_then(|n| n.checked_sub(count))
                .ok_or("static kit mesh measurement limit exceeded")?;
            let reader = primitive.reader(|_| Some(bin));
            let positions: Vec<_> = reader
                .read_positions()
                .ok_or("unreadable POSITION")?
                .collect();
            let points: Vec<_> = positions
                .iter()
                .map(|p| point_units(p.map(f64::from)))
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?;
            let indices: Vec<_> = if primitive.indices().is_some() {
                reader
                    .read_indices()
                    .ok_or("unreadable indices")?
                    .into_u32()
                    .collect()
            } else {
                (0..positions.len() as u32).collect()
            };
            if indices.iter().any(|&i| i as usize >= points.len()) {
                return Err("triangle index exceeds POSITION count".into());
            }
            let mut triangle = |a: u32, b: u32, c: u32| {
                topology.add([a, b, c].map(|i| points[i as usize]));
                resources.triangle_count += 1;
            };
            match primitive.mode() {
                Mode::Triangles if count >= 3 && count.is_multiple_of(3) => {
                    for face in indices.chunks_exact(3) {
                        triangle(face[0], face[1], face[2]);
                    }
                }
                Mode::TriangleStrip if count >= 3 => {
                    for (i, face) in indices.windows(3).enumerate() {
                        triangle(face[i % 2], face[1 - i % 2], face[2]);
                    }
                }
                Mode::TriangleFan if count >= 3 => {
                    for face in indices[1..].windows(2) {
                        triangle(indices[0], face[0], face[1]);
                    }
                }
                _ => return Err("invalid triangle count or unsupported primitive mode".into()),
            }
            primitive_count += 1;
            resources.vertex_count += positions.len() as u64;
            mesh_positions.extend(positions);
            let material = primitive.material();
            materials.insert(
                material.index(),
                (
                    material.double_sided(),
                    match material.alpha_mode() {
                        AlphaMode::Opaque => "OPAQUE",
                        AlphaMode::Mask => "MASK",
                        AlphaMode::Blend => "BLEND",
                    },
                ),
            );
        }
        let (open, invalid) = topology.edge_counts();
        boundary += open;
        non_manifold += invalid;
        degenerate += topology.degenerate_triangles;
        meshes.push(mesh_positions);
    }

    #[derive(serde::Deserialize)]
    struct NodeNames {
        #[serde(default)]
        nodes: Vec<NodeName>,
    }
    #[derive(serde::Deserialize)]
    struct NodeName {
        name: Option<String>,
    }
    let container = gltf::binary::Glb::from_slice(bytes).map_err(|e| e.to_string())?;
    let names: NodeNames = serde_json::from_slice(&container.json).map_err(|e| e.to_string())?;
    let mut named_origins = BTreeMap::new();
    let mut mesh_scales = Vec::new();
    let mut bounds = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
    let mut remaining = MAX_MEASURED_ENTRIES;
    for (node, world) in document.nodes().zip(static_kit_world_transforms(document)?) {
        if let Some(name) = &names.nodes[node.index()].name {
            let origin =
                point_units([world[3][0], world[3][1], world[3][2]]).map_err(|e| e.to_string())?;
            named_origins
                .entry(name.clone())
                .and_modify(|slot| *slot = None)
                .or_insert(Some(origin));
        }
        if let Some(mesh) = node.mesh() {
            let scales =
                std::array::from_fn(|i| world[i][..3].iter().map(|v| v * v).sum::<f64>().sqrt());
            mesh_scales.push((
                node.index(),
                point_units(scales).map_err(|e| e.to_string())?,
            ));
            let positions = &meshes[mesh.index()];
            remaining = remaining
                .checked_sub(positions.len())
                .ok_or("static kit instance measurement limit exceeded")?;
            for &position in positions {
                let point = super::static_kit_domain::transform(world, position);
                for (axis, value) in point.into_iter().enumerate() {
                    if !value.is_finite() {
                        return Err("non-finite world-space position".into());
                    }
                    bounds[0][axis] = bounds[0][axis].min(value);
                    bounds[1][axis] = bounds[1][axis].max(value);
                }
            }
        }
    }
    if !bounds[0][0].is_finite() {
        return Err("static kit requires an instantiated triangle mesh".into());
    }
    Ok(Facts {
        primitive_count,
        resources,
        bounds: canonical_bounds(bounds).map_err(|e| e.to_string())?,
        materials,
        boundary,
        non_manifold,
        degenerate,
        mesh_scales,
        named_origins,
    })
}

fn check_dense_accessor(accessor: &gltf::Accessor<'_>) -> Result<(), String> {
    let view = accessor.view().ok_or("mesh accessor lacks a buffer view")?;
    let size = accessor.size();
    let stride = view.stride().unwrap_or(size);
    let end = accessor
        .count()
        .checked_sub(1)
        .and_then(|count| count.checked_mul(stride))
        .and_then(|offset| offset.checked_add(accessor.offset()))
        .and_then(|offset| offset.checked_add(size));
    if accessor.sparse().is_some() || stride < size || end.is_none_or(|end| end > view.length()) {
        return Err(format!(
            "accessor {} must be dense and fit its view",
            accessor.index()
        ));
    }
    Ok(())
}

fn static_kit_world_transforms(
    document: &gltf::Document,
) -> Result<Vec<super::static_kit_domain::Matrix>, String> {
    let mut worlds = vec![[[0.0; 4]; 4]; document.nodes().len()];
    let mut has_parent = vec![false; worlds.len()];
    for node in document.nodes() {
        for child in node.children() {
            if std::mem::replace(&mut has_parent[child.index()], true) {
                return Err("node has more than one parent".into());
            }
        }
    }
    let mut pending: Vec<_> = document
        .nodes()
        .filter(|n| !has_parent[n.index()])
        .map(|node| (node, None))
        .collect();
    let mut visited = 0;
    while let Some((node, parent)) = pending.pop() {
        visited += 1;
        let local = node
            .transform()
            .matrix()
            .map(|column| column.map(f64::from));
        if local.iter().flatten().any(|v| !v.is_finite())
            || local.map(|column| column[3]) != [0.0, 0.0, 0.0, 1.0]
        {
            return Err("node transform must be finite and affine".into());
        }
        let world = parent.map_or(local, |index| {
            super::static_kit_domain::multiply(worlds[index], local)
        });
        worlds[node.index()] = world;
        pending.extend(node.children().map(|child| (child, Some(node.index()))));
    }
    if visited != worlds.len() {
        return Err("node hierarchy contains a cycle".into());
    }
    Ok(worlds)
}

fn decode_sprite_png(bytes: &[u8]) -> Result<(u32, u32, bool, Vec<u8>), String> {
    use png::{BitDepth, ColorType, DecodeOptions, Decoder, Limits, Transformations};
    use std::io::Cursor;

    let decode_error = |e: png::DecodingError| e.to_string();
    let mut options = DecodeOptions::default();
    options.set_ignore_checksums(false);
    options.set_skip_ancillary_crc_failures(false);
    options.set_ignore_text_chunk(true);
    options.set_ignore_iccp_chunk(true);
    let mut decoder = Decoder::new_with_options(Cursor::new(bytes), options);
    decoder.set_limits(Limits {
        bytes: 16 * 1024 * 1024,
    });
    let header = decoder.read_header_info().map_err(decode_error)?;
    let (width, height) = (header.width, header.height);
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || u64::from(width) * u64::from(height) > 4 * 1024 * 1024
    {
        return Err("PNG dimensions exceed sprite limits".into());
    }
    if header.bit_depth != BitDepth::Eight {
        return Err("sprite PNG requires 8-bit samples or palette indices".into());
    }
    if header.color_type != ColorType::Indexed {
        decoder.set_transformations(Transformations::EXPAND);
    }
    check_sprite_chunks(&bytes[8..])?;
    let mut reader = decoder.read_info().map_err(decode_error)?;
    let size = reader
        .output_buffer_size()
        .ok_or("PNG output size overflow")?;
    let mut decoded = vec![0; size];
    let frame = reader.next_frame(&mut decoded).map_err(decode_error)?;
    reader.finish().map_err(decode_error)?;
    let palette = reader.info().palette.as_deref();
    let alpha = reader.info().trns.as_deref();
    let color = frame.color_type;
    if let ColorType::Indexed = color {
        let palette = palette.ok_or("indexed PNG requires a palette")?;
        if palette.len() % 3 != 0 || alpha.is_some_and(|alpha| alpha.len() > palette.len() / 3) {
            return Err("invalid indexed PNG palette or alpha table".into());
        }
    }
    let has_alpha_channel = matches!(color, ColorType::Rgba | ColorType::GrayscaleAlpha)
        || (color == ColorType::Indexed && alpha.is_some());
    let rgba = if color == ColorType::Rgba {
        decoded
    } else {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for pixel in decoded.chunks_exact(color.samples()) {
            let channels = match color {
                ColorType::Rgb => [pixel[0], pixel[1], pixel[2], 255],
                ColorType::GrayscaleAlpha => [pixel[0], pixel[0], pixel[0], pixel[1]],
                ColorType::Grayscale => [pixel[0], pixel[0], pixel[0], 255],
                ColorType::Indexed => {
                    let index = usize::from(pixel[0]);
                    let rgb = palette
                        .and_then(|palette| palette.get(index * 3..index * 3 + 3))
                        .ok_or("PNG pixel index outside palette")?;
                    let opacity = alpha
                        .and_then(|alpha| alpha.get(index))
                        .copied()
                        .unwrap_or(255);
                    [rgb[0], rgb[1], rgb[2], opacity]
                }
                ColorType::Rgba => unreachable!("RGBA already decoded"),
            };
            rgba.extend_from_slice(&channels);
        }
        rgba
    };
    Ok((width, height, has_alpha_channel, rgba))
}

fn check_sprite_chunks(mut chunks: &[u8]) -> Result<(), &'static str> {
    while let Some((header, rest)) = chunks.split_first_chunk::<8>() {
        let len = u32::from_be_bytes(header[..4].try_into().expect("chunk length")) as usize;
        let (_, crc_and_tail) = rest.split_at_checked(len).ok_or("truncated PNG chunk")?;
        let (_, tail) = crc_and_tail
            .split_at_checked(4)
            .ok_or("truncated PNG CRC")?;
        match &header[4..] {
            b"acTL" | b"fcTL" | b"fdAT" => return Err("animated PNG unsupported"),
            b"IEND" => {
                return if len == 0 && tail.is_empty() {
                    Ok(())
                } else {
                    Err("invalid IEND or trailing PNG data")
                };
            }
            _ => chunks = tail,
        }
    }
    Err("missing PNG IEND")
}

#[derive(Debug)]
pub struct FaceAttachmentInputError {
    pub field: &'static str,
    pub detail: String,
}

pub fn inspect_face_attachment_file(
    file: &Path,
    spec: &Path,
    limits: ValidationLimits,
) -> Result<super::FaceAttachmentReport, FaceAttachmentInputError> {
    let invalid = |field, detail| FaceAttachmentInputError { field, detail };
    let bytes =
        super::read_bounded(file, limits).map_err(|err| invalid("file", err.to_string()))?;
    let spec = super::read_bounded(spec, limits).map_err(|err| invalid("spec", err.to_string()))?;
    let spec: super::face_attachment_domain::Spec =
        serde_json::from_slice(&spec).map_err(|err| invalid("spec", err.to_string()))?;
    spec.validate().map_err(|err| invalid("spec", err))?;
    face_attachment_report(&bytes, spec).map_err(|err| invalid("file", err))
}

fn face_attachment_report(
    bytes: &[u8],
    spec: super::face_attachment_domain::Spec,
) -> Result<super::FaceAttachmentReport, String> {
    use super::face_attachment_domain::{AttachmentPlacement, FaceFacts, FaceMode, distance};
    use super::static_kit_domain::multiply;
    use gltf::{Semantic, animation::Property};
    use std::collections::{BTreeMap, BTreeSet};

    let (gltf, _) = super::gltf_checks::parse_glb(bytes)?;
    let document = &gltf.document;
    let bin = gltf.blob.as_deref().unwrap_or_default();
    let raw = gltf::binary::Glb::from_slice(bytes).map_err(|err| err.to_string())?;
    let names: serde_json::Value =
        serde_json::from_slice(&raw.json).map_err(|err| err.to_string())?;
    let worlds = static_kit_world_transforms(document)?;
    if worlds.iter().flatten().flatten().any(|v| !v.is_finite()) {
        return Err("world transforms must be finite".into());
    }

    let name_at = |kind: &str, index: usize| names[kind][index]["name"].as_str();
    let mut node_names = BTreeMap::new();
    let mut parents = vec![None; worlds.len()];
    let mut deformed = vec![false; worlds.len()];
    let mut subtree_invalid = vec![false; worlds.len()];
    let mut mesh_targets = Vec::new();
    let mut mesh_deforms = Vec::new();
    let mut targets_by_name = Vec::new();
    let mut checked_morph_accessors = BTreeSet::new();
    let mut remaining_samples = 2_000_000_usize;
    for mesh in document.meshes() {
        let mut target_count = None;
        for primitive in mesh.primitives() {
            let count = primitive.morph_targets().len();
            if target_count
                .replace(count)
                .is_some_and(|previous| previous != count)
            {
                return Err("mesh primitives disagree on morph target count".into());
            }
            let vertices = primitive
                .get(&Semantic::Positions)
                .map(|accessor| accessor.count());
            for target in primitive.morph_targets() {
                let mut accessors = [target.positions(), target.normals(), target.tangents()]
                    .into_iter()
                    .flatten()
                    .peekable();
                if accessors.peek().is_none() {
                    return Err("morph target has no deformation accessor".into());
                }
                for accessor in accessors {
                    if accessor.dimensions() != gltf::accessor::Dimensions::Vec3
                        || Some(accessor.count()) != vertices
                    {
                        return Err("morph target accessor must be VEC3 and match its primitive vertex count".into());
                    }
                    if checked_morph_accessors.insert(accessor.index()) {
                        check_face_accessor(&accessor, bin, &mut remaining_samples)?;
                    }
                }
            }
        }
        let count = target_count.unwrap_or_default();
        mesh_targets.push(count);
        mesh_deforms.push(
            mesh.weights().is_some()
                || count != 0
                || mesh.primitives().any(|primitive| {
                    primitive.attributes().any(|(semantic, _)| {
                        matches!(semantic, Semantic::Joints(_) | Semantic::Weights(_))
                    })
                }),
        );
        let target_names = names["meshes"][mesh.index()]["extras"]["targetNames"].as_array();
        targets_by_name.push(
            target_names
                .into_iter()
                .flatten()
                .take(count)
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>(),
        );
    }

    let mut bound_meshes = BTreeSet::new();
    for node in document.nodes() {
        if let Some(name) = name_at("nodes", node.index()) {
            node_names
                .entry(name)
                .and_modify(|index| *index = None)
                .or_insert(Some(node.index()));
        }
        for child in node.children() {
            parents[child.index()] = Some(node.index());
        }
        deformed[node.index()] = node.skin().is_some() || node.weights().is_some();
        if let Some(mesh) = node.mesh() {
            bound_meshes.insert(mesh.index());
            deformed[node.index()] |= mesh_deforms[mesh.index()];
        }
    }

    let find_node = |name: &str| node_names.get(name).copied().flatten();
    let face_enabled = spec.face_mode != FaceMode::NotApplicable;
    let mut eye_jaw = BTreeSet::new();
    let mut valid_eye_jaw_nodes = true;
    if face_enabled {
        for name in spec.eye_jaw_names() {
            valid_eye_jaw_nodes &= find_node(name).is_some_and(|index| eye_jaw.insert(index));
        }
    }
    let expected: BTreeSet<_> = spec
        .expected_expressions
        .iter()
        .map(String::as_str)
        .collect();
    let mut facts = FaceFacts {
        morph_target_count: bound_meshes.iter().map(|&index| mesh_targets[index]).sum(),
        expressions: bound_meshes
            .iter()
            .flat_map(|&index| &targets_by_name[index])
            .copied()
            .filter(|name| expected.contains(name))
            .map(str::to_owned)
            .collect(),
        playable_expressions: BTreeSet::new(),
        valid_eye_jaw_nodes,
        playable: false,
    };
    for animation in document.animations() {
        for channel in animation.channels() {
            let target = channel.target();
            let node = target.node();
            deformed[node.index()] = true;
            if !face_enabled {
                continue;
            }
            let weights = target.property() == Property::MorphTargetWeights;
            if !weights && !eye_jaw.contains(&node.index()) {
                continue;
            }
            let mesh = node.mesh();
            let width = if weights {
                mesh.as_ref().map_or(0, |mesh| mesh_targets[mesh.index()])
            } else {
                1
            };
            let playable = face_track_playable(&channel, width, bin, &mut remaining_samples)?;
            facts.playable |= playable;
            let clip = &names["animations"][animation.index()];
            let raw_channel = &clip["channels"][channel.index()];
            let names = [
                name_at("animations", animation.index()),
                raw_channel["extras"]["name"].as_str(),
                raw_channel["name"].as_str(),
                name_at("nodes", node.index()),
            ];
            let morph_names = mesh
                .as_ref()
                .filter(|_| weights)
                .into_iter()
                .flat_map(|mesh| &targets_by_name[mesh.index()]);
            for name in names
                .into_iter()
                .flatten()
                .chain(morph_names.copied())
                .filter(|name| expected.contains(name))
            {
                if !facts.expressions.contains(name) {
                    facts.expressions.insert(name.to_owned());
                }
                if playable && !facts.playable_expressions.contains(name) {
                    facts.playable_expressions.insert(name.to_owned());
                }
            }
        }
    }
    for (index, &animated_or_deformed) in deformed.iter().enumerate() {
        if animated_or_deformed {
            let mut current = Some(index);
            while let Some(node) = current {
                if std::mem::replace(&mut subtree_invalid[node], true) {
                    break;
                }
                current = parents[node];
            }
        }
    }

    let local = |index| {
        document
            .nodes()
            .nth(index)
            .expect("validated node index")
            .transform()
            .matrix()
            .map(|column| column.map(f64::from))
    };
    let mut remaining_ancestors = 2_000_000_usize;
    let mut results = Vec::new();
    for attachment in &spec.rigid_attachments {
        let placement = match (
            find_node(&attachment.socket_node),
            find_node(&attachment.attachment_node),
        ) {
            (Some(socket), Some(part)) if socket != part => {
                let mut transform = local(part);
                let mut current = part;
                let mut rigid = !subtree_invalid[part];
                while let Some(parent) = parents[current].filter(|&parent| parent != socket) {
                    remaining_ancestors = remaining_ancestors
                        .checked_sub(1)
                        .ok_or("attachment hierarchy budget exceeded")?;
                    transform = multiply(local(parent), transform);
                    rigid &= !deformed[parent];
                    current = parent;
                }
                (parents[current] == Some(socket)).then(|| {
                    let mut expected = local(part);
                    expected[3] = [0.0, 0.0, 0.0, 1.0];
                    expected[3][..3].copy_from_slice(&attachment.expected_offset());
                    let translation =
                        |m: super::static_kit_domain::Matrix| [m[3][0], m[3][1], m[3][2]];
                    AttachmentPlacement {
                        socket_offset_m: translation(transform),
                        world_drift_m: distance(
                            translation(worlds[part]),
                            translation(multiply(worlds[socket], expected)),
                        ),
                        rigid,
                    }
                })
            }
            _ => None,
        };
        results.push(attachment.inspect(placement)?);
    }
    spec.inspect(facts, results)
}

fn face_track_playable(
    channel: &gltf::animation::Channel<'_>,
    width: usize,
    bin: &[u8],
    budget: &mut usize,
) -> Result<bool, String> {
    use gltf::{
        accessor::{DataType, Dimensions},
        animation::{Interpolation, Property, util::ReadOutputs},
    };
    let sampler = channel.sampler();
    let input = sampler.input();
    let output = sampler.output();
    for accessor in [&input, &output] {
        check_face_accessor(accessor, bin, budget)?;
    }
    let property = channel.target().property();
    let dimensions = match property {
        Property::MorphTargetWeights => Dimensions::Scalar,
        Property::Rotation => Dimensions::Vec4,
        Property::Translation | Property::Scale => Dimensions::Vec3,
    };
    let normalized_output = matches!(property, Property::Rotation | Property::MorphTargetWeights)
        && output.normalized()
        && matches!(
            output.data_type(),
            DataType::I8 | DataType::U8 | DataType::I16 | DataType::U16
        );
    let multiplier = if sampler.interpolation() == Interpolation::CubicSpline {
        3
    } else {
        1
    };
    if width == 0
        || input.dimensions() != Dimensions::Scalar
        || input.data_type() != DataType::F32
        || output.dimensions() != dimensions
        || (output.data_type() != DataType::F32 && !normalized_output)
        || input
            .count()
            .checked_mul(width)
            .and_then(|count| count.checked_mul(multiplier))
            != Some(output.count())
    {
        return Err(
            "facial animation accessor type or sample count does not match its channel".into(),
        );
    }
    let reader = channel.reader(|_| Some(bin));
    let mut previous = -1.0_f32;
    for time in reader
        .read_inputs()
        .ok_or("missing facial animation times")?
    {
        if !time.is_finite() || time < 0.0 || time <= previous {
            return Err(
                "facial animation times must be finite, nonnegative and strictly increasing".into(),
            );
        }
        previous = time;
    }
    let finite = match reader
        .read_outputs()
        .ok_or("missing facial animation values")?
    {
        ReadOutputs::MorphTargetWeights(values) => values.into_f32().all(f32::is_finite),
        ReadOutputs::Rotations(values) => values.into_f32().flatten().all(f32::is_finite),
        ReadOutputs::Translations(values) | ReadOutputs::Scales(values) => {
            values.flatten().all(f32::is_finite)
        }
    };
    if !finite {
        return Err("facial animation values must be finite".into());
    }
    Ok(input.count() >= 2)
}

fn check_face_accessor(
    accessor: &gltf::Accessor<'_>,
    bin: &[u8],
    budget: &mut usize,
) -> Result<(), String> {
    *budget = budget
        .checked_sub(accessor.count())
        .ok_or("facial accessor budget exceeded")?;
    if let Some(view) = accessor.view() {
        face_accessor_data(
            view,
            accessor.offset(),
            accessor.count(),
            accessor.size(),
            bin,
        )?;
    } else if accessor.sparse().is_none() {
        return Err("facial accessor needs a buffer view or sparse data".into());
    }
    if let Some(sparse) = accessor.sparse() {
        if sparse.count() > accessor.count() {
            return Err("sparse facial accessor count exceeds its accessor".into());
        }
        let indices = sparse.indices();
        let values = sparse.values();
        let width = indices.index_type().size();
        let index_view = indices.view();
        let value_view = values.view();
        if index_view.stride().is_some() || value_view.stride().is_some() {
            return Err("sparse facial accessor views must be tightly packed".into());
        }
        let index_data =
            face_accessor_data(index_view, indices.offset(), sparse.count(), width, bin)?;
        face_accessor_data(
            value_view,
            values.offset(),
            sparse.count(),
            accessor.size(),
            bin,
        )?;
        let mut previous = None;
        for chunk in index_data.chunks_exact(width) {
            let mut bytes = [0; 4];
            bytes[..width].copy_from_slice(chunk);
            let index = u32::from_le_bytes(bytes) as usize;
            if index >= accessor.count() || previous.is_some_and(|last| last >= index) {
                return Err(
                    "sparse facial accessor indices must ascend and fit the accessor".into(),
                );
            }
            previous = Some(index);
        }
    }
    Ok(())
}

fn face_accessor_data<'a>(
    view: gltf::buffer::View<'_>,
    offset: usize,
    count: usize,
    size: usize,
    bin: &'a [u8],
) -> Result<&'a [u8], String> {
    let buffer = view.buffer();
    let stride = view.stride().unwrap_or(size);
    let view_end = view.offset().checked_add(view.length());
    let end = count
        .checked_sub(1)
        .and_then(|count| count.checked_mul(stride))
        .and_then(|end| end.checked_add(offset))
        .and_then(|end| end.checked_add(size));
    if buffer.index() != 0
        || !matches!(buffer.source(), gltf::buffer::Source::Bin)
        || buffer.length() > bin.len()
        || view_end.is_none_or(|end| end > buffer.length())
        || stride < size
        || end.is_none_or(|end| end > view.length())
    {
        return Err(
            "facial accessor exceeds its embedded buffer or has an invalid stride or count".into(),
        );
    }
    Ok(&bin[view.offset() + offset..view.offset() + end.expect("validated accessor end")])
}

#[derive(Debug)]
pub struct ClothInputError {
    pub field: &'static str,
    pub detail: String,
}

pub fn inspect_cloth_file(
    file: &Path,
    spec: &Path,
    limits: ValidationLimits,
) -> Result<super::ClothReport, ClothInputError> {
    let invalid = |field, detail| ClothInputError { field, detail };
    let bytes = super::read_bounded(file, limits).map_err(|e| invalid("file", e.to_string()))?;
    let spec = super::read_bounded(spec, limits).map_err(|e| invalid("spec", e.to_string()))?;
    let spec: super::cloth_domain::Spec =
        serde_json::from_slice(&spec).map_err(|e| invalid("spec", e.to_string()))?;
    let motion = spec.measure().map_err(|e| invalid("spec", e))?;
    let facts = cloth_binding_facts(&bytes, &spec).map_err(|e| invalid("file", e))?;
    let garments = spec
        .garments
        .into_iter()
        .zip(facts)
        .zip(motion)
        .map(|((garment, facts), motion)| garment.report(facts, motion))
        .collect();
    super::ClothReport::from_garments(garments).map_err(|e| invalid("spec", e))
}

fn cloth_binding_facts(
    bytes: &[u8],
    spec: &super::cloth_domain::Spec,
) -> Result<Vec<super::cloth_domain::BindingFacts>, String> {
    use super::cloth_domain::BindingFacts;
    use gltf::{
        Semantic,
        accessor::{DataType, Dimensions},
        animation::Property,
    };
    use std::collections::{BTreeMap, BTreeSet};

    const CLOTH_INSPECTION_BUDGET: usize = 2_000_000;
    let parsed = super::gltf_checks::parse_glb(bytes);
    let raw = gltf::binary::Glb::from_slice(bytes).map_err(|err| err.to_string())?;
    let (gltf, required) = match parsed {
        Ok(result) => result,
        Err(error) => {
            // Only a validated document with unsupported required extensions earns a receipt.
            if !error.starts_with("gltf parser rejected the GLB structure: invalid glTF:") {
                return Err(error);
            }
            let mut root: gltf::json::Root =
                serde_json::from_slice(&raw.json).map_err(|err| err.to_string())?;
            let required = root.extensions_required.clone();
            root.extensions_required
                .retain(|name| gltf::json::extensions::ENABLED_EXTENSIONS.contains(&name.as_str()));
            if required.len() == root.extensions_required.len() {
                return Err(error);
            }
            let document = gltf::Document::from_json(root).map_err(|_| error)?;
            (
                gltf::Gltf {
                    document,
                    blob: None,
                },
                required,
            )
        }
    };
    let json: serde_json::Value =
        serde_json::from_slice(&raw.json).map_err(|err| err.to_string())?;
    let bin = raw.bin.as_deref().unwrap_or_default();
    let physics = |name: &str| {
        let name = name.to_ascii_lowercase();
        name.contains("cloth") || name.contains("physics")
    };
    let raw_extensions = json["extensions"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, _)| name.as_str());
    let physics_extension = gltf
        .document
        .extensions_used()
        .chain(raw_extensions)
        .find(|name| physics(name));
    let unsupported_required = required.iter().find(|name| {
        name.as_str() != super::gltf_checks::WEBP_EXTENSION
            && !gltf::json::extensions::ENABLED_EXTENSIONS.contains(&name.as_str())
    });
    if let Some(name) = unsupported_required
        .map(String::as_str)
        .or(physics_extension)
    {
        let reason = format!("UnsupportedExtension:{name}");
        if reason.len().saturating_mul(spec.garments.len()) > CLOTH_INSPECTION_BUDGET {
            return Err("cloth extension receipt budget exceeded".into());
        }
        return Ok(spec
            .garments
            .iter()
            .map(|_| BindingFacts {
                unsupported: BTreeSet::from([reason.clone()]),
                ..BindingFacts::default()
            })
            .collect());
    }

    let mut nodes = BTreeMap::new();
    for node in gltf.document.nodes() {
        if let Some(name) = json["nodes"][node.index()]["name"].as_str() {
            nodes
                .entry(name)
                .and_modify(|value| *value = None)
                .or_insert(Some(node));
        }
    }
    let morph_animated: BTreeSet<_> = gltf
        .document
        .animations()
        .flat_map(|animation| animation.channels())
        .filter(|channel| channel.target().property() == Property::MorphTargetWeights)
        .map(|channel| channel.target().node().index())
        .collect();

    let mut budget = CLOTH_INSPECTION_BUDGET;
    let mut results = Vec::with_capacity(spec.garments.len());
    for garment in &spec.garments {
        let mut facts = BindingFacts::default();
        for (name, is_garment) in [
            (garment.body_mesh_node.as_str(), false),
            (garment.node_name.as_str(), true),
        ] {
            let node = nodes.get(name).and_then(Option::as_ref);
            let Some((node, mesh)) = node.and_then(|node| node.mesh().map(|mesh| (node, mesh)))
            else {
                facts.unsupported.insert(
                    if is_garment {
                        "MissingOrAmbiguousGarmentMesh"
                    } else {
                        "MissingOrAmbiguousBodyMesh"
                    }
                    .into(),
                );
                continue;
            };
            let raw_node = &json["nodes"][node.index()];
            let raw_mesh = &json["meshes"][mesh.index()];
            if node.weights().is_some()
                || mesh.weights().is_some()
                || morph_animated.contains(&node.index())
            {
                facts.unsupported.insert("MorphDeformation".into());
            }
            for structure in [raw_node, raw_mesh] {
                check_cloth_extensions(
                    structure,
                    garment.require_supported_structure,
                    &mut facts,
                    &mut budget,
                )?;
            }

            let mut rigid = node.skin().is_none();
            let mut skinned = node.skin().is_some();
            let mut primitives = mesh.primitives().peekable();
            if primitives.peek().is_none() {
                facts.unsupported.insert("EmptyMesh".into());
                continue;
            }

            let joint_count = if is_garment {
                cloth_joint_count(node, bin, &mut budget, &mut facts)?
            } else {
                None
            };
            for part in primitives {
                budget = budget
                    .checked_sub(1)
                    .ok_or("cloth accessor budget exceeded")?;
                check_cloth_extensions(
                    &raw_mesh["primitives"][part.index()],
                    garment.require_supported_structure,
                    &mut facts,
                    &mut budget,
                )?;
                if part.morph_targets().len() > 0 {
                    facts.unsupported.insert("MorphDeformation".into());
                }
                let positions = part
                    .get(&Semantic::Positions)
                    .ok_or("cloth mesh primitive is missing POSITION")?;
                if positions.dimensions() != Dimensions::Vec3
                    || positions.data_type() != DataType::F32
                    || positions.normalized()
                {
                    facts.unsupported.insert("InvalidPositionLayout".into());
                    continue;
                }
                check_face_accessor(&positions, bin, &mut budget)?;
                let reader = part.reader(|_| Some(bin));
                if reader
                    .read_positions()
                    .ok_or("invalid cloth POSITION accessor")?
                    .flatten()
                    .any(|value| !value.is_finite())
                {
                    return Err("cloth POSITION values must be finite".into());
                }
                if part.attributes().any(|(semantic, _)| {
                    matches!(semantic, Semantic::Joints(1..) | Semantic::Weights(1..))
                }) {
                    facts.unsupported.insert("AdditionalJointWeightSets".into());
                    skinned = false;
                    rigid = false;
                }
                if !is_garment {
                    continue;
                }

                let joints = part.get(&Semantic::Joints(0));
                let weights = part.get(&Semantic::Weights(0));
                rigid &= joints.is_none() && weights.is_none();
                skinned &= joints.is_some() && weights.is_some();
                match (joint_count, joints, weights) {
                    (Some(count), Some(joints), Some(weights)) => {
                        check_cloth_weights(
                            &part,
                            [joints, weights],
                            positions.count(),
                            count,
                            bin,
                            &mut budget,
                            &mut facts,
                        )?;
                    }
                    (None, None, None) if node.skin().is_none() => {}
                    _ => {
                        facts.unsupported.insert("IncompleteSkinBinding".into());
                    }
                }
            }
            if is_garment {
                use super::GarmentBinding::{Rigid, Skinned};
                facts.classification = match (rigid, skinned) {
                    (true, false) => Some(Rigid),
                    (false, true) => Some(Skinned),
                    _ => None,
                };
            }
        }
        results.push(facts);
    }
    Ok(results)
}

fn check_cloth_extensions(
    structure: &serde_json::Value,
    strict: bool,
    facts: &mut super::cloth_domain::BindingFacts,
    budget: &mut usize,
) -> Result<(), String> {
    if let Some(extensions) = structure["extensions"].as_object() {
        for name in extensions.keys() {
            *budget = budget
                .checked_sub(name.len().saturating_add(1))
                .ok_or("cloth extension budget exceeded")?;
            let lower = name.to_ascii_lowercase();
            if lower.contains("cloth")
                || lower.contains("physics")
                || matches!(
                    name.as_str(),
                    "KHR_draco_mesh_compression"
                        | "EXT_meshopt_compression"
                        | "EXT_mesh_gpu_instancing"
                )
                || (strict && name != "KHR_materials_variants")
            {
                facts
                    .unsupported
                    .insert(format!("UnsupportedExtension:{name}"));
            }
        }
    }
    Ok(())
}

fn cloth_joint_count(
    node: &gltf::Node<'_>,
    bin: &[u8],
    budget: &mut usize,
    facts: &mut super::cloth_domain::BindingFacts,
) -> Result<Option<usize>, String> {
    use gltf::accessor::{DataType, Dimensions};

    let Some(skin) = node.skin() else {
        return Ok(None);
    };
    let count = skin.joints().len();
    *budget = budget
        .checked_sub(count)
        .ok_or("cloth accessor budget exceeded")?;
    let unique: std::collections::BTreeSet<_> = skin.joints().map(|joint| joint.index()).collect();
    if count == 0 || unique.len() != count {
        facts
            .unsupported
            .insert("MissingOrDuplicateSkinJoints".into());
        return Ok(None);
    }
    if let Some(accessor) = skin.inverse_bind_matrices() {
        if accessor.count() != count
            || accessor.dimensions() != Dimensions::Mat4
            || accessor.data_type() != DataType::F32
            || accessor.normalized()
        {
            facts
                .unsupported
                .insert("InvalidInverseBindMatrices".into());
        } else {
            check_face_accessor(&accessor, bin, budget)?;
            let reader = skin.reader(|_| Some(bin));
            if reader
                .read_inverse_bind_matrices()
                .ok_or("invalid inverse bind matrix accessor")?
                .flatten()
                .flatten()
                .any(|value| !value.is_finite())
            {
                return Err("inverse bind matrices must be finite".into());
            }
        }
    }
    Ok(Some(count))
}

fn check_cloth_weights(
    part: &gltf::Primitive<'_>,
    [joints, weights]: [gltf::Accessor<'_>; 2],
    vertices: usize,
    joint_count: usize,
    bin: &[u8],
    budget: &mut usize,
    facts: &mut super::cloth_domain::BindingFacts,
) -> Result<(), String> {
    use gltf::{
        accessor::{DataType, Dimensions},
        mesh::util::ReadWeights,
    };

    let is_uint = |ty| matches!(ty, DataType::U8 | DataType::U16);
    if [&joints, &weights]
        .into_iter()
        .any(|accessor| accessor.count() != vertices || accessor.dimensions() != Dimensions::Vec4)
        || joints.normalized()
        || !is_uint(joints.data_type())
        || !(weights.data_type() == DataType::F32 && !weights.normalized()
            || is_uint(weights.data_type()) && weights.normalized())
    {
        facts.unsupported.insert("InvalidSkinAccessorLayout".into());
        return Ok(());
    }
    check_face_accessor(&joints, bin, budget)?;
    check_face_accessor(&weights, bin, budget)?;
    let reader = part.reader(|_| Some(bin));
    let joints = reader
        .read_joints(0)
        .ok_or("invalid JOINTS_0 accessor")?
        .into_u16();
    let mut weights = reader.read_weights(0).ok_or("invalid WEIGHTS_0 accessor")?;
    for joints in joints {
        let (weights, expected_sum) = match &mut weights {
            ReadWeights::U8(values) => (values.next().map(|v| v.map(f64::from)), 255.0),
            ReadWeights::U16(values) => (values.next().map(|v| v.map(f64::from)), 65_535.0),
            ReadWeights::F32(values) => (values.next().map(|v| v.map(f64::from)), 1.0),
        };
        let weights = weights.ok_or("WEIGHTS_0 ended before JOINTS_0")?;
        if weights.iter().any(|weight| !weight.is_finite()) {
            return Err("WEIGHTS_0 values must be finite".into());
        }
        if joints
            .into_iter()
            .any(|joint| usize::from(joint) >= joint_count)
        {
            facts.unsupported.insert("OutOfRangeSkinJoint".into());
        }
        let sum: f64 = weights.into_iter().sum();
        facts.invalid_weights |= sum <= 0.0
            || weights
                .iter()
                .any(|weight| !(0.0..=expected_sum).contains(weight));
        facts.weight_sum_error = facts
            .weight_sum_error
            .max((sum - expected_sum).abs() / expected_sum);
    }
    Ok(())
}
