use super::{AudioReport, semantic_sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConnectorSpec {
    pub voice: VoiceTrack,
    pub sfx: Vec<SfxTrack>,
    pub bgm: Option<BgmTrack>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VoiceTrack {
    #[serde(flatten)]
    pub track: Track,
    pub text_version: String,
    pub script_text: String,
    pub voice_id: String,
    pub external_runtime_status: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SfxTrack {
    #[serde(flatten)]
    pub track: Track,
    pub category: String,
    pub loop_region: Option<LoopRegion>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BgmTrack {
    #[serde(flatten)]
    pub track: Track,
    pub loop_region: Option<LoopRegion>,
}

#[derive(Deserialize, Serialize)]
pub(super) struct Track {
    pub track_id: String,
    pub model_id: String,
    pub wav_path: String,
    pub expected_channels: u16,
    pub expected_sample_rate_hz: u32,
    pub min_duration_ms: u64,
    pub max_duration_ms: u64,
    pub licensing: TrackLicensing,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LoopRegion {
    pub start_sample: u64,
    pub end_sample: u64,
    #[serde(deserialize_with = "loop_tolerance")]
    pub max_step_delta: f64,
    #[serde(deserialize_with = "loop_tolerance")]
    pub max_zero_crossing_offset: f64,
}

fn loop_tolerance<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    let value = f64::deserialize(deserializer)?;
    super::quantize_1e6_units(value).map_err(serde::de::Error::custom)?;
    super::canonical_f64(value).map_err(serde::de::Error::custom)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrackLicensing {
    pub code_license: Option<String>,
    pub weights_license: Option<String>,
    pub dataset_license: Option<String>,
    pub service_tos_ref: Option<String>,
    pub linkage: Linkage,
    pub dependency_name: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Linkage {
    InProcess,
    ExternalSubprocess,
    RemoteApi,
}

#[derive(Debug, Serialize)]
pub struct AudioProvenance {
    pub track_id: String,
    pub role: &'static str,
    pub model_id: String,
    pub pcm_sha256: Option<String>,
    pub licensing: TrackLicensing,
    pub violations: Vec<&'static str>,
    pub audio: Option<AudioReport>,
}

#[derive(Debug, Serialize)]
pub struct AudioConnectorReport {
    pub connector_sha256: String,
    pub bgm_status: &'static str,
    pub bgm_generated: bool,
    pub asr_playback_status: &'static str,
    pub external_runtime_status: String,
    pub text_version: String,
    pub voice_id: String,
    pub provenance_entries: Vec<AudioProvenance>,
    pub violations: Vec<&'static str>,
}

impl TrackLicensing {
    fn violations(&self, model_id: &str) -> Vec<&'static str> {
        let records = [
            self.code_license.as_deref(),
            self.weights_license.as_deref(),
            self.dataset_license.as_deref(),
            self.service_tos_ref.as_deref(),
            Some(self.dependency_name.as_str()),
        ];
        let mut violations = Vec::new();
        if records.iter().any(|record| record.is_none_or(blank)) {
            violations.push("IncompleteLicenseProvenance");
        }
        if records
            .into_iter()
            .flatten()
            .chain([model_id])
            .any(|record| {
                let name: String = record
                    .to_ascii_lowercase()
                    .chars()
                    .filter(char::is_ascii_alphanumeric)
                    .collect();
                name.contains("fbxsdk") || name.contains("autodeskfbx")
            })
        {
            violations.push("ForbiddenProprietaryDependency");
        }
        let copyleft = self.code_license.as_deref().is_some_and(|license| {
            license
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
                .any(|part| {
                    let part = part.to_ascii_uppercase();
                    part.starts_with("GPL-") || part.starts_with("AGPL-")
                })
        }) || self
            .dependency_name
            .to_ascii_lowercase()
            .contains("piper1-gpl");
        if copyleft && self.linkage != Linkage::ExternalSubprocess {
            violations.push(if self.linkage == Linkage::InProcess {
                "CopyleftInProcessViolation"
            } else {
                "CopyleftRequiresExternalSubprocess"
            });
        }
        violations
    }
}

fn blank(text: &str) -> bool {
    text.trim().is_empty()
}

impl Track {
    pub fn provenance(
        &self,
        role: &'static str,
        audio: Result<AudioReport, &'static str>,
    ) -> AudioProvenance {
        let mut violations = self.licensing.violations(&self.model_id);
        if blank(&self.track_id) || blank(&self.model_id) {
            violations.push("IncompleteTrackIdentity");
        }
        let audio = match audio {
            Ok(audio) => {
                violations.extend(&audio.violations);
                Some(audio)
            }
            Err(violation) => {
                violations.push(violation);
                None
            }
        };
        AudioProvenance {
            track_id: self.track_id.clone(),
            role,
            model_id: self.model_id.clone(),
            pcm_sha256: audio.as_ref().map(|audio| audio.pcm_sha256.clone()),
            licensing: self.licensing.clone(),
            violations,
            audio,
        }
    }
}

impl ConnectorSpec {
    pub fn report(self, mut entries: Vec<AudioProvenance>) -> AudioConnectorReport {
        let voice = &mut entries[0];
        if [
            &self.voice.text_version,
            &self.voice.script_text,
            &self.voice.voice_id,
            &self.voice.external_runtime_status,
        ]
        .into_iter()
        .any(|value| blank(value))
        {
            voice.violations.push("IncompleteVoiceProvenance");
        }
        for (sfx, entry) in self.sfx.iter().zip(&mut entries[1..]) {
            if blank(&sfx.category) {
                entry.violations.push("IncompleteSfxCategory");
            }
        }
        for entry in &mut entries[1..] {
            if entry.model_id.trim() == self.voice.track.model_id.trim() {
                entry.violations.push("SharedModelAcrossAudioRoles");
            }
        }
        let bgm_status = if self.bgm.is_none() {
            "NOT_RUN"
        } else if entries
            .last()
            .is_some_and(|entry| entry.violations.is_empty())
        {
            "INSPECTED"
        } else {
            "FAIL"
        };
        let mut violations = Vec::new();
        if self.sfx.is_empty() {
            violations.push("MissingSfxTracks");
        }
        for entry in &entries {
            for &violation in &entry.violations {
                if !violations.contains(&violation) {
                    violations.push(violation);
                }
            }
        }

        let digests: Vec<_> = entries.iter().map(|entry| &entry.pcm_sha256).collect();
        let normalized = json!({
            "spec": self,
            "pcm_sha256": digests,
            "bgm_status": bgm_status,
        });
        // Preserve textual provenance byte-for-byte and bound the number of G-DET-1 units.
        let digest = Sha256::digest(normalized.to_string().as_bytes());
        let units: Vec<_> = digest.iter().copied().map(f64::from).collect();
        AudioConnectorReport {
            connector_sha256: semantic_sha256_hex(&units)
                .expect("byte values fit the canonical range"),
            bgm_status,
            bgm_generated: self.bgm.is_some(),
            asr_playback_status: "NOT_RUN",
            external_runtime_status: self.voice.external_runtime_status,
            text_version: self.voice.text_version,
            voice_id: self.voice.voice_id,
            provenance_entries: entries,
            violations,
        }
    }
}
