/*
 *  export.rs
 *
 *  vripr - The vinyl viper for perfect rippage - Audacity vinyl ripping helper
 *	(c) 2025-26 Stuart Hunter
 *
 *	TODO:
 *
 * MIT License
 * 
 * Copyright (c) 2026 VRipr Contributors
 * 
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 * 
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 * 
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 *
 */

#[allow(dead_code)]
#[allow(unused_imports)]
use std::sync::{Arc, Mutex};

use crate::config::Config;
use crate::pipe::AudacityPipe;
use crate::tagging::write_tags;
use crate::track::TrackMeta;
use crate::workers::{AppSender, TrackUpdate, WorkerMessage};

// ---------------------------------------------------------------------------
// Path template validation (used by the settings dialog for live feedback)
// ---------------------------------------------------------------------------

/// All token names recognised by `apply_path_template`.
pub const SUPPORTED_TOKENS: &[&str] = &[
    "title", "artist", "album", "album_artist", "genre", "year",
    "tracknum", "composer", "country", "country_iso", "catalog", "label", "discogs_id",
];

/// A single unknown token found during template validation.
#[derive(Debug, Clone)]
pub struct TemplateTokenError {
    /// The token name as written (without braces).
    pub token:      String,
    /// Best-guess replacement, if one could be found.
    pub suggestion: Option<String>,
}

/// Parse `template`, return one error per unknown `{token}`.
/// Unclosed braces are ignored (not an error).
pub fn validate_path_template(template: &str) -> Vec<TemplateTokenError> {
    let mut errors = Vec::new();
    let mut s = template;
    while let Some(open) = s.find('{') {
        s = &s[open + 1..];
        let Some(close) = s.find('}') else { break };
        let token = &s[..close];
        s = &s[close + 1..];
        if !token.is_empty() && !SUPPORTED_TOKENS.contains(&token) {
            errors.push(TemplateTokenError {
                token:      token.to_string(),
                suggestion: suggest_token(token),
            });
        }
    }
    errors
}

/// Return the best supported-token suggestion for an unknown token, or `None`.
fn suggest_token(unknown: &str) -> Option<String> {
    // 1. Explicit alias table — most common typos / alternate naming conventions
    let aliases: &[(&str, &str)] = &[
        ("track",            "tracknum"),
        ("track_number",     "tracknum"),
        ("track_no",         "tracknum"),
        ("trackno",          "tracknum"),
        ("track_num",        "tracknum"),
        ("number",           "tracknum"),
        ("num",              "tracknum"),
        ("album_name",       "album"),
        ("album_title",      "album"),
        ("record",           "album"),
        ("country_code",     "country_iso"),
        ("iso",              "country_iso"),
        ("iso_country",      "country_iso"),
        ("catno",            "catalog"),
        ("cat_no",           "catalog"),
        ("catalog_number",   "catalog"),
        ("catalogue",        "catalog"),
        ("catalogue_number", "catalog"),
        ("catalog_no",       "catalog"),
        ("release_id",       "discogs_id"),
        ("discogs",          "discogs_id"),
        ("discogs_release",  "discogs_id"),
        ("album_artist_name","album_artist"),
    ];
    for &(alias, target) in aliases {
        if unknown.eq_ignore_ascii_case(alias) {
            return Some(target.to_string());
        }
    }

    // 2. Normalization: strip underscores and compare lowercase
    let unknown_norm: String = unknown.chars().filter(|&c| c != '_').collect::<String>().to_lowercase();
    for &sup in SUPPORTED_TOKENS {
        let sup_norm: String = sup.chars().filter(|&c| c != '_').collect::<String>();
        if sup_norm == unknown_norm {
            return Some(sup.to_string());
        }
    }

    // 3. Prefix match: unknown is a prefix of a supported token (≥ 3 chars)
    if unknown.len() >= 3 {
        for &sup in SUPPORTED_TOKENS {
            if sup.starts_with(unknown) {
                return Some(sup.to_string());
            }
        }
    }

    // 4. Containment: a supported token (≥ 4 chars) appears inside the unknown string
    for &sup in SUPPORTED_TOKENS {
        if sup.len() >= 4 && unknown.contains(sup) {
            return Some(sup.to_string());
        }
    }

    // 5. Levenshtein: accept if distance ≤ max(2, shorter_length / 3)
    let (best, dist) = SUPPORTED_TOKENS
        .iter()
        .map(|&s| (s, levenshtein(unknown, s)))
        .min_by_key(|&(_, d)| d)?;
    let threshold = 2.max(unknown.len().min(best.len()) / 3);
    if dist <= threshold {
        Some(best.to_string())
    } else {
        None
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m { dp[i][0] = i; }
    for j in 0..=n { dp[0][j] = j; }
    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1]
            } else {
                1 + dp[i - 1][j].min(dp[i][j - 1]).min(dp[i - 1][j - 1])
            };
        }
    }
    dp[m][n]
}

