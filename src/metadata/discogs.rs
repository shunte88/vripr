use anyhow::{Context, Result};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A single track entry from a Discogs release tracklist.
#[derive(Debug, Clone)]
pub struct DiscogsTrack {
    /// Raw Discogs position string (e.g. "A1", "B2", "AA", "3").
    pub position: String,
    /// Vinyl side letter ('A', 'B', 'C', 'D'). '?' for unresolved numeric positions.
    pub side: char,
    /// Track number within the side (1-based).
    pub number: u32,
    pub title: String,
    /// Original duration string from Discogs (e.g. "5:24").
    pub duration_str: String,
    /// Parsed duration in seconds, if available.
    pub duration_secs: Option<f64>,
}

impl DiscogsTrack {
    fn new(position: &str, title: &str, duration: &str) -> Self {
        let (side, number) = parse_vinyl_position(position);
        let duration_secs = parse_duration(duration);
        DiscogsTrack {
            position: position.to_string(),
            side,
            number,
            title: title.to_string(),
            duration_str: duration.to_string(),
            duration_secs,
        }
    }
}

/// A full Discogs release with tracklist.
#[derive(Debug, Clone)]
pub struct DiscogsRelease {
    pub release_id: String,
    pub album: String,
    pub album_artist: String,
    pub year: String,
    pub genre: String,
    pub label: String,
    pub country: String,
    pub catalog: String,
    /// Primary cover art URL (thumbnail, ~150px), if available.
    pub cover_image_url: Option<String>,
    /// All tracks ordered as they appear on the release.
    pub tracks: Vec<DiscogsTrack>,
}

impl DiscogsRelease {
    /// Tracks for a specific vinyl side, in order.
    pub fn side_tracks(&self, side: char) -> Vec<&DiscogsTrack> {
        self.tracks.iter().filter(|t| t.side == side).collect()
    }

    /// All unique sides present (e.g. ['A', 'B']).
    pub fn sides(&self) -> Vec<char> {
        let mut sides: Vec<char> = self.tracks.iter().map(|t| t.side).collect();
        sides.dedup();
        sides.sort();
        sides.dedup();
        sides
    }

    /// Total duration for a side in seconds (None if any track has no duration).
    pub fn side_duration_secs(&self, side: char) -> Option<f64> {
        let tracks = self.side_tracks(side);
        if tracks.is_empty() { return None; }
        tracks.iter().try_fold(0.0f64, |acc, t| {
            t.duration_secs.map(|d| acc + d)
        })
    }
}

/// A single search-result candidate — enough to display a picker without
/// fetching the full release for every hit.
#[derive(Debug, Clone, Default)]
pub struct DiscogsCandidate {
    pub id:          String,
    /// Raw Discogs title field — usually "Artist - Album" format.
    pub raw_title:   String,
    pub artist:      String,
    pub album:       String,
    pub year:        String,
    pub label:       String,
    pub format:      String,
    pub country:     String,
    pub catno:       String,
    /// Discogs release page path, e.g. "/release/12345-Artist-Album"
    pub uri:         String,
    /// Number of tracks — from `tracklist` when available in search results.
    pub track_count: Option<usize>,
}

impl DiscogsCandidate {
    /// One-line summary for display in a picker list.
    pub fn summary(&self) -> String {
        let mut parts = vec![self.artist.clone(), self.album.clone()];
        if !self.year.is_empty()    { parts.push(format!("({})", self.year)); }
        if !self.label.is_empty()   { parts.push(format!("[{}]", self.label)); }
        if !self.format.is_empty()  { parts.push(self.format.clone()); }
        if !self.country.is_empty() { parts.push(self.country.clone()); }
        parts.join(" ")
    }
}

/// Basic album-level metadata (returned by quick search, kept for backward compat).
#[derive(Debug, Clone, Default)]
pub struct DiscogsMetadata {
    pub album: String,
    pub album_artist: String,
    pub year: String,
    pub genre: String,
    pub release_id: String,
}

// ---------------------------------------------------------------------------
// Vinyl position parsing
// ---------------------------------------------------------------------------

