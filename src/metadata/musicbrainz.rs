use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::{debug, warn};

#[derive(Debug, Clone, Default)]
pub struct MbMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: String,
    pub track_number: String,
    pub genre: String,
    pub recording_id: String,
}

const MB_API_BASE: &str = "https://musicbrainz.org/ws/2";

pub async fn mb_lookup_recording(
    recording_id: &str,
    user_agent: &str,
) -> Result<Option<MbMetadata>> {
    if recording_id.is_empty() {
        return Ok(None);
    }

    debug!("MB lookup recording: {}", recording_id);

    // Rate limit: 1 second between calls
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let url = format!(
        "{}/recording/{}?inc=artists+releases+tags&fmt=json",
        MB_API_BASE, recording_id
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", user_agent)
        .send()
        .await
        .context("Failed to query MusicBrainz")?;

    if !response.status().is_success() {
        warn!("MusicBrainz returned status: {}", response.status());
        return Ok(None);
    }

    let json: serde_json::Value = response
        .json()
        .await
        .context("Failed to parse MusicBrainz response")?;

    Ok(Some(parse_recording(&json, recording_id)))
}

fn parse_recording(rec: &serde_json::Value, recording_id: &str) -> MbMetadata {
    let title = rec["title"].as_str().unwrap_or("").to_string();

    // Artist credits
    let artist = if let Some(credits) = rec["artist-credit"].as_array() {
        credits
            .iter()
            .filter_map(|c| c["artist"]["name"].as_str())
            .collect::<Vec<_>>()
            .join(" & ")
    } else {
        String::new()
    };

    // First release
    let mut album = String::new();
    let mut year = String::new();
    let mut track_number = String::new();

    if let Some(releases) = rec["releases"].as_array().or_else(|| rec["release-list"].as_array()) {
        if let Some(rel) = releases.first() {
            album = rel["title"].as_str().unwrap_or("").to_string();
            let date = rel["date"].as_str().unwrap_or("");
            year = date.chars().take(4).collect();

            // Track number from first medium
            if let Some(mediums) = rel["media"].as_array().or_else(|| rel["medium-list"].as_array()) {
                'outer: for medium in mediums {
                    if let Some(tracks) = medium["tracks"].as_array().or_else(|| medium["track-list"].as_array()) {
                        for track in tracks {
                            let track_rec_id = track["recording"]["id"].as_str().unwrap_or("");
                            if track_rec_id == recording_id {
                                track_number = track["number"].as_str().unwrap_or("").to_string();
                                break 'outer;
                            }
                        }
                        // If we didn't find by recording ID, take first track number
                        if track_number.is_empty() {
                            if let Some(first_track) = tracks.first() {
                                track_number = first_track["number"].as_str().unwrap_or("").to_string();
                            }
                        }
                    }
                }
            }
        }
    }

    // Top genre tag by count
    let genre = if let Some(tags) = rec["tags"].as_array().or_else(|| rec["tag-list"].as_array()) {
        let mut best_tag = "";
        let mut best_count = -1i64;
        for tag in tags {
            let count = tag["count"].as_i64().unwrap_or(0);
            if count > best_count {
                best_count = count;
                best_tag = tag["name"].as_str().unwrap_or("");
            }
        }
        // Title-case the genre
        let mut chars = best_tag.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => first.to_uppercase().to_string() + &chars.as_str().to_lowercase(),
        }
    } else {
        String::new()
    };

    debug!(
        "MB parsed: title={} artist={} album={} year={} track={}",
        title, artist, album, year, track_number
    );

    MbMetadata {
        title,
        artist,
        album,
        year,
        track_number,
        genre,
        recording_id: recording_id.to_string(),
    }
}

pub async fn mb_search(
    title: &str,
    artist: &str,
    user_agent: &str,
) -> Result<Option<MbMetadata>> {
    if title.is_empty() {
        return Ok(None);
    }

    debug!("MB search: title={} artist={}", title, artist);

    // Rate limit
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let query = if artist.is_empty() {
        format!("recording:\"{}\"", title)
    } else {
        format!("recording:\"{}\" AND artist:\"{}\"", title, artist)
    };

    let url = format!(
        "{}/recording?query={}&fmt=json",
        MB_API_BASE,
        urlencoding_simple(&query)
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", user_agent)
        .send()
        .await
        .context("Failed to query MusicBrainz search")?;

    if !response.status().is_success() {
        warn!("MusicBrainz search returned status: {}", response.status());
        return Ok(None);
    }

    let json: serde_json::Value = response
        .json()
        .await
        .context("Failed to parse MusicBrainz search response")?;

    let recordings = match json["recordings"].as_array().or_else(|| json["recording-list"].as_array()) {
        Some(r) if !r.is_empty() => r,
        _ => return Ok(None),
    };

    let first = &recordings[0];
    let recording_id = first["id"].as_str().unwrap_or("").to_string();

    if recording_id.is_empty() {
        return Ok(None);
    }

    // Now do a full lookup
    mb_lookup_recording(&recording_id, user_agent).await
}

fn urlencoding_simple(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            b' ' => result.push('+'),
            _ => {
                result.push('%');
                result.push_str(&format!("{:02X}", byte));
            }
        }
    }
    result
}
