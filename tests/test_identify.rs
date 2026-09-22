use vripr::metadata::identify::{
    duration_agreement, extract_api_error, fingerprint_segment, format_http_error,
    parse_fpcalc_output, rank_score,
};

#[test]
fn parses_fpcalc_json() {
    let fingerprint = parse_fpcalc_output(br#"{"duration":123.4,"fingerprint":"AQADt..."}"#).unwrap();
    assert_eq!(fingerprint.duration, 123.4);
    assert_eq!(fingerprint.fingerprint, "AQADt...");
}

#[test]
fn rejects_invalid_fpcalc_output() {
    assert!(parse_fpcalc_output(br#"{"duration":0,"fingerprint":""}"#).is_err());
}

#[test]
fn reports_missing_fpcalc_actionably() {
    let wav = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(wav.path(), spec).unwrap();
        for i in 0..44100 * 2 {
            let v = ((i as f32 * 0.05).sin() * 1000.0) as i16;
            writer.write_sample(v).unwrap();
        }
        writer.finalize().unwrap();
    }
    let error = fingerprint_segment(
        "/definitely/not/fpcalc", wav.path(), 0.0, 1.0,
    ).unwrap_err();
    assert!(error.to_string().contains("Install Chromaprint"));
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
