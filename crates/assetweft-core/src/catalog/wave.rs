//! Catalog audition clips for canonical little-endian PCM16 WAVE.
//! Called under the managed-file lease with verified, size-limited bytes. Preserve whole frames.
use crate::storage::StorageError;

pub(super) fn audition(source: &[u8]) -> Result<Vec<u8>, StorageError> {
    let malformed = StorageError::Invalid("malformed PCM WAVE");
    let Some((header, samples)) = source.split_first_chunk::<44>() else {
        return Err(malformed);
    };
    let word = |i| u16::from_le_bytes([header[i], header[i + 1]]);
    let dword = |i| u32::from_le_bytes([header[i], header[i + 1], header[i + 2], header[i + 3]]);
    if &header[..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return Err(StorageError::Invalid("RIFF/WAVE required"));
    }
    if dword(4) as usize != source.len() - 8 {
        return Err(malformed);
    }
    if &header[12..16] != b"fmt " || dword(16) != 16 || &header[36..40] != b"data" {
        return Err(StorageError::Invalid(
            "canonical fmt and data chunks required",
        ));
    }
    let channels = word(22);
    let rate = dword(24);
    if word(20) != 1
        || word(34) != 16
        || !(1..=2).contains(&channels)
        || !(8000..=48000).contains(&rate)
    {
        return Err(StorageError::Invalid(
            "mono or stereo PCM16 at 8000–48000 Hz required",
        ));
    }
    let align = channels * 2;
    if word(32) != align
        || dword(28) != rate * u32::from(align)
        || dword(40) as usize != samples.len()
        || !samples.len().is_multiple_of(usize::from(align))
    {
        return Err(malformed);
    }

    let frames = samples.len() / usize::from(align);
    let clip_frames = (frames / 2).min(rate as usize);
    if clip_frames == 0 {
        return Err(StorageError::Invalid("at least two WAVE frames required"));
    }
    let clip_bytes = clip_frames * usize::from(align);
    let mut clip = source[..header.len() + clip_bytes].to_vec();
    clip[4..8].copy_from_slice(&(36 + clip_bytes as u32).to_le_bytes());
    clip[40..44].copy_from_slice(&(clip_bytes as u32).to_le_bytes());
    Ok(clip)
}
