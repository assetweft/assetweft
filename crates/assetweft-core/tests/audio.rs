use assetweft_core::assurance::{AudioReport, ValidationLimits, checks::inspect_audio_file};
use std::fs;

fn wav(codec: u16, bits: u16, channels: u16, rate: u32, samples: &[u8]) -> Vec<u8> {
    let align = channels * (bits / 8);
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((36 + samples.len().next_multiple_of(2) as u32).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16_u32.to_le_bytes());
    bytes.extend(codec.to_le_bytes());
    bytes.extend(channels.to_le_bytes());
    bytes.extend(rate.to_le_bytes());
    bytes.extend((rate * u32::from(align)).to_le_bytes());
    bytes.extend(align.to_le_bytes());
    bytes.extend(bits.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend((samples.len() as u32).to_le_bytes());
    bytes.extend(samples);
    bytes.resize(bytes.len().next_multiple_of(2), 0);
    bytes
}

fn inspect(bytes: &[u8], spec: Option<&str>) -> Result<AudioReport, &'static str> {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("audio.wav");
    let spec_path = dir.path().join("spec.json");
    fs::write(&file, bytes).unwrap();
    if let Some(text) = spec {
        fs::write(&spec_path, text).unwrap();
    }
    inspect_audio_file(
        &file,
        spec.map(|_| spec_path.as_path()),
        ValidationLimits::default(),
    )
    .map_err(|e| e.field)
}

#[test]
fn normalized_samples_have_the_same_digest_across_codecs() {
    let pcm16: Vec<_> = [0_i16, 16_384, -16_384, 0]
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect();
    let pcm24 = [0, 0, 0, 0, 0, 64, 0, 0, 192, 0, 0, 0];
    let floats: Vec<_> = [-0.0_f32, 0.5, -0.5, 0.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    for (codec, bits, bytes) in [
        (1, 8, &[128, 192, 64, 128][..]),
        (1, 16, &pcm16),
        (1, 24, &pcm24),
        (3, 32, &floats),
    ] {
        let report = inspect(&wav(codec, bits, 1, 3000, bytes), None).unwrap();
        assert_eq!(
            report.pcm_sha256,
            "99b0378ae42a430bd3968829279e4bf38dbdc99e8c80dd2b404e489daa1f633b"
        );
        assert_eq!(report.bits_per_sample, bits);
        assert_eq!(report.sample_frames, 4);
        assert_eq!(report.duration_ms, 1.333);
        assert_eq!(report.peak_amplitude, 0.5);
        assert_eq!(report.rms_level, 0.125_f64.sqrt());
        assert_eq!(report.clipped_samples, 0);
        assert!(!report.silent);
    }
}

#[test]
fn loop_checks_interleaved_channels_and_both_endpoint_bounds() {
    let file = wav(1, 8, 2, 4, &[128, 160, 192, 160, 64, 176, 128, 160]);
    let spec = |end, step, offset| {
        format!(
            r#"{{"expected_channels":2,"expected_sample_rate_hz":4,"min_duration_ms":1000,"max_duration_ms":1000,"transcript_text":"音a","loop_region":{{"start_sample":0,"end_sample":{end},"max_step_delta":{step},"max_zero_crossing_offset":{offset}}}}}"#
        )
    };
    let result = inspect(&file, Some(&spec(4, 0.0, 0.25))).unwrap();
    assert_eq!(result.transcript_text.as_deref(), Some("音a"));
    assert_eq!(result.chars_per_sec, Some(2.0));
    assert_eq!(result.asr_playback_status, "NOT_RUN");
    let seam = result.loop_region.unwrap();
    assert_eq!((seam.step_delta, seam.zero_crossing_offset), (0.0, 0.25));
    assert!(result.violations.is_empty());
    for spec in [spec(3, 0.1, 1.0), spec(4, 0.0, 0.24)] {
        assert_eq!(
            inspect(&file, Some(&spec)).unwrap().violations,
            ["LoopBoundaryClick"]
        );
    }
    for spec in [
        spec(5, 0.0, 0.25),
        spec(4, -0.1, 0.25),
        spec(4, 0.0, 1.0).replace(
            "\"max_zero_crossing_offset\":1",
            "\"max_zero_crossing_offset\":1e999",
        ),
    ] {
        assert_eq!(inspect(&file, Some(&spec)).unwrap_err(), "spec");
    }
    for (byte, violation) in [(128, "Silent"), (0, "Clipping")] {
        let file = wav(1, 8, 1, 1, &[byte]);
        assert!(inspect(&file, None).unwrap().violations.is_empty());
        assert_eq!(inspect(&file, Some("{}")).unwrap().violations, [violation]);
    }
}

#[test]
fn rejects_malformed_riff_and_nonfinite_samples() {
    let valid = wav(1, 16, 2, 8000, &[0, 0, 0, 0]);
    for (offset, value) in [
        (4, 0),
        (16, 17),
        (20, 2),
        (22, 0),
        (28, 1),
        (32, 2),
        (40, 5),
        (40, 3),
    ] {
        let mut invalid = valid.clone();
        invalid[offset] = value;
        assert_eq!(
            inspect(&invalid, None).unwrap_err(),
            "file",
            "offset {offset}"
        );
    }
    assert_eq!(
        inspect(&valid[..valid.len() - 1], None).unwrap_err(),
        "file"
    );
    assert_eq!(
        inspect(&wav(1, 16, 2, 8000, &[0, 0]), None).unwrap_err(),
        "file"
    );
    assert_eq!(inspect(&wav(1, 8, 1, 0, &[128]), None).unwrap_err(), "file");
    let mut duplicate = valid.clone();
    duplicate.extend_from_slice(&valid[36..]);
    let size = (duplicate.len() - 8) as u32;
    duplicate[4..8].copy_from_slice(&size.to_le_bytes());
    assert_eq!(inspect(&duplicate, None).unwrap_err(), "file");
    for sample in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            inspect(&wav(3, 32, 1, 8000, &sample.to_le_bytes()), None).unwrap_err(),
            "file"
        );
    }
}
