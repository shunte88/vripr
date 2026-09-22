use vripr::metadata::identify::{
    build_fingerprint, duration_agreement, extract_api_error, fingerprint_segment,
    format_http_error, rank_score,
};

/// Write `secs` seconds of a 440 Hz-ish sweep to a temporary mono 44.1 kHz WAV.
fn tone_wav(secs: u32) -> tempfile::NamedTempFile {
    let wav = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(wav.path(), spec).unwrap();
    let total = 44100 * secs;
    for i in 0..total {
        let t = i as f32 / 44100.0;
        // Two detuned partials plus a slow sweep so the chroma features vary.
        let v = ((t * 440.0 * std::f32::consts::TAU).sin() * 0.4
            + (t * (660.0 + t * 20.0) * std::f32::consts::TAU).sin() * 0.3)
            * 12000.0;
        writer.write_sample(v as i16).unwrap();
    }
    writer.finalize().unwrap();
    wav
}

#[test]
fn fingerprints_a_segment_in_process() {
    let wav = tone_wav(12);
    let fingerprint = fingerprint_segment(wav.path(), 2.0, 8.0).unwrap();
    assert!(!fingerprint.fingerprint.is_empty());
    // Duration is the length of audio actually fingerprinted, not the file's.
    assert!((fingerprint.duration - 8.0).abs() < 0.1, "got {}", fingerprint.duration);
    // Chromaprint's compressed fingerprints are base64 and start with the
    // algorithm marker for Test2, the variant AcoustID expects.
    assert!(fingerprint.fingerprint.starts_with('A'));
}

#[test]
fn identical_segments_fingerprint_identically() {
    let wav = tone_wav(12);
    let a = fingerprint_segment(wav.path(), 1.0, 6.0).unwrap();
    let b = fingerprint_segment(wav.path(), 1.0, 6.0).unwrap();
    assert_eq!(a, b);
}

#[test]
fn rejects_an_empty_fingerprint() {
    assert!(build_fingerprint(String::new(), 12.0).is_err());
    assert!(build_fingerprint("AQADt...".into(), 0.0).is_err());
    let ok = build_fingerprint("AQADt...".into(), 123.4).unwrap();
    assert_eq!(ok.duration, 123.4);
    assert_eq!(ok.fingerprint, "AQADt...");
}

#[test]
fn reports_unreadable_audio_actionably() {
    let missing = std::path::Path::new("/definitely/not/an/audio/file.wav");
    let error = fingerprint_segment(missing, 0.0, 1.0).unwrap_err();
    assert!(error.to_string().contains("Cannot open analysis audio"));
}

#[test]
fn reports_a_segment_past_the_end_of_the_audio() {
    let wav = tone_wav(4);
    let error = fingerprint_segment(wav.path(), 600.0, 10.0).unwrap_err();
    assert!(error.to_string().contains("no samples at this track's position"));
}

#[test]
fn ranking_rewards_fingerprint_and_duration() {
    assert!(rank_score(0.9, 1.0, false) > rank_score(0.9, 0.0, false));
    assert!(rank_score(0.9, 1.0, true) > rank_score(0.9, 1.0, false));
    assert_eq!(duration_agreement(180.0, Some(180.0)), 1.0);
    assert_eq!(duration_agreement(180.0, Some(220.0)), 0.0);
}

#[test]
fn reports_acoustid_api_error_details() {
    let payload = r#"{"status":"error","error":{"message":"API key is invalid","type":"invalid_api_key"}}"#;
    let message = extract_api_error(payload).unwrap();
    assert!(message.contains("API key is invalid"));
    assert!(message.contains("invalid_api_key"));

    let formatted = format_http_error(reqwest::StatusCode::FORBIDDEN, payload);
    assert!(formatted.contains("403"));
    assert!(formatted.contains("API key is invalid"));
}
