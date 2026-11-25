use super::determinism_domain::semantic_sha256_hex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Spec {
    clip_duration_ms: u64,
    phases: Phases,
    bindings: Vec<Binding>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Phases {
    charge_ms: [u64; 2],
    release_ms: [u64; 2],
    recovery_ms: [u64; 2],
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    id: String,
    kind: Kind,
    phase: Phase,
    trigger_ms: u64,
    duration_ms: u64,
    cancel_cleanup_policy: CancelCleanupPolicy,
    event_source: EventSource,
    claims_gameplay_hit: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Vfx,
    Sfx,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Charge,
    Release,
    Recovery,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum CancelCleanupPolicy {
    StopAndClear,
    FadeOut,
    PersistLeak,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum EventSource {
    FixedTimeline,
    AuthoritativeHit,
}

#[derive(Debug, Serialize)]
pub struct ActionPresentationReport {
    pub presentation_sha256: String,
    pub engine_playback_status: &'static str,
    pub violations: Vec<ActionPresentationViolation>,
}

#[derive(Debug, Serialize)]
pub struct ActionPresentationViolation {
    pub field: String,
    pub code: &'static str,
    pub detail: &'static str,
}

impl Spec {
    pub(super) fn inspect(&self) -> Result<ActionPresentationReport, String> {
        let mut violations = Vec::new();
        let mut reject = |field, code, detail| {
            violations.push(ActionPresentationViolation {
                field,
                code,
                detail,
            });
        };
        let [charge_start, charge_end] = self.phases.charge_ms;
        let [release_start, release_end] = self.phases.release_ms;
        let [recovery_start, recovery_end] = self.phases.recovery_ms;
        if !(charge_start < charge_end
            && charge_end == release_start
            && release_start < release_end
            && release_end == recovery_start
            && recovery_start < recovery_end
            && recovery_end <= self.clip_duration_ms)
        {
            reject(
                "phases".into(),
                "InvalidPhaseBounds",
                "phases must be nonempty, ordered, contiguous and inside the clip",
            );
        }
        if self.bindings.is_empty() {
            reject(
                "bindings".into(),
                "EmptyBindings",
                "at least one binding is required",
            );
        }
        for (index, binding) in self.bindings.iter().enumerate() {
            let [start, end] = match binding.phase {
                Phase::Charge => self.phases.charge_ms,
                Phase::Release => self.phases.release_ms,
                Phase::Recovery => self.phases.recovery_ms,
            };
            if !(start..end).contains(&binding.trigger_ms) {
                reject(
                    format!("bindings[{index}].trigger_ms"),
                    "TriggerOutsidePhase",
                    "trigger must be within the declared phase",
                );
            }
            if matches!(
                binding.cancel_cleanup_policy,
                CancelCleanupPolicy::PersistLeak
            ) {
                reject(
                    format!("bindings[{index}].cancel_cleanup_policy"),
                    "UncleanCancelCleanup",
                    "cancel must stop and clear or fade out the cue",
                );
            }
            if matches!(binding.event_source, EventSource::FixedTimeline)
                && binding.claims_gameplay_hit
            {
                reject(
                    format!("bindings[{index}].claims_gameplay_hit"),
                    "HitEventConflation",
                    "a fixed timeline cue cannot claim a gameplay hit",
                );
            }
        }
        let canonical = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        let digest = Sha256::digest(canonical).map(f64::from);
        Ok(ActionPresentationReport {
            presentation_sha256: semantic_sha256_hex(&digest).map_err(|e| e.to_string())?,
            engine_playback_status: "NOT_RUN",
            violations,
        })
    }
}
