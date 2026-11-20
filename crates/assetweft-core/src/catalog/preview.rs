//! Catalog PNG thumbnail codec. The managed-file adapter supplies verified, bounded bytes.
//! Decode the whole static image before producing a deterministic nearest-neighbour preview.
use crate::storage::StorageError;
use png::{BitDepth, ColorType, DecodeOptions, Decoder, Encoder, Limits};
use std::io::Cursor;

pub(super) fn thumbnail(source: &[u8]) -> Result<Vec<u8>, StorageError> {
    let invalid = |_| StorageError::Invalid("malformed PNG");
    let mut options = DecodeOptions::default();
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
    let encode = |_| StorageError::Invalid("PNG encoding failed");
    let mut writer = encoder.write_header().map_err(encode)?;
    writer.write_image_data(&sampled).map_err(encode)?;
    writer.finish().map_err(encode)?;
    Ok(output)
}
