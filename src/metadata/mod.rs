pub mod acoustid;
pub mod discogs;
pub mod musicbrainz;

pub use acoustid::*;
pub use discogs::*;
pub use musicbrainz::*;

use crate::config::TrackNumberFormat;
use crate::track::TrackMeta;

#[derive(Debug, Clone, Default)]
pub struct MetadataResult {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub track_number: Option<String>,
    pub year: Option<String>,
    pub acoustid: Option<String>,
    pub mb_recording_id: Option<String>,
    pub discogs_release_id: Option<String>,
}

/// Merge AcoustID + MusicBrainz + Discogs results into a single MetadataResult.
/// Priority: AcoustID title/artist > MusicBrainz > Discogs
pub fn merge_metadata(
    acoustid_match: Option<&AcoustidMatch>,
    mb: Option<&MbMetadata>,
    discogs: Option<&DiscogsMetadata>,
) -> MetadataResult {
    let mut result = MetadataResult::default();

    // AcoustID: recording_id
    if let Some(a) = acoustid_match {
        result.acoustid = Some(a.recording_id.clone());

        if !a.title.is_empty() {
            result.title = Some(a.title.clone());
        }
        if !a.artist.is_empty() {
            result.artist = Some(a.artist.clone());
        }
    }

    // MusicBrainz: fills in everything
    if let Some(m) = mb {
        result.mb_recording_id = Some(m.recording_id.clone());

        if result.title.is_none() && !m.title.is_empty() {
            result.title = Some(m.title.clone());
        }
        if result.artist.is_none() && !m.artist.is_empty() {
            result.artist = Some(m.artist.clone());
        }
        if result.album.is_none() && !m.album.is_empty() {
            result.album = Some(m.album.clone());
        }
        if result.year.is_none() && !m.year.is_empty() {
            result.year = Some(m.year.clone());
        }
        if result.track_number.is_none() && !m.track_number.is_empty() {
            result.track_number = Some(m.track_number.clone());
        }
        if result.genre.is_none() && !m.genre.is_empty() {
            result.genre = Some(m.genre.clone());
        }
    }

    // Discogs: fills in remaining album-level info
    if let Some(d) = discogs {
        result.discogs_release_id = Some(d.release_id.clone());

        if result.album.is_none() && !d.album.is_empty() {
            result.album = Some(d.album.clone());
        }
        if result.album_artist.is_none() && !d.album_artist.is_empty() {
            result.album_artist = Some(d.album_artist.clone());
        }
        if result.year.is_none() && !d.year.is_empty() {
            result.year = Some(d.year.clone());
        }
        if result.genre.is_none() && !d.genre.is_empty() {
            result.genre = Some(d.genre.clone());
        }
    }

    result
}

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
    }
}

/// Generate synthetic track boundaries from Discogs track durations.
///
/// `offset_secs` – where the first track starts (0.0 for fresh recordings).
/// `gap_secs` – silence gap inserted between consecutive synthetic tracks.
///
/// Tracks without a known duration are skipped.
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
            ..Default::default()
        };
        pos += dur + gap_secs;
        tracks.push(track);
    }

    tracks
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
