use super::{
    domain::{CheckedBytes, ComponentFacts},
    ports::StructureCheck,
};
use gltf::{Gltf, binary::Glb, json};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

fn check_glb_container(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 12 {
        return Err(format!("too short for a GLB header: {} bytes", bytes.len()));
    }
    if &bytes[0..4] != b"glTF" {
        return Err(
            "magic is not glTF; plain JSON or other formats are outside the declared GLB scope"
                .to_owned(),
        );
    }
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if version != 2 {
        return Err(format!(
            "unsupported GLB container version {version}; only 2 is in scope"
        ));
    }
    let declared = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    if declared != bytes.len() {
        return Err(format!(
            "declared length {declared} != actual {} bytes",
            bytes.len()
        ));
    }
    let mut offset = 12usize;
    let mut chunk_index = 0usize;
    while offset < bytes.len() {
        if offset + 8 > bytes.len() {
            return Err("truncated chunk header".to_owned());
        }
        let len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let ty = &bytes[offset + 4..offset + 8];
        if chunk_index == 0 && ty != b"JSON" {
            return Err("first GLB chunk must be JSON".to_owned());
        }
        // glTF 2.0.1 §4.4.3: JSON exactly once at index 0, optional BIN
        // only at index 1. Unknown extension chunks remain allowed in the tail.
        if ty == b"JSON" && chunk_index != 0 {
            return Err("JSON chunk must occur exactly once, first".to_owned());
        }
        if ty == b"BIN\0" && chunk_index != 1 {
            return Err("BIN chunk may occur only once, second".to_owned());
        }
        chunk_index += 1;
        if !len.is_multiple_of(4) {
            return Err(format!("chunk length {len} is not 4-byte aligned"));
        }
        let start = offset + 8;
        if start + len > bytes.len() {
            return Err("chunk length exceeds the file".to_owned());
        }
        offset = start + len;
    }
    Ok(())
}

pub(super) const WEBP_EXTENSION: &str = "EXT_texture_webp";
const WEBP_MIME: &[&str] = &["image/webp"];
const FALLBACK_MIME: &[&str] = &["image/png", "image/jpeg"];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextureDeclarations {
    #[serde(default)]
    extensions_used: Vec<String>,
    #[serde(default)]
    extensions_required: Vec<String>,
    #[serde(default)]
    textures: Vec<TextureDeclaration>,
}

#[derive(Deserialize)]
struct TextureDeclaration {
    source: Option<u32>,
    extensions: Option<TextureExtensions>,
}

#[derive(Deserialize)]
struct TextureExtensions {
    #[serde(
        default,
        rename = "EXT_texture_webp",
        deserialize_with = "deserialize_webp_source"
    )]
    webp: Option<u32>,
}

fn deserialize_webp_source<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u32>, D::Error> {
    #[derive(Deserialize)]
    struct WebpReference {
        source: u32,
    }
    let raw = <&RawValue>::deserialize(deserializer)?;
    if !raw.get().starts_with('{') {
        return Err(serde::de::Error::custom(
            "EXT_texture_webp must be an object",
        ));
    }
    serde_json::from_str::<WebpReference>(raw.get())
        .map(|reference| Some(reference.source))
        .map_err(serde::de::Error::custom)
}

fn declared_uri_mime<'a>(uri: &'a str, mime: Option<&'a str>) -> Option<&'a str> {
    if uri.is_empty()
        || uri.chars().any(|c| c.is_whitespace() || c.is_control())
        || uri.match_indices('%').any(|(i, _)| {
            !uri.as_bytes()
                .get(i + 1..i + 3)
                .is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit))
        })
    {
        return None;
    }
    if let Some((scheme, data)) = uri.split_once(':')
        && scheme.eq_ignore_ascii_case("data")
    {
        let (metadata, payload) = data.split_once(',')?;
        let media_type = metadata.split(';').next()?;
        return (!payload.is_empty()
            && mime.is_none_or(|mime| media_type.eq_ignore_ascii_case(mime)))
        .then_some(media_type);
    }
    mime.or_else(|| {
        let path = uri.split(['?', '#']).next()?;
        let path = match path
            .strip_prefix("//")
            .or_else(|| path.split_once("://").map(|(_, rest)| rest))
        {
            Some(authority_and_path) => authority_and_path.split_once('/')?.1,
            None => path,
        };
        let (_, extension) = path.rsplit('/').next()?.rsplit_once('.')?;
        match extension.to_ascii_lowercase().as_str() {
            "webp" => Some("image/webp"),
            "png" => Some("image/png"),
            "jpg" | "jpeg" => Some("image/jpeg"),
            _ => None,
        }
    })
}

