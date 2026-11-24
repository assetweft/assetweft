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
