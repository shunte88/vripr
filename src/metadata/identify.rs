use anyhow::{anyhow, bail, Context, Result};
use hound::{SampleFormat, WavSpec, WavWriter};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

const ACOUSTID_URL: &str = "https://api.acoustid.org/v2/lookup";
const MUSICBRAINZ_URL: &str = "https://musicbrainz.org/ws/2/recording";

#[derive(Debug, Clone, PartialEq)]
pub struct Fingerprint {
    pub duration: f64,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IdentificationCandidate {
    pub recording_id: String,
    pub artist: String,
    pub title: String,
    pub release: String,
    pub fingerprint_score: f32,
    pub duration_score: f32,
    pub score: f32,
}

#[derive(Deserialize)]
struct FpcalcOutput {
    duration: f64,
    fingerprint: String,
}

#[derive(Deserialize, Debug)]
struct AcoustIdError {
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct AcoustIdResponse {
    status: String,
    #[serde(default)]
    error: Option<AcoustIdError>,
    #[serde(default)]
    results: Vec<AcoustIdResult>,
}

#[derive(Deserialize)]
struct AcoustIdResult {
    score: f32,
    #[serde(default)]
    recordings: Vec<AcoustIdRecording>,
}

#[derive(Deserialize)]
struct AcoustIdRecording {
    id: String,
}

#[derive(Deserialize)]
struct MusicBrainzRecording {
    title: String,
    length: Option<i64>,
    #[serde(default)]
    #[serde(rename = "artist-credit")]
    artist_credit: Vec<ArtistCredit>,
    #[serde(default)]
    releases: Vec<MusicBrainzRelease>,
}

#[derive(Deserialize)]
struct ArtistCredit {
    name: String,
}

#[derive(Deserialize)]
struct MusicBrainzRelease {
    title: String,
}

/// Run fpcalc for a bounded segment. Its JSON output is parsed without ever
/// logging the fingerprint, which is an identifier derived from the audio.
///
/// fpcalc has no `-offset` option — it can only fingerprint from the start of
/// a file (optionally truncated with `-length`). To fingerprint an arbitrary
/// segment of the analysis audio (e.g. an individual track within a whole
/// side/album recording), the segment is decoded and written to a temporary
/// WAV file first, then fpcalc is run on that file with no offset needed.
pub fn fingerprint_segment(
    fpcalc_path: &str,
    audio_path: &Path,
    start_secs: f64,
    length_secs: f64,
) -> Result<Fingerprint> {
    let executable = if fpcalc_path.trim().is_empty() { "fpcalc" } else { fpcalc_path };
    let length = length_secs.clamp(1.0, 120.0);
    let segment_file = extract_segment_wav(audio_path, start_secs.max(0.0), length)
        .context("Could not extract the track segment from the analysis audio")?;
    let output = Command::new(executable)
        .args(["-json"])
        .arg(segment_file.path())
        .output()
        .with_context(|| format!(
            "Could not run fpcalc ({executable}). Install Chromaprint/fpcalc or set its path in Settings"
        ))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        if detail.is_empty() {
            bail!(
                "fpcalc failed. Check that it can read the analysis audio and that its configured path is correct"
            );
        }
        bail!(
            "fpcalc failed: {detail}. Check that it can read the analysis audio and that its configured path is correct"
        );
    }
    parse_fpcalc_output(&output.stdout)
}

/// Decode `audio_path` and write the `[start_secs, start_secs + length_secs)`
/// window to a temporary mono 16-bit PCM WAV file, returning the handle that
/// deletes the file on drop.
fn extract_segment_wav(
    audio_path: &Path,
    start_secs: f64,
    length_secs: f64,
) -> Result<tempfile::NamedTempFile> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::errors::Error as SErr;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let src = std::fs::File::open(audio_path)
        .with_context(|| format!("Cannot open analysis audio {:?}", audio_path))?;
    let mss = MediaSourceStream::new(Box::new(src), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = audio_path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .context("Could not read the analysis audio format")?;
    let mut format = probed.format;
    let track = format.tracks().iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("No decodable audio track found in the analysis audio"))?;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let n_channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2).max(1);
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("Could not create a decoder for the analysis audio")?;

    // Seek close to the segment start so tracks late into a long recording
    // don't require re-decoding everything before them for every call.
    // Best-effort: if the format/codec doesn't support seeking, fall back to
    // decoding from the beginning and skipping frames before start_frame below.
    let seek_result = format.seek(
        symphonia::core::formats::SeekMode::Accurate,
        symphonia::core::formats::SeekTo::Time {
            time: symphonia::core::units::Time::from(start_secs.max(0.0)),
            track_id: Some(track_id),
        },
    );

    let requested_start_frame = (start_secs * sample_rate as f64) as u64;
    let end_frame = requested_start_frame + (length_secs * sample_rate as f64) as u64;

    let mut sample_buf: Option<SampleBuffer<f32>> = None;
    // `frame_index` tracks the absolute position of decoded frames. After a
    // successful seek, decoding resumes at (approximately) requested_start_frame
    // rather than 0, so this is initialised accordingly; a failed/no-op seek
    // leaves decoding starting at 0. `start_frame` is clamped to whatever
    // position decoding actually resumes at, so a seek that lands slightly
    // past the requested start (permitted for "Accurate" mode too, on some
    // demuxers) doesn't silently drop the beginning of the segment.
    let mut frame_index: u64 = seek_result.map(|seeked| seeked.actual_ts).unwrap_or(0);
    let start_frame = requested_start_frame.max(frame_index).min(end_frame);

    let mut tmp = tempfile::Builder::new()
        .prefix("vripr_fp_")
        .suffix(".wav")
        .tempfile()
        .context("Could not create a temporary file for the track segment")?;
    let mut wrote_any = false;
    {
        let spec = WavSpec {
            channels: n_channels as u16,
            sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut writer = WavWriter::new(&mut tmp, spec)
            .context("Could not write the track segment WAV header")?;

        'decode: loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(SErr::IoError(_)) => break,
                Err(SErr::ResetRequired) => { decoder.reset(); continue; }
                Err(e) => return Err(e.into()),
            };
            if packet.track_id() != track_id { continue; }
            let decoded = match decoder.decode(&packet) {
                Ok(d) => d,
                Err(SErr::DecodeError(_)) => continue,
                Err(e) => return Err(e.into()),
            };
            let spec = *decoded.spec();
            if sample_buf.is_none() {
                sample_buf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
            }
            let buf = sample_buf.as_mut().unwrap();
            buf.copy_interleaved_ref(decoded);
            for frame in buf.samples().chunks(n_channels) {
                if frame_index >= end_frame { break 'decode; }
                if frame_index >= start_frame {
                    for &s in frame {
                        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                        writer.write_sample(v)?;
                    }
                    wrote_any = true;
                }
                frame_index += 1;
            }
        }

        writer.finalize().context("Could not finalise the track segment WAV")?;
    }

    if !wrote_any {
        bail!("the analysis audio has no samples at this track's position");
    }

    Ok(tmp)
}

