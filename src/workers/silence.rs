use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

use crate::config::Config;
use crate::pipe::AudacityPipe;
use crate::track::TrackMeta;
use crate::workers::{AppSender, WorkerMessage};

pub async fn run_silence_worker(
    pipe: Arc<Mutex<AudacityPipe>>,
    config: Config,
    tx: AppSender,
    ctx: egui::Context,
) {
    let _ = tx.send(WorkerMessage::Log("Detecting silence via LabelSounds...".into()));

    let pipe_clone = pipe.clone();
    let config_clone = config.clone();

    let result = tokio::task::spawn_blocking(move || {
        let mut pipe_guard = match pipe_clone.lock() {
            Ok(g) => g,
            Err(e) => return Err(anyhow::anyhow!("Failed to lock pipe: {}", e)),
        };

        // Send LabelSounds command
        pipe_guard.label_sounds(&config_clone)
            .map_err(|e| anyhow::anyhow!("LabelSounds failed: {}", e))?;

        // Wait for Audacity to process
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Give Audacity a moment to finish writing the label track.
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Get the labels that were created
        let labels = pipe_guard.get_labels()
            .map_err(|e| anyhow::anyhow!("Failed to get labels: {}", e))?;

        Ok(labels)
    })
    .await;

    match result {
        Ok(Ok(labels)) => {
            let _ = tx.send(WorkerMessage::Log(format!(
                "Audacity returned {} label(s) from LabelSounds.", labels.len()
            )));
            debug!("Got {} labels from LabelSounds", labels.len());

            if labels.is_empty() {
                let _ = tx.send(WorkerMessage::WorkerError(
                    "No labels found after LabelSounds. Try adjusting the silence threshold.".into()
                ));
                ctx.request_repaint();
                return;
            }

            let tracks: Vec<TrackMeta> = labels
                .into_iter()
                .enumerate()
                .map(|(i, (start, end, _label))| TrackMeta {
                    index: i + 1,
                    start,
                    end,
                    track_number: (i + 1).to_string(),
                    artist: config.default_artist.clone(),
                    album: config.default_album.clone(),
                    album_artist: config.default_album_artist.clone(),
                    genre: config.default_genre.clone(),
                    year: config.default_year.clone(),
                    ..Default::default()
                })
                .collect();

            let count = tracks.len();
            let _ = tx.send(WorkerMessage::TracksDetected(tracks));
            let _ = tx.send(WorkerMessage::Log(format!("Found {} track(s).", count)));
            let _ = tx.send(WorkerMessage::WorkerFinished);
        }
        Ok(Err(e)) => {
            warn!("Silence detection error: {}", e);
            let _ = tx.send(WorkerMessage::WorkerError(format!("Silence detection failed: {}", e)));
        }
        Err(e) => {
            warn!("Silence worker task panicked: {}", e);
            let _ = tx.send(WorkerMessage::WorkerError(format!("Worker task error: {}", e)));
        }
    }

    ctx.request_repaint();
}

/// Import labels directly from Audacity without running LabelSounds first.
pub async fn run_import_labels_worker(
    pipe: Arc<Mutex<AudacityPipe>>,
    config: Config,
    tx: AppSender,
    ctx: egui::Context,
) {
    let _ = tx.send(WorkerMessage::Log("Importing labels from Audacity...".into()));

    let pipe_clone = pipe.clone();

    let result = tokio::task::spawn_blocking(move || {
        let mut pipe_guard = match pipe_clone.lock() {
            Ok(g) => g,
            Err(e) => return Err(anyhow::anyhow!("Failed to lock pipe: {}", e)),
        };

        let labels = pipe_guard.get_labels()
            .map_err(|e| anyhow::anyhow!("Failed to get labels: {}", e))?;

        Ok(labels)
    })
    .await;

    match result {
        Ok(Ok(labels)) => {
            if labels.is_empty() {
                let _ = tx.send(WorkerMessage::WorkerError(
                    "No labels found in Audacity. Place labels manually then click Import Labels.".into()
                ));
                ctx.request_repaint();
                return;
            }

            let tracks: Vec<TrackMeta> = labels
                .into_iter()
                .enumerate()
                .map(|(i, (start, end, label))| {
                    let title = if label.is_empty() {
                        format!("Track {}", i + 1)
                    } else {
                        label
                    };
                    TrackMeta {
                        index: i + 1,
                        start,
                        end,
                        title,
                        track_number: (i + 1).to_string(),
                        artist: config.default_artist.clone(),
                        album: config.default_album.clone(),
                        album_artist: config.default_album_artist.clone(),
                        genre: config.default_genre.clone(),
                        year: config.default_year.clone(),
                        ..Default::default()
                    }
                })
                .collect();

            let count = tracks.len();
            let _ = tx.send(WorkerMessage::TracksDetected(tracks));
            let _ = tx.send(WorkerMessage::Log(format!("Imported {} track(s) from labels.", count)));
            let _ = tx.send(WorkerMessage::WorkerFinished);
        }
        Ok(Err(e)) => {
            let _ = tx.send(WorkerMessage::WorkerError(format!("Import labels failed: {}", e)));
        }
        Err(e) => {
            let _ = tx.send(WorkerMessage::WorkerError(format!("Worker task error: {}", e)));
        }
    }

    ctx.request_repaint();
}
