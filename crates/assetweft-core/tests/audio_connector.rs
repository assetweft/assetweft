use assetweft_core::assurance::{
    AudioConnectorReport, ValidationLimits, checks::inspect_audio_connector_file,
};
use serde_json::{Value, json};
use std::fs;

fn licensing() -> Value {
    json!({
        "code_license": "MIT", "weights_license": "CC-BY-4.0",
        "dataset_license": "CC0-1.0", "service_tos_ref": "local-offline",
        "linkage": "in_process", "dependency_name": "fixture-sfx"
    })
}

fn manifest() -> Value {
    json!({
        "voice": {
            "track_id": "voice-1", "text_version": "v1", "script_text": "测试!",
            "voice_id": "voice-v2", "model_id": "piper:model-v1", "wav_path": "track.wav",
            "expected_channels": 1, "expected_sample_rate_hz": 4,
            "min_duration_ms": 1000, "max_duration_ms": 1000, "external_runtime_status": "NOT_RUN",
            "licensing": {
                "code_license": "GPL-3.0", "weights_license": "CC-BY-4.0",
                "dataset_license": "CC0-1.0", "service_tos_ref": "local-offline",
                "linkage": "external_subprocess", "dependency_name": "piper1-gpl"
            }
        },
        "sfx": [{
            "track_id": "sfx-1", "category": "impact", "model_id": "sfx-v1", "wav_path": "track.wav",
            "expected_channels": 1, "expected_sample_rate_hz": 4,
            "min_duration_ms": 1000, "max_duration_ms": 1000,
            "loop_region": {"start_sample":0, "end_sample":4, "max_step_delta":0, "max_zero_crossing_offset":0},
            "licensing": licensing()
        }]
    })
}

fn inspect(dir: &tempfile::TempDir, spec: &Value) -> AudioConnectorReport {
    let path = dir.path().join("connector.json");
    fs::write(&path, spec.to_string()).unwrap();
    inspect_audio_connector_file(&path, ValidationLimits::default()).unwrap()
}

const WAV: &[u8] = b"RIFF\x2c\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x04\0\0\0\x08\0\0\0\x02\0\x10\0data\x08\0\0\0\0\0\0\x40\0\xc0\0\0";

#[test]
fn dedup_preserves_provenance_and_bgm_is_optional() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("track.wav"), WAV).unwrap();
    let mut spec = manifest();
    let report = inspect(&dir, &spec);
    assert!(report.violations.is_empty(), "{report:?}");
    assert_eq!(report.bgm_status, "NOT_RUN");
    assert!(!report.bgm_generated);
    assert_eq!(report.asr_playback_status, "NOT_RUN");
    assert_eq!(report.external_runtime_status, "NOT_RUN");
    assert_eq!(report.text_version, "v1");
    assert_eq!(report.voice_id, "voice-v2");
    assert_eq!(report.provenance_entries.len(), 2);
    let [voice, sfx] = &report.provenance_entries[..] else {
        unreachable!()
    };
    assert_eq!((voice.role, voice.track_id.as_str()), ("voice", "voice-1"));
    assert_eq!((sfx.role, sfx.track_id.as_str()), ("sfx", "sfx-1"));
    assert_eq!(voice.pcm_sha256, sfx.pcm_sha256);
    assert_eq!(voice.pcm_sha256.as_ref().unwrap().len(), 64);
    assert_ne!(voice.licensing.code_license, sfx.licensing.code_license);
    assert_eq!(voice.audio.as_ref().unwrap().chars_per_sec, Some(3.0));
    assert_eq!(
        report.connector_sha256,
        inspect(&dir, &spec).connector_sha256
    );

    spec["voice"]["text_version"] = json!("v2");
    assert_ne!(
        report.connector_sha256,
        inspect(&dir, &spec).connector_sha256
    );
    spec["bgm"] = spec["sfx"][0].clone();
    spec["bgm"].as_object_mut().unwrap().remove("category");
    spec["bgm"]["track_id"] = json!("bgm-1");
    spec["bgm"]["model_id"] = json!("bgm-v1");
    let report = inspect(&dir, &spec);
    assert!(report.violations.is_empty(), "{report:?}");
    assert_eq!(report.bgm_status, "INSPECTED");
    assert!(report.bgm_generated);
    assert_eq!(report.provenance_entries.len(), 3);
    assert_eq!(report.provenance_entries[2].role, "bgm");
    spec["bgm"]["model_id"] = spec["voice"]["model_id"].clone();
    let report = inspect(&dir, &spec);
    assert_eq!(report.bgm_status, "FAIL");
    assert!(report.violations.contains(&"SharedModelAcrossAudioRoles"));
}

#[test]
fn rejects_isolation_provenance_and_role_violations() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("track.wav"), WAV).unwrap();
    for (field, value, violation) in [
        (
            "/voice/licensing/linkage",
            json!("in_process"),
            "CopyleftInProcessViolation",
        ),
        (
            "/voice/licensing/linkage",
            json!("remote_api"),
            "CopyleftRequiresExternalSubprocess",
        ),
        (
            "/sfx/0/licensing/dependency_name",
            json!("Autodesk FBX SDK"),
            "ForbiddenProprietaryDependency",
        ),
        (
            "/sfx/0/licensing/code_license",
            json!("MIT OR AGPL-3.0-only"),
            "CopyleftInProcessViolation",
        ),
        (
            "/sfx/0/licensing/dataset_license",
            json!(" "),
            "IncompleteLicenseProvenance",
        ),
        (
            "/sfx/0/model_id",
            json!("piper:model-v1"),
            "SharedModelAcrossAudioRoles",
        ),
        ("/sfx", json!([]), "MissingSfxTracks"),
    ] {
        let mut spec = manifest();
        *spec.pointer_mut(field).unwrap() = value;
        assert_eq!(inspect(&dir, &spec).violations, [violation]);
    }
    let mut spec = manifest();
    spec["sfx"][0]["licensing"]
        .as_object_mut()
        .unwrap()
        .remove("weights_license");
    assert_eq!(
        inspect(&dir, &spec).violations,
        ["IncompleteLicenseProvenance"]
    );
}

#[test]
fn rejects_unplayable_wavs_and_loop_seams() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("track.wav");
    let mut spec = manifest();
    fs::write(&file, WAV).unwrap();
    spec["sfx"][0]["expected_channels"] = json!(2);
    spec["sfx"][0]["min_duration_ms"] = json!(999);
    spec["sfx"][0]["max_duration_ms"] = json!(999);
    spec["sfx"][0]["loop_region"]["end_sample"] = json!(3);
    assert_eq!(
        inspect(&dir, &spec).violations,
        ["ChannelMismatch", "DurationMismatch", "LoopBoundaryClick"]
    );

    let mut wav = WAV.to_vec();
    wav[44..].fill(0);
    fs::write(&file, &wav).unwrap();
    assert_eq!(inspect(&dir, &manifest()).violations, ["Silent"]);
    wav[47] = 0x80;
    fs::write(&file, &wav).unwrap();
    assert_eq!(inspect(&dir, &manifest()).violations, ["Clipping"]);
    fs::write(&file, b"RIFF").unwrap();
    assert_eq!(inspect(&dir, &manifest()).violations, ["InvalidWavFormat"]);
}
