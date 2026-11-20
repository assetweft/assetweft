//! Catalog PNG thumbnail codec. The managed-file adapter supplies verified, bounded bytes.
//! Decode the whole static image before producing a deterministic nearest-neighbour preview.
use crate::storage::StorageError;
use png::{BitDepth, ColorType, DecodeOptions, Decoder, Encoder, Limits};
use std::io::Cursor;

pub(super) fn thumbnail(source: &[u8]) -> Result<Vec<u8>, StorageError> {
    let invalid = |_| StorageError::Invalid("malformed PNG");
    let mut options = DecodeOptions::default();
    options.set_ignore_iccp_chunk(true); // Rejected by chunk presence, without inflating the profile.
    options.set_ignore_checksums(false);
    options.set_skip_ancillary_crc_failures(false);
    let mut cursor = Cursor::new(source);
    let mut decoder = Decoder::new_with_options(&mut cursor, options);
    decoder.set_limits(Limits {
        bytes: 16 * 1024 * 1024,
    });
    let header = decoder.read_header_info().map_err(invalid)?;
    let (w, h) = (header.width, header.height);
    if w > 4096 || h > 4096 || u64::from(w) * u64::from(h) > 4 * 1024 * 1024 {
        return Err(StorageError::Invalid(
            "PNG dimensions exceed preview limits",
        ));
    }
    if (header.color_type, header.bit_depth) != (ColorType::Rgba, BitDepth::Eight) {
        return Err(StorageError::Invalid("8-bit RGBA PNG required"));
    }
    let mut reader = decoder.read_info().map_err(invalid)?;
    if reader.info().animation_control.is_some() {
        return Err(StorageError::Invalid("animated PNG unsupported"));
    }
    let info = reader.info();
    check_color_chunks(source, info)?;
    let (gamma, chromaticities, srgb) = (info.gama_chunk, info.chrm_chunk, info.srgb);
    let mut pixels = vec![0; (w * h * 4) as usize];
    reader.next_frame(&mut pixels).map_err(invalid)?;
    reader.finish().map_err(invalid)?;
    if cursor.position() != source.len() as u64 {
        return Err(StorageError::Invalid("trailing PNG data"));
    }

    let longest = w.max(h);
    let edge = longest.min(128);
    let scale = |n: u32| ((n * edge + longest / 2) / longest).max(1);
    let (width, height) = (scale(w), scale(h));
    let mut sampled = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let offset = ((y * h / height * w + x * w / width) * 4) as usize;
            sampled.extend_from_slice(&pixels[offset..offset + 4]);
        }
    }
    let mut output = Vec::new();
    let mut encoder = Encoder::new(&mut output, width, height);
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    if let Some(intent) = srgb {
        encoder.set_source_srgb(intent);
    }
    if let Some(value) = gamma {
        encoder.set_source_gamma(value);
    }
    if let Some(value) = chromaticities {
        encoder.set_source_chromaticities(value);
    }
    let encode = |_| StorageError::Invalid("PNG encoding failed");
    let mut writer = encoder.write_header().map_err(encode)?;
    writer.write_image_data(&sampled).map_err(encode)?;
    writer.finish().map_err(encode)?;
    Ok(output)
}

// png 0.18.1 can discard invalid ancillary chunks and still return Info successfully.
// Check actual declarations against what the encoder can faithfully reproduce.
fn check_color_chunks(source: &[u8], info: &png::Info<'_>) -> Result<(), StorageError> {
    let malformed = || StorageError::Invalid("malformed PNG chunk");
    let mut chunks = &source[8..]; // read_info already verified the signature.
    let mut gamma = info.gama_chunk;
    let mut chromaticities = info.chrm_chunk;
    let mut srgb = info.srgb;
    while !chunks.is_empty() {
        let (header, rest) = chunks.split_first_chunk::<8>().ok_or_else(malformed)?;
        let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        chunks = rest
            .get(length..)
            .and_then(|tail| tail.get(4..))
            .ok_or_else(malformed)?;
        let supported = match &header[4..] {
            b"gAMA" => length == 4 && gamma.take().is_some_and(|g| Some(g) == info.gamma()),
            b"cHRM" => {
                length == 32
                    && chromaticities
                        .take()
                        .is_some_and(|c| Some(c) == info.chromaticities())
            }
            b"sRGB" => length == 1 && srgb.take().is_some(),
            b"iCCP" | b"cICP" | b"mDCV" | b"cLLI" | b"sBIT" | b"bKGD" | b"eXIf" => false,
            _ => true,
        };
        if !supported {
            return Err(StorageError::Invalid("unsupported PNG color declaration"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba_with(chunks: &[(png::chunk::ChunkType, &[u8])]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(ColorType::Rgba);
        let mut writer = encoder.write_header().unwrap();
        for &(kind, data) in chunks {
            writer.write_chunk(kind, data).unwrap();
        }
        writer.write_image_data(&[42, 43, 44, 45]).unwrap();
        writer.finish().unwrap();
        bytes
    }

    #[test]
    fn honors_srgb_intent_and_explicit_fallbacks() {
        use png::chunk::{cHRM, gAMA, sRGB};
        let primaries: Vec<_> = [31270u32, 32900, 64000, 33000, 30000, 60000, 15000, 6000]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect();
        let gamma = 45455u32.to_be_bytes();
        for chunks in [
            vec![(sRGB, &[2u8][..])],
            vec![(cHRM, &primaries)],
            vec![(cHRM, &primaries), (gAMA, &gamma), (sRGB, &[2])],
        ] {
            let source = rgba_with(&chunks);
            let output = thumbnail(&source).unwrap();
            let read = |bytes| Decoder::new(Cursor::new(bytes)).read_info().unwrap();
            let src = read(source);
            let dst = read(output);
            assert_eq!(src.info().srgb, dst.info().srgb);
            assert_eq!(src.info().chrm_chunk, dst.info().chrm_chunk);
            assert_eq!(src.info().gama_chunk, dst.info().gama_chunk);
        }
    }

    #[test]
    fn refuses_lossy_color_fallback() {
        use png::chunk::{cICP, gAMA, iCCP, sRGB};
        let gamma = 100_000u32.to_be_bytes();
        for (kind, data) in [
            (cICP, &[9, 16, 0, 1][..]),
            (iCCP, b"ignored profile"),
            (sRGB, &[0]),
            (gAMA, &gamma),
        ] {
            let source = rgba_with(&[(gAMA, &gamma), (kind, data)]);
            assert!(matches!(
                thumbnail(&source),
                Err(StorageError::Invalid("unsupported PNG color declaration"))
            ));
        }
    }
}
