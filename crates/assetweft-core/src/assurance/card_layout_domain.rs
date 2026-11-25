use super::determinism_domain::canonical_semantic_bytes;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

const MAX_CANVAS_EDGE: u32 = 4096;
const MAX_TEXT_BOXES: usize = 1024;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateKind {
    Ui,
    Card,
    Tile,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Spec {
    kind: TemplateKind,
    canvas: Canvas,
    illustration_slot: Slot,
    text_boxes: Vec<TextBox>,
    #[serde(default = "allow_overlay")]
    allow_text_over_illustration: bool,
}

fn allow_overlay() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Canvas {
    width_px: u32,
    height_px: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Slot {
    slot_id: String,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    editable_independently: bool,
    #[serde(rename = "art_asset_sha256")]
    _art_asset_sha256: Option<String>,
    fit_mode: FitMode,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum FitMode {
    Contain,
    Cover,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TextBox {
    box_id: String,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    text: String,
    font_px: u32,
    line_height_px: u32,
    max_chars_per_line: u32,
    max_lines: u32,
}

struct Rect {
    x: u32,
    y: u32,
    right: u32,
    bottom: u32,
}

impl Rect {
    fn in_canvas(x: u32, y: u32, w: u32, h: u32, canvas: &Canvas) -> Option<Self> {
        let right = x.checked_add(w)?;
        let bottom = y.checked_add(h)?;
        (w > 0 && h > 0 && right <= canvas.width_px && bottom <= canvas.height_px).then_some(Self {
            x,
            y,
            right,
            bottom,
        })
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.x < other.right
            && other.x < self.right
            && self.y < other.bottom
            && other.y < self.bottom
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum LayoutViolation {
    CanvasDimensions,
    IllustrationSlot,
    SlotNotIndependent,
    TextBoxCount,
    TextBoxId,
    TextBoxBounds,
    TextMetrics,
    TextOverflow,
    Overlap,
}

#[derive(Debug, Serialize)]
pub struct CardLayoutReport {
    pub scope: &'static str,
    pub kind: TemplateKind,
    pub template_layout_sha256: String,
    pub illustration_sha256: Option<String>,
    pub illustration_width: Option<u32>,
    pub illustration_height: Option<u32>,
    pub illustration_fitted_box: Option<FittedBox>,
    pub visual_quality_status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct FittedBox {
    pub x_numerator: i64,
    pub y_numerator: i64,
    pub width_numerator: i64,
    pub height_numerator: i64,
    pub denominator: i64,
}

impl Slot {
    fn fitted_box(&self, width: u32, height: u32) -> FittedBox {
        let [iw, ih, sw, sh] = [width, height, self.width, self.height].map(i64::from);
        let width_limited = sw * ih <= sh * iw;
        let (scale_num, scale_den) = if width_limited == matches!(self.fit_mode, FitMode::Contain) {
            (sw, iw)
        } else {
            (sh, ih)
        };
        FittedBox {
            x_numerator: (2 * i64::from(self.x) + sw) * scale_den - iw * scale_num,
            y_numerator: (2 * i64::from(self.y) + sh) * scale_den - ih * scale_num,
            width_numerator: 2 * iw * scale_num,
            height_numerator: 2 * ih * scale_num,
            denominator: 2 * scale_den,
        }
    }
}

impl TextBox {
    fn text_fits(&self) -> bool {
        let limit = self.max_chars_per_line as usize;
        let mut lines = 0;
        for paragraph in self.text.split('\n') {
            lines += 1;
            let mut width = 0;
            for word in paragraph.split_whitespace() {
                let length = word.chars().count();
                if length > limit {
                    return false;
                }
                if width == 0 {
                    width = length;
                } else if length < limit - width {
                    width += 1 + length;
                } else {
                    lines += 1;
                    width = length;
                }
            }
            if lines > self.max_lines {
                return false;
            }
        }
        true
    }
}

impl Spec {
    pub(super) fn check(
        mut self,
        illustration: Option<(u32, u32, String)>,
    ) -> Result<CardLayoutReport, LayoutViolation> {
        use LayoutViolation::*;
        if !(1..=MAX_CANVAS_EDGE).contains(&self.canvas.width_px)
            || !(1..=MAX_CANVAS_EDGE).contains(&self.canvas.height_px)
        {
            return Err(CanvasDimensions);
        }
        let slot = &self.illustration_slot;
        let slot_rect = Rect::in_canvas(slot.x, slot.y, slot.width, slot.height, &self.canvas)
            .ok_or(IllustrationSlot)?;
        if slot.slot_id.trim().is_empty() {
            return Err(IllustrationSlot);
        }
        if !slot.editable_independently {
            return Err(SlotNotIndependent);
        }
        if !(1..=MAX_TEXT_BOXES).contains(&self.text_boxes.len()) {
            return Err(TextBoxCount);
        }
        let mut ids = HashSet::new();
        let mut rects = Vec::new();
        for b in &self.text_boxes {
            if b.box_id.trim().is_empty() || !ids.insert(b.box_id.as_str()) {
                return Err(TextBoxId);
            }
            let rect =
                Rect::in_canvas(b.x, b.y, b.width, b.height, &self.canvas).ok_or(TextBoxBounds)?;
            if b.font_px == 0
                || b.line_height_px < b.font_px
                || b.max_chars_per_line == 0
                || b.max_lines == 0
            {
                return Err(TextMetrics);
            }
            if b.max_lines
                .checked_mul(b.line_height_px)
                .is_none_or(|h| h > b.height)
                || !b.text_fits()
            {
                return Err(TextOverflow);
            }
            if (!self.allow_text_over_illustration && rect.overlaps(&slot_rect))
                || rects.iter().any(|other| rect.overlaps(other))
            {
                return Err(Overlap);
            }
            rects.push(rect);
        }

        self.text_boxes
            .sort_unstable_by(|a, b| a.box_id.cmp(&b.box_id));
        let geometry = [
            self.canvas.width_px,
            self.canvas.height_px,
            slot.x,
            slot.y,
            slot.width,
            slot.height,
        ]
        .map(f64::from);
        let mut canonical = canonical_semantic_bytes(&geometry)
            .expect("validated canvas and slot coordinates are integers in 0..=4096");
        let fields = (
            "card_layout_v1",
            &slot.slot_id,
            slot.fit_mode,
            slot.editable_independently,
            &self.text_boxes,
        );
        serde_json::to_writer(&mut canonical, &fields)
            .expect("layout contains only serializable strings, integers, and booleans");
        let (width, height, digest, fitted_box) = match illustration {
            Some((w, h, digest)) => (Some(w), Some(h), Some(digest), Some(slot.fitted_box(w, h))),
            None => (None, None, None, None),
        };
        Ok(CardLayoutReport {
            scope: "card_layout_v1",
            kind: self.kind,
            template_layout_sha256: sha256_hex(&canonical),
            illustration_sha256: digest,
            illustration_width: width,
            illustration_height: height,
            illustration_fitted_box: fitted_box,
            visual_quality_status: "NOT_RUN",
        })
    }
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
