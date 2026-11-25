use super::determinism_domain::{DeterminismError, canonical_f64, quantize_1e6_units};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VfxMode {
    Parametric,
    Flipbook,
    Hybrid,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthoringState {
    NativeEditable,
    Baked,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlendMode {
    Alpha,
    Additive,
    Premultiplied,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VfxTiming {
    pub fps: f64,
    pub frame_count: u32,
    pub duration_ms: u64,
    pub loop_cycle: bool,
    pub blend_mode: BlendMode,
}

impl VfxTiming {
    fn duration_matches(&self) -> bool {
        let expected = f64::from(self.frame_count) * 1000.0 / self.fps;
        let whole_ms = (expected as i128).saturating_sub(i128::from(self.duration_ms));
        expected.is_finite()
            && (-2..=1).contains(&whole_ms)
            && quantize_1e6_units(whole_ms as f64 + expected.fract())
                .is_ok_and(|delta| delta.abs() <= 1_000_000)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VfxParameter {
    pub name: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlipbookGrid {
    pub frame_width: u32,
    pub frame_height: u32,
    pub columns: u32,
    pub rows: u32,
    pub require_transparency: bool,
}

#[derive(Debug, Serialize)]
pub struct FlipbookFacts {
    pub width: u32,
    pub height: u32,
    pub has_alpha_channel: bool,
    pub transparent_pixel_count: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Spec {
    vfx_mode: VfxMode,
    authoring_state: AuthoringState,
    #[serde(default)]
    requires_native_editable: bool,
    timing: VfxTiming,
    parameters: Vec<VfxParameter>,
    flipbook: Option<FlipbookGrid>,
}

#[derive(Debug, Serialize)]
pub struct VfxViolation {
    pub field: &'static str,
    pub detail: &'static str,
}

#[derive(Debug, Serialize)]
pub struct VfxReport {
    pub vfx_mode: VfxMode,
    pub authoring_state: AuthoringState,
    pub requires_native_editable: bool,
    pub timing: VfxTiming,
    pub parameters: Vec<VfxParameter>,
    pub flipbook: Option<FlipbookGrid>,
    pub flipbook_facts: Option<FlipbookFacts>,
    pub vfx_spec_sha256: String,
    pub flipbook_sha256: Option<String>,
    pub visual_engine_status: &'static str,
    pub violations: Vec<VfxViolation>,
}

impl Spec {
    pub(super) fn inspect(
        mut self,
        vfx_spec_sha256: String,
        atlas: Option<(FlipbookFacts, String)>,
    ) -> Result<VfxReport, DeterminismError> {
        self.timing.fps = canonical_f64(self.timing.fps)?;
        let mut violations = Vec::new();
        let mut fail = |field, detail| violations.push(VfxViolation { field, detail });

        if self.requires_native_editable && self.authoring_state == AuthoringState::Baked {
            fail("authoring_state", "native_editable required");
        }
        if self.timing.fps <= 0.0 || self.timing.frame_count == 0 {
            fail("timing", "fps and frame_count must be positive");
        } else if !self.timing.duration_matches() {
            fail(
                "timing.duration_ms",
                "duration differs from frame_count / fps by more than 1 ms",
            );
        }

        if self.parameters.is_empty() {
            fail("parameters", "at least one parameter is required");
        }
        let mut names = HashSet::new();
        for parameter in &mut self.parameters {
            parameter.value = canonical_f64(parameter.value)?;
            parameter.min = canonical_f64(parameter.min)?;
            parameter.max = canonical_f64(parameter.max)?;
            let name = parameter.name.trim();
            if name.is_empty() || !names.insert(name) {
                fail(
                    "parameters.name",
                    "parameter names must be non-blank and unique",
                );
            }
            if !(parameter.min <= parameter.value && parameter.value <= parameter.max) {
                fail(
                    "parameters.value",
                    "parameter value must be within [min, max]",
                );
            }
        }

        match (&self.vfx_mode, &self.flipbook, &atlas) {
            (VfxMode::Parametric, None, None) => {}
            (VfxMode::Parametric, _, _) => {
                fail(
                    "flipbook",
                    "parametric VFX cannot declare or supply a flipbook",
                );
            }
            (_, Some(grid), Some((facts, _))) => {
                if grid.frame_width.checked_mul(grid.columns) != Some(facts.width)
                    || grid.frame_height.checked_mul(grid.rows) != Some(facts.height)
                {
                    fail(
                        "flipbook",
                        "flipbook grid dimensions must exactly match the PNG",
                    );
                }
                if u64::from(grid.columns) * u64::from(grid.rows)
                    < u64::from(self.timing.frame_count)
                {
                    fail(
                        "timing.frame_count",
                        "frame count exceeds flipbook grid capacity",
                    );
                }
                if grid.require_transparency
                    && (!facts.has_alpha_channel || facts.transparent_pixel_count == 0)
                {
                    fail(
                        "flipbook.require_transparency",
                        "flipbook needs alpha and a non-opaque pixel",
                    );
                }
            }
            _ => fail("flipbook", "flipbook and hybrid VFX require a grid and PNG"),
        }

        let (flipbook_facts, flipbook_sha256) = atlas.unzip();
        Ok(VfxReport {
            vfx_mode: self.vfx_mode,
            authoring_state: self.authoring_state,
            requires_native_editable: self.requires_native_editable,
            timing: self.timing,
            parameters: self.parameters,
            flipbook: self.flipbook,
            flipbook_facts,
            vfx_spec_sha256,
            flipbook_sha256,
            visual_engine_status: "NOT_RUN",
            violations,
        })
    }
}