/// Parse a Discogs position string into (side, track_number).
///
/// Handles:
/// - Standard:  "A1" → ('A', 1),  "B2" → ('B', 2)
/// - Letter run: "A" → ('A', 1),  "AA" → ('A', 2),  "AAA" → ('A', 3)
/// - Numeric:   "1" → ('?', 1) — caller handles A/B split
/// - Empty:     ('?', 0)
pub fn parse_vinyl_position(pos: &str) -> (char, u32) {
    let pos = pos.trim();
    if pos.is_empty() {
        return ('?', 0);
    }

    let first = pos.chars().next().unwrap();

    if first.is_ascii_uppercase() {
        // All characters are the same letter: "A", "AA", "AAA" → side A, track 1/2/3
        if pos.chars().all(|c| c == first) {
            return (first, pos.len() as u32);
        }
        // Letter + digits: "A1", "B12"
        let rest = &pos[first.len_utf8()..];
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(n) = rest.parse::<u32>() {
                return (first, n);
            }
        }
    }

    // Pure numeric position — side unresolved, caller assigns A/B split
    if let Ok(n) = pos.parse::<u32>() {
        return ('?', n);
    }

    // Fallback
    ('?', 1)
}

// ---------------------------------------------------------------------------
// Duration parsing
// ---------------------------------------------------------------------------

/// Parse "M:SS" or "H:MM:SS" into total seconds.
fn parse_duration(s: &str) -> Option<f64> {
    if s.is_empty() { return None; }
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        2 => {
            let m = parts[0].parse::<f64>().ok()?;
            let s = parts[1].parse::<f64>().ok()?;
            Some(m * 60.0 + s)
        }
        3 => {
            let h = parts[0].parse::<f64>().ok()?;
            let m = parts[1].parse::<f64>().ok()?;
            let s = parts[2].parse::<f64>().ok()?;
            Some(h * 3600.0 + m * 60.0 + s)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tracklist parsing from Discogs JSON
// ---------------------------------------------------------------------------

fn parse_tracklist(tracklist: &serde_json::Value) -> Vec<DiscogsTrack> {
    let Some(arr) = tracklist.as_array() else { return Vec::new() };

    let mut tracks: Vec<DiscogsTrack> = Vec::new();
    let mut sequential: Vec<(u32, String, String)> = Vec::new(); // (num, title, dur)

    for item in arr {
        // Skip headings / non-track entries
        let type_ = item["type_"].as_str().unwrap_or("track");
        if type_ == "heading" { continue; }

        let pos   = item["position"].as_str().unwrap_or("").trim();
        let title = item["title"].as_str().unwrap_or("").trim();
        let dur   = item["duration"].as_str().unwrap_or("").trim();

        if title.is_empty() { continue; }

        let (side, num) = parse_vinyl_position(pos);

        if side == '?' {
            // Pure numeric — collect for A/B split
            sequential.push((num, title.to_string(), dur.to_string()));
        } else {
            tracks.push(DiscogsTrack::new(pos, title, dur));
        }
    }

    // Convert sequential numeric positions: first half → side A, second half → side B
    if !sequential.is_empty() {
        sequential.sort_by_key(|(n, _, _)| *n);
        let total = sequential.len();
        let half  = (total + 1) / 2;
        for (i, (_, title, dur)) in sequential.into_iter().enumerate() {
            let (side, num) = if i < half {
                ('A', (i + 1) as u32)
            } else {
                ('B', (i - half + 1) as u32)
            };
            let fake_pos = format!("{}{}", side, num);
            tracks.push(DiscogsTrack::new(&fake_pos, &title, &dur));
        }
    }

    // Sort by side then track number
    tracks.sort_by(|a, b| {
        a.side.cmp(&b.side).then(a.number.cmp(&b.number))
    });

    tracks
}

// ---------------------------------------------------------------------------
// HTTP client helpers
// ---------------------------------------------------------------------------

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("vripr/0.2.0 (https://github.com/shunte88/vripr)")
        .build()
        .expect("Failed to build reqwest client — TLS backend not available")
}

async fn rate_limit() {
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}

// ---------------------------------------------------------------------------
// Public API functions
// ---------------------------------------------------------------------------

/// Fetch a complete Discogs release (with full tracklist) by numeric release ID.
pub async fn discogs_fetch_release(
    release_id: &str,
    token: &str,
) -> Result<Option<DiscogsRelease>> {
    if token.is_empty() || release_id.is_empty() {
        return Ok(None);
    }

    rate_limit().await;

    let url = format!(
        "https://api.discogs.com/releases/{}?token={}",
        release_id, token
    );

    let resp = http_client()
        .get(&url)
        .send()
        .await
        .context("Discogs release fetch failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Discogs release/{} HTTP {} — {}", release_id, status, body.trim()));
    }

    let json: serde_json::Value = resp.json().await.context("Discogs release parse failed")?;
    Ok(Some(release_from_json(&json, release_id)))
}