fn declares_image(root: &json::Root, bin: Option<&[u8]>, source: u32, formats: &[&str]) -> bool {
    let Some(image) = root.images.get(source as usize) else {
        return false;
    };
    let mime = image.mime_type.as_ref().map(|mime| mime.0.as_str());
    if mime.is_some_and(|mime| !formats.contains(&mime)) {
        return false;
    }
    match (image.buffer_view, image.uri.as_deref()) {
        (None, Some(uri)) => declared_uri_mime(uri, mime).is_some_and(|mime| {
            formats
                .iter()
                .any(|format| mime.eq_ignore_ascii_case(format))
        }),
        (Some(index), None) => {
            let Some(view) = root.get(index) else {
                return false;
            };
            let Some(buffer) = root.get(view.buffer) else {
                return false;
            };
            mime.is_some()
                && view.byte_length.0 > 0
                && view
                    .byte_offset
                    .map_or(0, |offset| offset.0)
                    .checked_add(view.byte_length.0)
                    .is_some_and(|end| {
                        end <= buffer.byte_length.0
                            && (buffer.uri.is_some()
                                || bin.is_some_and(|bin| end <= bin.len() as u64))
                    })
        }
        _ => false,
    }
}

pub(super) fn parse_glb(bytes: &[u8]) -> Result<(Gltf, Vec<String>), String> {
    check_glb_container(bytes)?;
    let parser_error = |err: gltf::Error| format!("gltf parser rejected the GLB structure: {err}");
    let glb = Glb::from_slice(bytes).map_err(parser_error)?;
    let declarations: TextureDeclarations =
        serde_json::from_slice(&glb.json).map_err(|err| parser_error(err.into()))?;
    let used = declarations
        .extensions_used
        .iter()
        .filter(|&ext| ext == WEBP_EXTENSION)
        .count();
    let required = declarations
        .extensions_required
        .iter()
        .filter(|&ext| ext == WEBP_EXTENSION)
        .count();
    let sources = declarations
        .textures
        .iter()
        .enumerate()
        .filter_map(|(index, texture)| {
            let source = texture.extensions.as_ref()?.webp?;
            Some((index, source, texture.source))
        });
    if sources.clone().next().is_none() && used == 0 && required == 0 {
        return Ok((
            Gltf::from_slice(bytes).map_err(parser_error)?,
            declarations.extensions_required,
        ));
    }
    if used != 1 || required > 1 || sources.clone().next().is_none() {
        return Err(format!(
            "{WEBP_EXTENSION} must occur on a texture and be listed once in extensionsUsed and at most once in extensionsRequired"
        ));
    }

    let mut root: json::Root =
        json::deserialize::from_slice(&glb.json).map_err(|err| parser_error(err.into()))?;
    for (index, source, fallback) in sources.clone() {
        let path = format!("textures[{index}].extensions.{WEBP_EXTENSION}");
        if !declares_image(&root, glb.bin.as_deref(), source, WEBP_MIME) {
            return Err(format!(
                "{path}.source must reference a declared WebP image"
            ));
        }
        if let Some(fallback) = fallback {
            if !declares_image(&root, glb.bin.as_deref(), fallback, FALLBACK_MIME) {
                return Err(format!(
                    "textures[{index}].source must reference a PNG or JPEG fallback"
                ));
            }
        } else if required == 0 {
            return Err(format!(
                "{path} without a fallback must be listed in extensionsRequired"
            ));
        }
    }

    // gltf 1.4.1 validates core texture sources but does not recognize required WebP.
    for (index, source, _) in sources {
        root.textures[index].source = json::Index::new(source);
    }
    root.extensions_required.retain(|ext| ext != WEBP_EXTENSION);
    let normalized = serde_json::to_vec(&root).map_err(|err| parser_error(err.into()))?;
    let mut gltf = Gltf::from_slice(&normalized).map_err(parser_error)?;
    gltf.blob = glb.bin.map(|bin| bin.into_owned());
    Ok((gltf, declarations.extensions_required))
}

pub(crate) struct GltfChecks;

impl StructureCheck for GltfChecks {
    fn check(&self, bytes: &[u8]) -> CheckedBytes {
        let digest = Sha256::digest(bytes);
        // 只做十六进制展示（sha2 0.11 的摘要类型不再实现 LowerHex）；哈希算法仍由 sha2 提供。
        let file_sha256 = digest
            .iter()
            .fold(String::with_capacity(64), |mut acc, byte| {
                use std::fmt::Write as _;
                let _ = write!(acc, "{byte:02x}");
                acc
            });
        let structure = parse_glb(bytes).map(|(gltf, _)| {
            let document = gltf.document;
            let mesh_count = document.meshes().count();
            let skin_count = document.skins().count();
            let animation_count = document.animations().count();
            let has_bound_skinned_mesh = document
                .nodes()
                .any(|n| n.mesh().is_some() && n.skin().is_some());
            let bound_skins: HashSet<usize> = document
                .nodes()
                .filter(|n| n.mesh().is_some())
                .filter_map(|n| n.skin().map(|s| s.index()))
                .collect();
            let joints: HashSet<usize> = document
                .skins()
                .filter(|s| bound_skins.contains(&s.index()))
                .flat_map(|s| s.joints())
                .map(|n| n.index())
                .collect();
            let has_joint_animation = document.animations().any(|a| {
                a.channels()
                    .any(|c| joints.contains(&c.target().node().index()))
            });

            ComponentFacts {
                mesh_count,
                skin_count,
                animation_count,
                has_bound_skinned_mesh,
                has_joint_animation,
            }
        });
        CheckedBytes {
            file_sha256,
            bytes_read: bytes.len(),
            structure,
        }
    }
}
