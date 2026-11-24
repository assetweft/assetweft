use super::determinism_domain::canonical_f64;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

#[derive(Deserialize)]
pub(super) struct Manifest {
    sheet: Sheet,
    frames: Vec<Frame>,
    looping: Looping,
}

#[derive(Deserialize)]
struct Sheet {
    frame_width: u32,
    frame_height: u32,
    columns: u32,
    rows: u32,
}

#[derive(Deserialize)]
struct Frame {
    index: u32,
    col: u32,
    row: u32,
    anchor_x: f64,
    anchor_y: f64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Looping {
    pub loop_cycle: bool,
    pub start_frame: u32,
    pub end_frame: u32,
}

#[derive(Debug, Serialize)]
pub struct SpriteFrame {
    pub index: u32,
    pub col: u32,
    pub row: u32,
    pub anchor: [f64; 2],
    pub has_alpha_channel: bool,
    pub transparent_pixels: u32,
    pub opaque_pixels: u32,
    pub empty_frame: bool,
    pub frame_sha256: String,
}

#[derive(Debug, Serialize)]
pub struct SpriteReport {
    pub scope: &'static str,
    pub width: u32,
    pub height: u32,
    pub frame_count: usize,
    pub has_transparency: bool,
    pub empty_frames: Vec<u32>,
    pub duplicate_frame_groups: Vec<Vec<u32>>,
    pub looping: Looping,
    pub semantic_quality_status: &'static str,
    pub frames: Vec<SpriteFrame>,
}

impl Manifest {
    pub(super) fn inspect(
        self,
        width: u32,
        height: u32,
        has_alpha_channel: bool,
        rgba: &[u8],
    ) -> Result<SpriteReport, &'static str> {
        let sheet = self.sheet;
        if sheet.frame_width.checked_mul(sheet.columns) != Some(width)
            || sheet.frame_height.checked_mul(sheet.rows) != Some(height)
        {
            return Err("sheet grid dimensions must exactly match the PNG");
        }
        if self.looping.start_frame > self.looping.end_frame
            || self.looping.end_frame as usize >= self.frames.len()
        {
            return Err("loop bounds must select an inclusive frame range");
        }

        let mut slots = HashSet::new();
        let mut frames = Vec::new();
        for (position, frame) in self.frames.into_iter().enumerate() {
            if frame.index as usize != position {
                return Err("frame indices must be contiguous and zero-based");
            }
            if frame.col >= sheet.columns || frame.row >= sheet.rows {
                return Err("frame slot outside sheet grid");
            }
            if !slots.insert((frame.col, frame.row)) {
                return Err("frame slot appears more than once");
            }
            let anchor = [anchor(frame.anchor_x)?, anchor(frame.anchor_y)?];

            let pixel_x = frame.col * sheet.frame_width;
            let pixel_y = frame.row * sheet.frame_height;
            let mut digest = Sha256::new();
            let mut transparent_pixels = 0;
            let mut opaque_pixels = 0;
            let mut empty_frame = true;
            for y in pixel_y..pixel_y + sheet.frame_height {
                let start = ((y * width + pixel_x) * 4) as usize;
                let end = start + (sheet.frame_width * 4) as usize;
                let row = &rgba[start..end];
                digest.update(row);
                for pixel in row.chunks_exact(4) {
                    transparent_pixels += u32::from(pixel[3] < 255);
                    opaque_pixels += u32::from(pixel[3] == 255);
                    empty_frame &= pixel[3] == 0;
                }
            }
            frames.push(SpriteFrame {
                index: frame.index,
                col: frame.col,
                row: frame.row,
                anchor,
                has_alpha_channel,
                transparent_pixels,
                opaque_pixels,
                empty_frame,
                frame_sha256: digest
                    .finalize()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect(),
            });
        }
        let mut groups = BTreeMap::<&str, Vec<u32>>::new();
        for frame in &frames {
            groups
                .entry(&frame.frame_sha256)
                .or_default()
                .push(frame.index);
        }
        let mut duplicate_frame_groups: Vec<_> = groups
            .into_values()
            .filter(|group| group.len() > 1)
            .collect();
        duplicate_frame_groups.sort_unstable();
        Ok(SpriteReport {
            scope: "inspected_sprite_sheet",
            width,
            height,
            frame_count: frames.len(),
            has_transparency: rgba.chunks_exact(4).any(|pixel| pixel[3] < 255),
            empty_frames: frames
                .iter()
                .filter(|frame| frame.empty_frame)
                .map(|frame| frame.index)
                .collect(),
            duplicate_frame_groups,
            looping: self.looping,
            semantic_quality_status: "NOT_RUN",
            frames,
        })
    }
}

fn anchor(value: f64) -> Result<f64, &'static str> {
    let value = canonical_f64(value).map_err(|_| "frame anchor must be finite")?;
    if !(0.0..=1.0).contains(&value) {
        return Err("frame anchor must be normalized to [0, 1]");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::anchor;

    #[test]
    fn anchor_checks_finiteness_and_canonicalizes_zero() {
        assert_eq!(anchor(-0.0).unwrap().to_bits(), 0);
        assert_eq!(anchor(1.0), Ok(1.0));
        for value in [f64::NAN, f64::INFINITY, -f64::INFINITY, -0.1, 1.2] {
            assert!(anchor(value).is_err());
        }
    }
}
