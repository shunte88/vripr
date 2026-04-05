use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use egui::Context;

use crate::audio::{detect_tracks_guided, GuidedDetectorConfig};
use crate::config::Config;
use crate::metadata::DiscogsRelease;
use crate::pipe::AudacityPipe;
use crate::track::TrackMeta;
use crate::workers::{AppSender, WorkerMessage};

pub async fn run_guided_detection_worker(
    pipe: Arc<Mutex<AudacityPipe>>,
    config: Config,
    release: DiscogsRelease,
    tx: AppSender,
    ctx: Context,
) {
    let _ = tx.send(WorkerMessage::Log(
        "Guided detection: locating audio file...".into(),
    ));

    // Resolve audio file path: prefer config, fall back to Audacity pipe
    let audio_path: Option<PathBuf> = {
        let from_config = config.audio_file.trim().to_string();
        if !from_config.is_empty() {
            Some(PathBuf::from(from_config))
        } else {
            // Ask Audacity for the currently-open file
            tokio::task::spawn_blocking(move || {
                pipe.lock()
                    .ok()
                    .and_then(|mut p| p.get_audio_file_path().ok().flatten())
                    .map(PathBuf::from)
            })
            .await
            .unwrap_or(None)
        }
    };

    let path = match audio_path {
        Some(p) => p,
        None => {
            let _ = tx.send(WorkerMessage::WorkerError(
                "No audio file path. Set it in Settings → Audio File Path, \
                 or open the file in Audacity first."
                    .into(),
            ));
            ctx.request_repaint();
            return;
        }
    };

    if !path.exists() {
        let _ = tx.send(WorkerMessage::WorkerError(format!(
            "Audio file not found: {}",
            path.display()
        )));
        ctx.request_repaint();
        return;
    }

    // Collect durations from the release (all sides, in order)
    // Tracks without a known duration are represented as 0 and skipped by the detector
    let durations: Vec<f64> = release
        .tracks
        .iter()
        .map(|t| t.duration_secs.unwrap_or(0.0))
        .collect();

    let valid = durations.iter().filter(|&&d| d > 0.0).count();
    if valid == 0 {
        let _ = tx.send(WorkerMessage::WorkerError(
            "Discogs release has no track durations — cannot perform guided detection.".into(),
        ));
        ctx.request_repaint();
        return;
    }

    let total_dur: f64 = durations.iter().sum();
    let _ = tx.send(WorkerMessage::Log(format!(
        "Guided detection: {} tracks with known durations, ~{:.0}s total",
        valid, total_dur
    )));
    let _ = tx.send(WorkerMessage::Log(format!("Analyzing: {}", path.display())));

    let cfg = GuidedDetectorConfig {
        threshold_db:       config.silence_threshold_db,
        adaptive:           config.use_adaptive_threshold,
        adaptive_margin_db: config.adaptive_margin_db,
        ..GuidedDetectorConfig::default()
    };

    let tx2 = tx.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut last_pct = 0u32;
        detect_tracks_guided(&path, &durations, &cfg, &mut |p| {
            let pct = (p * 100.0) as u32;
            if pct != last_pct {
                last_pct = pct;
                let _ = tx2.send(WorkerMessage::Progress {
                    done:  pct as usize,
                    total: 100,
                });
            }
        })
    })
    .await;

    match result {
        Ok(Ok(detected)) if detected.is_empty() => {
            let _ = tx.send(WorkerMessage::Log(
                "Guided detection: no tracks found in audio.".into(),
            ));
            let _ = tx.send(WorkerMessage::WorkerFinished);
        }
        Ok(Ok(detected)) => {
            let _ = tx.send(WorkerMessage::Log(format!(
                "Guided detection found {} track boundary/ies.",
                detected.len()
            )));

            let disc_tracks: Vec<&crate::metadata::DiscogsTrack> =
                release.tracks.iter().collect();

            let tracks: Vec<TrackMeta> = detected
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    let dt = disc_tracks.get(i);
                    TrackMeta {
                        index:              i + 1,
                        start:              d.start,
                        end:                d.end,
                        title:              dt.map(|t| t.title.clone()).unwrap_or_default(),
                        track_number:       dt.map(|t| format!("{}{}", t.side, t.number))
                                              .unwrap_or_else(|| (i + 1).to_string()),
                        album:              release.album.clone(),
                        album_artist:       release.album_artist.clone(),
                        artist:             release.album_artist.clone(),
                        year:               release.year.clone(),
                        genre:              release.genre.clone(),
                        discogs_release_id: release.release_id.clone(),
                        ..Default::default()
                    }
                })
                .collect();

            let _ = tx.send(WorkerMessage::TracksDetected(tracks));
            let _ = tx.send(WorkerMessage::WorkerFinished);
        }
        Ok(Err(e)) => {
            let _ = tx.send(WorkerMessage::WorkerError(format!(
                "Guided detection error: {}",
                e
            )));
        }
        Err(e) => {
            let _ = tx.send(WorkerMessage::WorkerError(format!("Task error: {}", e)));
        }
    }

    ctx.request_repaint();
}
