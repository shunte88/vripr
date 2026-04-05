use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub struct AcoustidMatch {
    pub score: f64,
    pub recording_id: String,
    pub title: String,
    pub artist: String,
}

#[derive(Debug, Deserialize)]
struct FpcalcOutput {
    duration: f64,
    fingerprint: String,
}

#[derive(Debug, Deserialize)]
struct AcoustidResponse {
    status: String,
    results: Option<Vec<AcoustidResult>>,
}

#[derive(Debug, Deserialize)]
struct AcoustidResult {
    id: String,
    score: f64,
    recordings: Option<Vec<AcoustidRecording>>,
}

#[derive(Debug, Deserialize)]
struct AcoustidRecording {
    id: String,
    title: Option<String>,
    artists: Option<Vec<AcoustidArtist>>,
}

#[derive(Debug, Deserialize)]
struct AcoustidArtist {
    name: String,
}

pub async fn fingerprint_file(
    filepath: &Path,
    api_key: &str,
) -> Result<Option<AcoustidMatch>> {
    debug!("Fingerprinting file: {:?}", filepath);

    // Run fpcalc to get fingerprint
    let output = tokio::process::Command::new("fpcalc")
        .arg("-json")
        .arg(filepath)
        .output()
        .await
        .context("Failed to run fpcalc — is chromaprint installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("fpcalc failed: {}", stderr);
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let fpcalc: FpcalcOutput = serde_json::from_str(&stdout)
        .context("Failed to parse fpcalc JSON output")?;

    debug!("Got fingerprint, duration={:.1}s", fpcalc.duration);

    if api_key.is_empty() {
        warn!("AcoustID API key not set, skipping lookup");
        return Ok(None);
    }

    // Query AcoustID API
    let client = reqwest::Client::new();
    let response = client
        .post("https://api.acoustid.org/v2/lookup")
        .form(&[
            ("client", api_key),
            ("meta", "recordings"),
            ("duration", &fpcalc.duration.to_string()),
            ("fingerprint", &fpcalc.fingerprint),
        ])
        .send()
        .await
        .context("Failed to query AcoustID API")?;

    let acoustid_resp: AcoustidResponse = response
        .json()
        .await
        .context("Failed to parse AcoustID response")?;

    if acoustid_resp.status != "ok" {
        warn!("AcoustID API returned non-ok status: {}", acoustid_resp.status);
        return Ok(None);
    }

    let results = match acoustid_resp.results {
        Some(r) if !r.is_empty() => r,
        _ => return Ok(None),
    };

    // Find result with highest score > 0.5
    let best = results
        .into_iter()
        .filter(|r| r.score > 0.5)
        .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));

    let result = match best {
        Some(r) => r,
        None => return Ok(None),
    };

    // Get recording info
    let (recording_id, title, artist) = if let Some(recordings) = result.recordings {
        if let Some(rec) = recordings.into_iter().next() {
            let title = rec.title.unwrap_or_default();
            let artist = rec
                .artists
                .unwrap_or_default()
                .into_iter()
                .map(|a| a.name)
                .collect::<Vec<_>>()
                .join(" & ");
            (rec.id, title, artist)
        } else {
            (result.id, String::new(), String::new())
        }
    } else {
        (result.id, String::new(), String::new())
    };

    debug!(
        "AcoustID match: score={:.2} recording={} title={}",
        result.score, recording_id, title
    );

    Ok(Some(AcoustidMatch {
        score: result.score,
        recording_id,
        title,
        artist,
    }))
}
