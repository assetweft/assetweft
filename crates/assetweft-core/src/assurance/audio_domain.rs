use super::{canonical_f64, quantize_1e6_units};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AudioSpec {
    expected_channels: Option<u16>,
    expected_sample_rate_hz: Option<u32>,
    min_duration_ms: Option<u64>,
    max_duration_ms: Option<u64>,
    transcript_text: Option<String>,
    loop_region: Option<LoopRegion>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoopRegion {
    start_sample: u64,
    end_sample: u64,
    max_step_delta: f64,
    max_zero_crossing_offset: f64,
}

#[derive(Debug, Serialize)]
pub struct AudioLoopReport {
    pub start_sample: u64,
    pub end_sample: u64,
    pub step_delta: f64,
    pub zero_crossing_offset: f64,
}

#[derive(Debug, Serialize)]
pub struct AudioReport {
    pub sample_frames: u64,
    pub duration_ms: f64,
    pub channels: u16,
    pub sample_rate_hz: u32,
    pub bits_per_sample: u16,
    pub peak_amplitude: f64,
    pub rms_level: f64,
    pub clipped_samples: u64,
    pub silent: bool,
    pub pcm_sha256: String,
    pub transcript_text: Option<String>,
    pub chars_per_sec: Option<f64>,
    pub asr_playback_status: &'static str,
    pub loop_region: Option<AudioLoopReport>,
    pub violations: Vec<&'static str>,
}

impl AudioSpec {
    pub(super) fn validate(&self, frames: u64) -> Result<(), &'static str> {
        if self.expected_channels == Some(0) || self.expected_sample_rate_hz == Some(0) {
            return Err("expected channels and sample rate must be positive");
        }
        if matches!((self.min_duration_ms, self.max_duration_ms), (Some(min), Some(max)) if min > max)
        {
            return Err("min_duration_ms exceeds max_duration_ms");
        }
        if let Some(region) = &self.loop_region {
            if region.start_sample >= region.end_sample || region.end_sample > frames {
                return Err("loop region must be a nonempty range within the WAV frames");
            }
            for tolerance in [region.max_step_delta, region.max_zero_crossing_offset] {
                if canonical_f64(tolerance).is_err() || tolerance < 0.0 {
                    return Err("loop tolerances must be finite and non-negative");
                }
            }
        }
        Ok(())
    }
}

pub(super) struct AudioFormat {
    pub channels: u16,
    pub sample_rate_hz: u32,
    pub bits_per_sample: u16,
    pub decode: fn(&[u8]) -> f64,
}

impl AudioFormat {
    pub(super) fn inspect(
        &self,
        data: &[u8],
        sample_frames: u64,
        spec: Option<AudioSpec>,
    ) -> Result<AudioReport, &'static str> {
        let width = usize::from(self.bits_per_sample / 8);
        let mut peak_amplitude = 0.0_f64;
        let mut square_sum = 0.0;
        let mut clipped_samples = 0;
        let mut digest = Sha256::new();
        for bytes in data.chunks_exact(width) {
            let sample = (self.decode)(bytes);
            let units = quantize_1e6_units(sample)
                .map_err(|_| "sample is non-finite or exceeds the canonical range")?;
            digest.update(units.to_le_bytes());
            peak_amplitude = peak_amplitude.max(sample.abs());
            square_sum += sample * sample;
            clipped_samples += u64::from(sample.abs() >= 1.0);
        }
        let silent = peak_amplitude == 0.0;
        let millis_numerator = u128::from(sample_frames) * 1_000;
        let rate = u128::from(self.sample_rate_hz);
        let duration_us = (millis_numerator * 1_000 + rate / 2) / rate;
        let mut violations = Vec::new();
        let mut loop_region = None;
        let mut transcript_text = None;
        if let Some(spec) = spec {
            if spec
                .expected_channels
                .is_some_and(|value| value != self.channels)
            {
                violations.push("ChannelMismatch");
            }
            if spec
                .expected_sample_rate_hz
                .is_some_and(|value| value != self.sample_rate_hz)
            {
                violations.push("SampleRateMismatch");
            }
            if spec
                .min_duration_ms
                .is_some_and(|value| millis_numerator < u128::from(value) * rate)
                || spec
                    .max_duration_ms
                    .is_some_and(|value| millis_numerator > u128::from(value) * rate)
            {
                violations.push("DurationMismatch");
            }
            if clipped_samples > 0 {
                violations.push("Clipping");
            }
            if silent {
                violations.push("Silent");
            }
            if let Some(region) = spec.loop_region {
                let frame_bytes = width * usize::from(self.channels);
                let frame = |index: u64| {
                    let offset = index as usize * frame_bytes;
                    data[offset..offset + frame_bytes].chunks_exact(width)
                };
                let mut step_delta = 0.0_f64;
                let mut zero_crossing_offset = 0.0_f64;
                for (start, end) in frame(region.start_sample).zip(frame(region.end_sample - 1)) {
                    let start = (self.decode)(start);
                    let end = (self.decode)(end);
                    step_delta = step_delta.max((end - start).abs());
                    zero_crossing_offset = zero_crossing_offset.max(start.abs()).max(end.abs());
                }
                if step_delta > region.max_step_delta
                    || zero_crossing_offset > region.max_zero_crossing_offset
                {
                    violations.push("LoopBoundaryClick");
                }
                loop_region = Some(AudioLoopReport {
                    start_sample: region.start_sample,
                    end_sample: region.end_sample,
                    step_delta,
                    zero_crossing_offset,
                });
            }
            transcript_text = spec.transcript_text;
        }
        let chars_per_sec = transcript_text.as_ref().map(|text| {
            text.chars().count() as f64 * f64::from(self.sample_rate_hz) / sample_frames as f64
        });
        Ok(AudioReport {
            sample_frames,
            duration_ms: duration_us as f64 / 1_000.0,
            channels: self.channels,
            sample_rate_hz: self.sample_rate_hz,
            bits_per_sample: self.bits_per_sample,
            peak_amplitude,
            rms_level: (square_sum / (data.len() / width) as f64).sqrt(),
            clipped_samples,
            silent,
            pcm_sha256: digest
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            transcript_text,
            chars_per_sec,
            asr_playback_status: "NOT_RUN",
            loop_region,
            violations,
        })
    }
}
