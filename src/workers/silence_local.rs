use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::audio::{DetectorConfig, detect_tracks};
use crate::config::Config;
use crate::pipe::AudacityPipe;
use crate::track::TrackMeta;
use crate::workers::{AppSender, WorkerMessage};

/// Run our own silence detector on the audio file directly.
///
/// Does not require Audacity to run the analysis — the pipe is only used
/// to read back the audio file path and optionally push labels afterwards.
pub async fn run_local_silence_worker(
    pipe: Arc<Mutex<AudacityPipe>>,
    config: Config,
    tx: AppSender,
    ctx: egui::Context,
) {
    let _ = tx.send(WorkerMessage::Log(
        "Local silence detection: locating audio file...".into(),
    ));

    // --- Step 1: resolve the audio file path ---
    let audio_path = {
        // Try Audacity first
        let from_pipe = tokio::task::spawn_blocking({
            let pipe = pipe.clone();
            move || {
                let mut g = pipe.lock().map_err(|e| anyhow::anyhow!("{}", e))?;
                g.get_audio_file_path()
            }
        })
        .await;

        match from_pipe {
            Ok(Ok(Some(p))) => {
                let _ = tx.send(WorkerMessage::Log(format!(
                    "Audio file from Audacity: {}",
                    p.display()
                )));
                p
            }
            _ => {
                // Fall back to config
                if !config.audio_file.is_empty() {
                    let p = PathBuf::from(&config.audio_file);
                    let _ = tx.send(WorkerMessage::Log(format!(
                        "Audio file from config: {}",
                        p.display()
                    )));
                    p
                } else {
                    let _ = tx.send(WorkerMessage::WorkerError(
                        "Cannot find audio file path. \
                         Open the file in Audacity first, or set it in Settings → Defaults → Audio File."
                            .into(),
                    ));
                    ctx.request_repaint();
                    return;
                }
            }
        }
    };

    if !audio_path.exists() {
        let _ = tx.send(WorkerMessage::WorkerError(format!(
            "Audio file not found: {}",
            audio_path.display()
        )));
        ctx.request_repaint();
        return;
    }

    // --- Step 2: build detector config from app config ---
    let det_cfg = DetectorConfig {
        threshold_db:       config.silence_threshold_db,
        adaptive:           config.use_adaptive_threshold,
        adaptive_margin_db: config.adaptive_margin_db,
        min_silence_secs:   config.silence_min_duration,
        min_sound_secs:     config.silence_min_sound_dur,
        ..DetectorConfig::default()
    };

    let _ = tx.send(WorkerMessage::Log(format!(
        "Detector: threshold={} dB, adaptive={}, min_silence={:.1}s, min_sound={:.1}s",
        det_cfg.threshold_db,
        det_cfg.adaptive,
        det_cfg.min_silence_secs,
        det_cfg.min_sound_secs,
    )));

    // --- Step 3: run detection in blocking thread (reads audio file) ---
    let tx2 = tx.clone();
    let ctx2 = ctx.clone();

    let result = tokio::task::spawn_blocking(move || {
        let mut last_pct = 0u32;
        let mut progress_cb = |pct: f64| {
            let p = (pct * 100.0) as u32;
            if p >= last_pct + 5 {
                last_pct = p;
                let _ = tx2.send(WorkerMessage::Log(format!("  decoding... {}%", p)));
                ctx2.request_repaint();
            }
        };
        detect_tracks(&audio_path, &det_cfg, &mut progress_cb)
    })
    .await;

    match result {
        Ok(Ok((tracks, diag))) => {
            if let Some(nf) = diag.noise_floor_db {
                let _ = tx.send(WorkerMessage::Log(format!(
                    "Noise floor: {:.1} dB → threshold: {:.1} dB",
                    nf, diag.threshold_db
                )));
            }
            let _ = tx.send(WorkerMessage::Log(format!(
                "Local detection: found {} track(s) in {:.1}s of audio ({} windows × {:.0}ms)",
                tracks.len(),
                diag.total_secs,
                diag.n_windows,
                diag.window_secs * 1000.0,
            )));

            if tracks.is_empty() {
                let _ = tx.send(WorkerMessage::WorkerError(
                    "No tracks detected. Try:\n\
                     • Lowering the threshold (e.g. -35 dB)\n\
                     • Enabling Adaptive threshold in Settings\n\
                     • Reducing Min Silence duration"
                        .into(),
                ));
                ctx.request_repaint();
                return;
            }

            let track_metas: Vec<TrackMeta> = tracks
                .into_iter()
                .enumerate()
                .map(|(i, t)| TrackMeta {
                    index:        i + 1,
                    start:        t.start,
                    end:          t.end,
                    track_number: (i + 1).to_string(),
                    artist:       config.default_artist.clone(),
                    album:        config.default_album.clone(),
                    album_artist: config.default_album_artist.clone(),
                    genre:        config.default_genre.clone(),
                    year:         config.default_year.clone(),
                    ..Default::default()
                })
                .collect();

            let _ = tx.send(WorkerMessage::TracksDetected(track_metas));
            let _ = tx.send(WorkerMessage::WorkerFinished);
        }
        Ok(Err(e)) => {
            let _ = tx.send(WorkerMessage::WorkerError(format!(
                "Detection failed: {}",
                e
            )));
        }
        Err(e) => {
            let _ = tx.send(WorkerMessage::WorkerError(format!(
                "Worker panic: {}",
                e
            )));
        }
    }

    ctx.request_repaint();
}