// ---------------------------------------------------------------------------

fn sanitize_filename(s: &str) -> String {
    if s.is_empty() {
        return "Unknown".to_string();
    }
    s.chars()
        .map(|c| match c {
            '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\\' => '_',
            _ => c,
        })
        .collect()
}

/// Map a Discogs country name to its ISO 3166-1 alpha-2 code.
/// Returns the original string if no mapping is found (already a code, or unusual name).
fn country_to_iso(country: &str) -> &str {
    match country.trim() {
        "UK"                   => "GB",
        "US"                   => "US",
        "Germany"              => "DE",
        "France"               => "FR",
        "Japan"                => "JP",
        "Italy"                => "IT",
        "Netherlands"          => "NL",
        "Australia"            => "AU",
        "Canada"               => "CA",
        "Spain"                => "ES",
        "Brazil"               => "BR",
        "Belgium"              => "BE",
        "Sweden"               => "SE",
        "Norway"               => "NO",
        "Denmark"              => "DK",
        "Finland"              => "FI",
        "Switzerland"          => "CH",
        "Austria"              => "AT",
        "New Zealand"          => "NZ",
        "South Africa"         => "ZA",
        "Mexico"               => "MX",
        "Argentina"            => "AR",
        "Portugal"             => "PT",
        "Greece"               => "GR",
        "Poland"               => "PL",
        "Czech Republic"       => "CZ",
        "Hungary"              => "HU",
        "Romania"              => "RO",
        "Bulgaria"             => "BG",
        "Russia"               => "RU",
        "Yugoslavia"           => "YU",
        "India"                => "IN",
        "South Korea"          => "KR",
        "Taiwan"               => "TW",
        "Hong Kong"            => "HK",
        "Israel"               => "IL",
        "Turkey"               => "TR",
        "Venezuela"            => "VE",
        "Colombia"             => "CO",
        "Chile"                => "CL",
        "Uruguay"              => "UY",
        "Ireland"              => "IE",
        "Iceland"              => "IS",
        other                  => other,
    }
}

/// Remove `[...]` groups whose interior is empty or whitespace-only.
/// Iterates until no more can be collapsed (handles adjacent groups like `[][]`).
fn collapse_empty_brackets(s: &str) -> String {
    let mut result = s.to_string();
    loop {
        let mut changed = false;
        let bytes = result.as_bytes();
        if let Some(open) = bytes.iter().position(|&b| b == b'[') {
            if let Some(rel_close) = bytes[open + 1..].iter().position(|&b| b == b']') {
                let close = open + 1 + rel_close;
                let inner = &result[open + 1..close];
                if inner.trim().is_empty() {
                    result = format!("{}{}", &result[..open], &result[close + 1..]);
                    changed = true;
                }
            }
        }
        if !changed { break; }
    }
    result
}

/// Expand a template string using per-track metadata tokens, returning the
/// substituted `String` with empty bracket groups collapsed.
///
/// This is the shared substitution core used by both the path template and the
/// album name format.
pub fn apply_token_string(template: &str, track: &TrackMeta) -> String {
    let tracknum = if track.track_number.is_empty() {
        "00".to_string()
    } else {
        track.track_number.parse::<u32>()
            .map(|n| format!("{:02}", n))
            .unwrap_or_else(|_| track.track_number.clone())
    };

    let mut s = template.to_string();
    s = s.replace("{title}",        &track.title);
    s = s.replace("{artist}",       &track.artist);
    s = s.replace("{album}",        &track.album);
    s = s.replace("{album_artist}", &track.album_artist);
    s = s.replace("{genre}",        &track.genre);
    s = s.replace("{year}",         &track.year);
    s = s.replace("{tracknum}",     &tracknum);
    s = s.replace("{composer}",     &track.composer);
    s = s.replace("{country}",      &track.country);
    s = s.replace("{country_iso}",  country_to_iso(&track.country));
    s = s.replace("{catalog}",      &track.catalog);
    s = s.replace("{label}",        &track.label);
    s = s.replace("{discogs_id}",   &track.discogs_release_id);

    collapse_empty_brackets(&s)
}