pub fn parse_fpcalc_output(output: &[u8]) -> Result<Fingerprint> {
    let fingerprint: FpcalcOutput = serde_json::from_slice(output)
        .context("fpcalc returned unreadable JSON")?;
    if fingerprint.fingerprint.is_empty() || fingerprint.duration <= 0.0 {
        bail!("fpcalc did not return a usable fingerprint");
    }
    Ok(Fingerprint { duration: fingerprint.duration, fingerprint: fingerprint.fingerprint })
}

pub fn extract_api_error(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let direct_message = value.get("message").and_then(serde_json::Value::as_str);
    if let Some(message) = direct_message {
        return Some(message.to_owned());
    }

    let error = value.get("error").and_then(serde_json::Value::as_object)?;
    let message = error.get("message").and_then(serde_json::Value::as_str);
    let kind = error.get("type").and_then(serde_json::Value::as_str);
    match (message, kind) {
        (Some(message), Some(kind)) => Some(format!("{message} ({kind})")),
        (Some(message), None) => Some(message.to_owned()),
        (None, Some(kind)) => Some(kind.to_owned()),
        (None, None) => None,
    }
}

pub fn format_http_error(status: reqwest::StatusCode, body: &str) -> String {
    let detail = extract_api_error(body).or_else(|| {
        let trimmed = body.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    }).unwrap_or_else(|| "unknown error".to_string());
    let detail = detail.lines().take(2).collect::<Vec<_>>().join(" ");
    format!("HTTP {}: {}", status, detail)
}

