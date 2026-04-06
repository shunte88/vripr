/*
 *  mod.rs
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
pub mod discogs;
pub mod genre;

pub use discogs::*;
pub use genre::{reload_genre_map, sanitize_genres};

use crate::config::TrackNumberFormat;
use crate::track::TrackMeta;

// ---------------------------------------------------------------------------
// Duration-based track utilities (vinylflow-inspired)
// ---------------------------------------------------------------------------

/// Assign Discogs track titles/numbers/album metadata to detected tracks in order.
/// Only assigns as many tracks as the shorter of the two slices.
pub fn assign_discogs_titles(
    tracks: &mut [TrackMeta],
    discogs_tracks: &[&DiscogsTrack],
    release: &DiscogsRelease,
) {
    for (i, track) in tracks.iter_mut().enumerate() {
        let Some(dt) = discogs_tracks.get(i) else { break };
        track.title              = dt.title.clone();
        track.track_number       = format!("{}{}", dt.side, dt.number);
        track.album              = release.album.clone();
        track.album_artist       = release.album_artist.clone();
        track.year               = release.year.clone();
        track.genre              = release.genre.clone();
        track.discogs_release_id = release.release_id.clone();
        track.country            = release.country.clone();
        track.catalog            = release.catalog.clone();
        track.label              = release.label.clone();
    }
}

/// Generate synthetic track boundaries from Discogs track durations.
///
/// `offset_secs` – where the first track starts (0.0 for fresh recordings).
/// `gap_secs` – silence gap inserted between consecutive synthetic tracks.
///
/// Tracks without a known duration are skipped.
#[allow(dead_code)]
pub fn split_by_discogs_durations(
    discogs_tracks: &[&DiscogsTrack],
    release: &DiscogsRelease,
    offset_secs: f64,
    gap_secs: f64,
) -> Vec<TrackMeta> {
    split_by_discogs_durations_fmt(discogs_tracks, release, offset_secs, gap_secs, &TrackNumberFormat::Alpha)
}

pub fn split_by_discogs_durations_fmt(
    discogs_tracks: &[&DiscogsTrack],
    release: &DiscogsRelease,
    offset_secs: f64,
    gap_secs: f64,
    fmt: &TrackNumberFormat,
) -> Vec<TrackMeta> {
    let mut tracks = Vec::new();
    let mut pos    = offset_secs;

    for (i, dt) in discogs_tracks.iter().enumerate() {
        let dur = match dt.duration_secs {
            Some(d) if d > 0.0 => d,
            _ => continue,
        };

        let track_number = match fmt {
            TrackNumberFormat::Alpha   => format!("{}{}", dt.side, dt.number),
            TrackNumberFormat::Numeric => (i + 1).to_string(),
        };
        let track = TrackMeta {
            index:           i + 1,
            start:           pos,
            end:             pos + dur,
            title:           dt.title.clone(),
            track_number,
            album:           release.album.clone(),
            album_artist:    release.album_artist.clone(),
            artist:          release.album_artist.clone(),
            year:            release.year.clone(),
            genre:           release.genre.clone(),
            discogs_release_id: release.release_id.clone(),
            country:         release.country.clone(),
            catalog:         release.catalog.clone(),
            label:           release.label.clone(),
            ..Default::default()
        };
        pos += dur + gap_secs;
        tracks.push(track);
    }

    tracks
}

/// Create placeholder TrackMeta entries from Discogs title-only data (no durations).
/// All tracks get start=0/end=0 — the user can set times manually in the edit panel.
pub fn title_only_tracks(
    discogs_tracks: &[&DiscogsTrack],
    release: &DiscogsRelease,
    fmt: &TrackNumberFormat,
) -> Vec<TrackMeta> {
    discogs_tracks.iter().enumerate().map(|(i, dt)| {
        let track_number = match fmt {
            TrackNumberFormat::Alpha   => format!("{}{}", dt.side, dt.number),
            TrackNumberFormat::Numeric => (i + 1).to_string(),
        };
        TrackMeta {
            index:              i + 1,
            start:              0.0,
            end:                0.0,
            title:              dt.title.clone(),
            track_number,
            album:              release.album.clone(),
            album_artist:       release.album_artist.clone(),
            artist:             release.album_artist.clone(),
            year:               release.year.clone(),
            genre:              release.genre.clone(),
            discogs_release_id: release.release_id.clone(),
            country:            release.country.clone(),
            catalog:            release.catalog.clone(),
            label:              release.label.clone(),
            ..Default::default()
        }
    }).collect()
}

/// Compare the duration of each detected track against each Discogs track.
///
/// Returns a human-readable comparison string for logging, plus a best-guess
/// count match (detected count == discogs count).
pub fn compare_duration_report(
    detected: &[TrackMeta],
    discogs_tracks: &[&DiscogsTrack],
    tolerance_secs: f64,
) -> (String, bool) {
    if discogs_tracks.is_empty() || detected.is_empty() {
        return ("No tracks to compare.".into(), false);
    }

    let count_match = detected.len() == discogs_tracks.len();
    let mut lines   = Vec::new();

    lines.push(format!(
        "Duration comparison: {} detected vs {} Discogs tracks (tolerance ±{:.1}s)",
        detected.len(), discogs_tracks.len(), tolerance_secs
    ));

    let pairs = detected.len().min(discogs_tracks.len());
    for i in 0..pairs {
        let det_dur = detected[i].end - detected[i].start;
        let disc_dur = discogs_tracks[i].duration_secs.unwrap_or(0.0);
        let delta    = det_dur - disc_dur;
        let ok       = delta.abs() <= tolerance_secs;
        lines.push(format!(
            "  Track {}: detected {:.1}s, Discogs {:.1}s, Δ{:+.1}s {}",
            i + 1, det_dur, disc_dur, delta,
            if ok { "✓" } else { "⚠" }
        ));
    }

    if !count_match {
        lines.push(format!(
            "  ⚠ Count mismatch — consider using 'Split by Durations' to regenerate boundaries"
        ));
    }

    (lines.join("\n"), count_match)
}