/// Expand a path template using per-track metadata tokens, returning a relative
/// `PathBuf` (without extension). Path components are split on `/`, each segment
/// is sanitised. Empty bracket groups are collapsed before splitting.
///
/// Supported tokens:
///   `{title}`, `{artist}`, `{album}`, `{album_artist}`, `{genre}`, `{year}`,
///   `{tracknum}`, `{composer}`, `{country}`, `{catalog}`, `{label}`, `{discogs_id}`
fn apply_path_template(template: &str, track: &TrackMeta) -> std::path::PathBuf {
    let s = apply_token_string(template, track);
    let tracknum = if track.track_number.is_empty() {
        "00".to_string()
    } else {
        track.track_number.parse::<u32>()
            .map(|n| format!("{:02}", n))
            .unwrap_or_else(|_| track.track_number.clone())
    };

    let mut path = std::path::PathBuf::new();
    for segment in s.split('/') {
        let seg = segment.trim();
        if seg.is_empty() { continue; }
        path.push(sanitize_filename(seg));
    }

    // Ensure we always have at least one segment
    if path.as_os_str().is_empty() {
        path.push(format!("{} - Unknown", tracknum));
    }

    path
}

pub async fn run_export_worker(
    tracks: Vec<TrackMeta>,
    pipe: Arc<Mutex<AudacityPipe>>,
    config: Config,
    tx: AppSender,
    ctx: egui::Context,
    cover_bytes: Option<Vec<u8>>,
) {
    let total = tracks.len();
    let _ = tx.send(WorkerMessage::Log(format!("Starting export of {} track(s)...", total)));

    // ---- Pre-export: verify Audacity label state --------------------------------
    {
        let pipe_chk = pipe.clone();
        let tracks_chk = tracks.clone();
        let tx_chk = tx.clone();
        let check = tokio::task::spawn_blocking(move || {
            let mut g = pipe_chk.lock().map_err(|e| anyhow::anyhow!("{}", e))?;
            g.get_labels()
        }).await;

        match check {
            Ok(Ok(labels)) => {
                let count_ok = labels.len() == tracks_chk.len();
                let mut title_issues: Vec<String> = Vec::new();
                for (i, track) in tracks_chk.iter().enumerate() {
                    match labels.get(i) {
                        Some((_, _, label_title)) if label_title != &track.title => {
                            title_issues.push(format!(
                                "  track {}: expected {:?}, Audacity has {:?}",
                                i + 1, track.title, label_title
                            ));
                        }
                        None => {
                            title_issues.push(format!("  track {}: no label in Audacity", i + 1));
                        }
                        _ => {}
                    }
                }

                if count_ok && title_issues.is_empty() {
                    let _ = tx_chk.send(WorkerMessage::Log(format!(
                        "Label check: {} label(s) confirmed, titles match.", labels.len()
                    )));
                } else {
                    let _ = tx_chk.send(WorkerMessage::Log(format!(
                        "Label check: Audacity has {} label(s), expected {}. {}",
                        labels.len(), tracks_chk.len(),
                        if title_issues.is_empty() { String::new() } else { "Title mismatches:".into() }
                    )));
                    for issue in title_issues {
                        let _ = tx_chk.send(WorkerMessage::Log(issue));
                    }
                }
            }
            Ok(Err(e)) => {
                let _ = tx.send(WorkerMessage::Log(format!("Label check failed: {}", e)));
            }
            Err(e) => {
                let _ = tx.send(WorkerMessage::Log(format!("Label check task error: {}", e)));
            }
        }
    }

    // ---- Export loop ------------------------------------------------------------
    let mut cover_written = false;

    for (i, track) in tracks.iter().enumerate() {
        let ext = config.export_format.extension();

        // Build output path from configurable template
        let rel = apply_path_template(&config.export_path_template, track);
        // Append extension to the final filename component
        let out_path = {
            let mut p = config.export_dir.join(&rel);
            let stem = p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "track".to_string());
            p.set_file_name(format!("{}.{}", stem, ext));
            p
        };

        // Create parent directories
        if let Some(parent) = out_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                let _ = tx.send(WorkerMessage::Log(format!(
                    "  Track {}: failed to create directory {:?}: {}",
                    track.index, parent, e
                )));
                let _ = tx.send(WorkerMessage::Progress { done: i + 1, total });
                ctx.request_repaint();
                continue;
            }
        }

        let _ = tx.send(WorkerMessage::Log(format!(
            "Exporting track {}: {}", track.track_number, track.title
        )));

        let pipe_clone    = pipe.clone();
        let t_start       = track.start;
        let t_end         = track.end;
        let out_path_clone = out_path.clone();

        let export_result = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let mut g = pipe_clone.lock()
                .map_err(|e| anyhow::anyhow!("Failed to lock pipe: {}", e))?;
            g.select_time(t_start, t_end)
                .map_err(|e| anyhow::anyhow!("SelectTime failed: {}", e))?;
            g.export_selection(&out_path_clone, 2)
                .map_err(|e| anyhow::anyhow!("Export failed: {}", e))?;
            Ok(())
        }).await;

        match export_result {
            Err(e) => {
                let _ = tx.send(WorkerMessage::Log(format!(
                    "  Track {}: export task error: {}", track.index, e
                )));
                let _ = tx.send(WorkerMessage::Progress { done: i + 1, total });
                ctx.request_repaint();
                continue;
            }
            Ok(Err(e)) => {
                let _ = tx.send(WorkerMessage::Log(format!(
                    "  Track {}: export failed: {}", track.index, e
                )));
                let _ = tx.send(WorkerMessage::Progress { done: i + 1, total });
                ctx.request_repaint();
                continue;
            }
            Ok(Ok(())) => {}
        }

        // Wait for Audacity to finish writing
        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

        if out_path.exists() {
            let effective_comments = if track.comments.is_empty() {
                &config.default_comments
            } else {
                &track.comments
            };
            // Apply album name format if configured; fall back to track.album.
            let tagged_track;
            let track_for_tags = if config.album_name_format.is_empty() {
                track
            } else {
                let formatted = apply_token_string(&config.album_name_format, track);
                tagged_track = TrackMeta { album: formatted, ..track.clone() };
                &tagged_track
            };
            if let Err(e) = write_tags(&out_path, track_for_tags, effective_comments, &config.custom_tags) {
                let _ = tx.send(WorkerMessage::Log(format!(
                    "  Track {}: tagging warning: {}", track.index, e
                )));
            }

            let _ = tx.send(WorkerMessage::Log(format!("  ✓ {}", out_path.display())));
            let _ = tx.send(WorkerMessage::TrackUpdate {
                index: track.index,
                updates: TrackUpdate {
                    export_path: Some(out_path.clone()),
                    ..Default::default()
                },
            });

            // Write folder.jpg alongside the first successfully exported track
            if !cover_written {
                if let Some(ref bytes) = cover_bytes {
                    if let Some(album_dir_path) = out_path.parent() {
                        let cover_path = album_dir_path.join("folder.jpg");
                        match std::fs::write(&cover_path, bytes) {
                            Ok(()) => {
                                let _ = tx.send(WorkerMessage::Log(format!(
                                    "  Cover art → {}", cover_path.display()
                                )));
                                cover_written = true;
                            }
                            Err(e) => {
                                let _ = tx.send(WorkerMessage::Log(format!(
                                    "  Cover art write failed: {}", e
                                )));
                            }
                        }
                    }
                }
            }
        } else {
            let _ = tx.send(WorkerMessage::Log(format!(
                "  Track {}: file not found after export: {}",
                track.index, out_path.display()
            )));
        }

        let _ = tx.send(WorkerMessage::Progress { done: i + 1, total });
        ctx.request_repaint();
    }

    let _ = tx.send(WorkerMessage::Log("Export complete.".into()));
    let _ = tx.send(WorkerMessage::WorkerFinished);
    ctx.request_repaint();
}