pub fn duration_agreement(observed: f64, candidate: Option<f64>) -> f32 {
    let Some(candidate) = candidate.filter(|d| *d > 0.0) else { return 0.5 };
    (1.0 - ((observed - candidate).abs() / 30.0) as f32).clamp(0.0, 1.0)
}

pub fn rank_score(fingerprint_score: f32, duration_score: f32, hint_match: bool) -> f32 {
    (fingerprint_score.clamp(0.0, 1.0) * 0.75
        + duration_score.clamp(0.0, 1.0) * 0.20
        + if hint_match { 0.05 } else { 0.0 })
        .clamp(0.0, 1.0)
}

pub async fn identify(
    client: &reqwest::Client,
    acoustid_key: &str,
    fingerprint: &Fingerprint,
    track_duration: f64,
    title_hint: &str,
    artist_hint: &str,
) -> Result<Vec<IdentificationCandidate>> {
    if acoustid_key.trim().is_empty() {
        bail!("AcoustID API key is not set — add it in Settings → API Keys");
    }
    let duration = fingerprint.duration.round().to_string();
    let response = client.get(ACOUSTID_URL)
        .query(&[
            ("client", acoustid_key),
            ("meta", "recordings"),
            ("duration", &duration),
            ("fingerprint", &fingerprint.fingerprint),
        ])
        .send().await.context("AcoustID request failed")?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!("AcoustID request was rejected ({})", format_http_error(status, &body));
    }

    let response_text = response.text().await
        .map_err(|_| anyhow::anyhow!("AcoustID returned an unreadable response"))?;
    let response: AcoustIdResponse = serde_json::from_str(&response_text)
        .map_err(|_| anyhow::anyhow!("AcoustID returned an unreadable response"))?;
    if response.status != "ok" {
        let detail = response.error
            .as_ref()
            .and_then(|error| error.message.clone())
            .or_else(|| response.error.as_ref().and_then(|error| error.kind.clone()))
            .unwrap_or_else(|| response.status.clone());
        bail!("AcoustID could not process the lookup: {detail}");
    }

    let mut candidates = Vec::new();
    for result in response.results.into_iter().take(5) {
        for recording in result.recordings.into_iter().take(3) {
            let response = client
                .get(format!("{MUSICBRAINZ_URL}/{}", recording.id))
                .query(&[("inc", "artists+releases"), ("fmt", "json")])
                .send().await.context("MusicBrainz request failed")?;
            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                bail!("MusicBrainz request was rejected ({})", format_http_error(status, &body));
            }
            let recording_data: MusicBrainzRecording = response.json().await
                .context("MusicBrainz returned an unreadable response")?;
            let artist = recording_data.artist_credit.iter()
                .map(|credit| credit.name.as_str()).collect::<Vec<_>>().join(", ");
            let release = recording_data.releases.first()
                .map(|release| release.title.clone()).unwrap_or_default();
            let hint = !title_hint.is_empty() && recording_data.title.eq_ignore_ascii_case(title_hint)
                || !artist_hint.is_empty() && artist.eq_ignore_ascii_case(artist_hint);
            let duration_score = duration_agreement(
                track_duration,
                recording_data.length.map(|milliseconds| milliseconds as f64 / 1000.0),
            );
            candidates.push(IdentificationCandidate {
                recording_id: recording.id,
                artist,
                title: recording_data.title,
                release,
                fingerprint_score: result.score,
                duration_score,
                score: rank_score(result.score, duration_score, hint),
            });
        }
    }
    candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
    candidates.dedup_by(|a, b| a.recording_id == b.recording_id);
    Ok(candidates)
}
