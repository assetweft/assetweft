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