/// Search Discogs and return the first matching release with full tracklist.
///
/// `query` can be "Artist Album Title" or just an album title.
pub async fn discogs_search_release(
    query: &str,
    token: &str,
) -> Result<Option<DiscogsRelease>> {
    if token.is_empty() || query.trim().is_empty() {
        return Ok(None);
    }

    debug!("Discogs search_release: {}", query);

    let url = format!(
        "https://api.discogs.com/database/search?q={}&type=release&token={}",
        urlencoding_simple(query),
        token
    );

    let resp = http_client()
        .get(&url)
        .send()
        .await
        .context("Discogs search failed")?;

    if !resp.status().is_success() {
        warn!("Discogs search returned {}", resp.status());
        return Ok(None);
    }

    let json: serde_json::Value = resp.json().await.context("Discogs search parse failed")?;

    let id = match json["results"].as_array().and_then(|r| r.first()) {
        Some(first) => first["id"].as_u64().map(|id| id.to_string()),
        None => return Ok(None),
    };

    let Some(release_id) = id else { return Ok(None) };

    // Fetch the full release to get tracklist
    discogs_fetch_release(&release_id, token).await
}

/// Search Discogs and return up to `max_results` candidates for user selection.
///
/// Each candidate contains enough metadata to populate a picker list without
/// fetching the full release for every hit.
pub async fn discogs_search_candidates(
    query:       &str,
    token:       &str,
    max_results: usize,
) -> Result<Vec<DiscogsCandidate>> {
    if token.is_empty() || query.trim().is_empty() {
        return Ok(Vec::new());
    }

    debug!("Discogs candidates search: {}", query);

    let url = format!(
        "https://api.discogs.com/database/search?q={}&type=release&per_page={}&token={}",
        urlencoding_simple(query),
        max_results.max(1).min(50),
        token
    );

    rate_limit().await;

    let resp = http_client()
        .get(&url)
        .send()
        .await
        .context("Discogs search failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Discogs HTTP {} — {}", status, body.trim()));
    }

    let json: serde_json::Value = resp.json().await.context("Discogs search parse failed")?;

    debug!("Discogs raw payload: {}", serde_json::to_string_pretty(&json).unwrap_or_default());
    let raw_count = json["results"].as_array().map(|a| a.len()).unwrap_or(0);
    info!("Discogs search returned {} raw result(s) for {:?}", raw_count, query);

    let results = match json["results"].as_array() {
        Some(r) if !r.is_empty() => r,
        _ => return Ok(Vec::new()),
    };

    let candidates = results
        .iter()
        .take(max_results)
        .filter_map(|item| {
            let id = item["id"].as_u64()?.to_string();

            let raw_title = item["title"].as_str().unwrap_or("").to_string();
            let (artist, album) = if let Some(idx) = raw_title.find(" - ") {
                (raw_title[..idx].to_string(), raw_title[idx + 3..].to_string())
            } else {
                (String::new(), raw_title.clone())
            };

            let year = item["year"].as_str()
                .map(|s| s.to_string())
                .or_else(|| item["year"].as_u64().map(|y| y.to_string()))
                .unwrap_or_default();

            let label = item["label"].as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let format = item["format"].as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();

            let country = item["country"].as_str().unwrap_or("").to_string();
            let catno   = item["catno"].as_str().unwrap_or("").to_string();
            let uri     = item["uri"].as_str().unwrap_or("").to_string();
            let track_count = item["tracklist"].as_array().map(|a| a.len());

            Some(DiscogsCandidate { id, raw_title, artist, album, year, label, format, country, catno, uri, track_count })
        })
        .collect();

    Ok(candidates)
}

/// Quick search — returns basic album metadata only (no tracklist).
/// Kept for backward compatibility with the fingerprint worker.
pub async fn discogs_search(
    artist: &str,
    album: &str,
    token: &str,
) -> Result<Option<DiscogsMetadata>> {
    if token.is_empty() { return Ok(None); }
    if artist.is_empty() && album.is_empty() { return Ok(None); }

    // Build a general `q=` query — same approach as vinylflow; more reliable than
    // field-specific artist= / release_title= which is stricter and breaks on partial data.
    let query = match (artist.is_empty(), album.is_empty()) {
        (false, false) => format!("{} {}", artist, album),
        (false, true)  => artist.to_string(),
        (true,  false) => album.to_string(),
        (true,  true)  => unreachable!(),
    };

    info!("Discogs search: {:?}", query);

    let url = format!(
        "https://api.discogs.com/database/search?q={}&type=release&token={}",
        urlencoding_simple(&query),
        token
    );

    rate_limit().await;

    let resp = http_client()
        .get(&url)
        .send()
        .await
        .context("Discogs search failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Discogs HTTP {} — {}", status, body.trim()));
    }

    let json: serde_json::Value = resp.json().await.context("Discogs response parse failed")?;

    debug!("Discogs raw payload: {}", serde_json::to_string_pretty(&json).unwrap_or_default());
    let raw_count = json["results"].as_array().map(|a| a.len()).unwrap_or(0);
    info!("Discogs search returned {} raw result(s)", raw_count);

    let results = match json["results"].as_array() {
        Some(r) if !r.is_empty() => r,
        _ => return Ok(None),
    };

    let first = &results[0];
    let raw_title = first["title"].as_str().unwrap_or("").to_string();
    let album_title = if raw_title.contains(" - ") {
        raw_title.splitn(2, " - ").nth(1).unwrap_or(&raw_title).to_string()
    } else {
        raw_title
    };

    let album_artist = first["artists"].as_array()
        .and_then(|a| a.first())
        .and_then(|a| a["name"].as_str())
        .unwrap_or("")
        .to_string();

    let year = first["year"].as_str()
        .map(|s| s.to_string())
        .or_else(|| first["year"].as_u64().map(|y| y.to_string()))
        .unwrap_or_default();

    let genre = first["genre"].as_array()
        .and_then(|g| g.first())
        .and_then(|g| g.as_str())
        .unwrap_or("")
        .to_string();

    let release_id = first["id"].as_u64()
        .map(|id| id.to_string())
        .unwrap_or_default();

    info!("Discogs: {} / {} ({}) genre={}", album_title, album_artist, year, genre);

    Ok(Some(DiscogsMetadata { album: album_title, album_artist, year, genre, release_id }))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn release_from_json(json: &serde_json::Value, release_id: &str) -> DiscogsRelease {
    let album = json["title"].as_str().unwrap_or("").to_string();

    let album_artist = json["artists"].as_array()
        .and_then(|a| a.first())
        .and_then(|a| a["name"].as_str())
        .map(clean_artist_name)
        .unwrap_or_default();

    let year = json["year"].as_u64()
        .map(|y| y.to_string())
        .unwrap_or_default();

    let genre = json["genres"].as_array()
        .and_then(|g| g.first())
        .and_then(|g| g.as_str())
        .unwrap_or("")
        .to_string();

    let label = json["labels"].as_array()
        .and_then(|l| l.first())
        .and_then(|l| l["name"].as_str())
        .unwrap_or("")
        .to_string();

    let country = json["country"].as_str().unwrap_or("").to_string();

    let catalog = json["labels"].as_array()
        .and_then(|l| l.first())
        .and_then(|l| l["catno"].as_str())
        .unwrap_or("")
        .to_string();

    let tracks = parse_tracklist(&json["tracklist"]);

    // Prefer the 150px thumbnail; fall back to the full URI.
    let cover_image_url = json["images"].as_array()
        .and_then(|imgs| {
            // Try primary first
            imgs.iter()
                .find(|img| img["type"].as_str() == Some("primary"))
                .or_else(|| imgs.first())
        })
        .and_then(|img| {
            img["uri150"].as_str()
                .or_else(|| img["uri"].as_str())
        })
        .map(|s| s.to_string());

    debug!(
        "Discogs release {}: {} - {} ({}) {} tracks, cover={}",
        release_id, album_artist, album, year, tracks.len(),
        cover_image_url.as_deref().unwrap_or("none")
    );

    DiscogsRelease {
        release_id: release_id.to_string(),
        album,
        album_artist,
        year,
        genre,
        label,
        country,
        catalog,
        cover_image_url,
        tracks,
    }
}

/// Discogs often appends " (N)" to disambiguate artist names — strip it.
fn clean_artist_name(name: &str) -> String {
    let name = name.trim();
    // Remove trailing " (N)" where N is digits
    if let Some(idx) = name.rfind(" (") {
        let suffix = &name[idx..];
        if suffix.ends_with(')') && suffix[2..suffix.len()-1].chars().all(|c| c.is_ascii_digit()) {
            return name[..idx].to_string();
        }
    }
    name.to_string()
}

/// Fetch raw image bytes from a Discogs CDN URL.
pub async fn discogs_fetch_image(url: &str) -> Result<Vec<u8>> {
    if url.is_empty() {
        return Err(anyhow::anyhow!("Empty image URL"));
    }
    let resp = http_client()
        .get(url)
        .send()
        .await
        .context("Cover image fetch failed")?;
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("Cover image HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().await.context("Cover image read failed")?;
    Ok(bytes.to_vec())
}

/// Public re-export of URL encoder so callers can construct masked log URLs.
pub fn discogs_encode_query(s: &str) -> String {
    urlencoding_simple(s)
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
