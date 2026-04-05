use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

use crate::config::Config;
use crate::metadata::{discogs_search, fingerprint_file, mb_lookup_recording, mb_search, merge_metadata};
use crate::pipe::AudacityPipe;
use crate::track::TrackMeta;
use crate::workers::{AppSender, TrackUpdate, WorkerMessage};

pub async fn run_fingerprint_worker(
    tracks: Vec<TrackMeta>,
    pipe: Arc<Mutex<AudacityPipe>>,
    config: Config,
    tx: AppSender,
    ctx: egui::Context,
) {
    let total = tracks.len();
    let _ = tx.send(WorkerMessage::Log(format!("Starting fingerprint of {} track(s)...", total)));

    for (i, track) in tracks.iter().enumerate() {
        let _ = tx.send(WorkerMessage::Log(format!(
            "Fingerprinting track {}...",
            track.index
        )));

        // Create temp file path
        let fmt = config.export_format.extension();
        let tmp_path = std::env::temp_dir().join(format!(
            "vripr_tmp_{}.{}",
            track.index, fmt
        ));

        // Export the track selection to temp file
        let pipe_clone = pipe.clone();
        let t_start = track.start;
        let t_end = track.end;
        let tmp_path_clone = tmp_path.clone();

        let export_result = tokio::task::spawn_blocking(move || {
            let mut pipe_guard = match pipe_clone.lock() {
                Ok(g) => g,
                Err(e) => return Err(anyhow::anyhow!("Failed to lock pipe: {}", e)),
            };

            pipe_guard.select_time(t_start, t_end)
                .map_err(|e| anyhow::anyhow!("SelectTime failed: {}", e))?;

            pipe_guard.export_selection(&tmp_path_clone, 2)
                .map_err(|e| anyhow::anyhow!("Export failed: {}", e))?;

            Ok(())
        })
        .await;

        let export_ok = match export_result {
            Ok(Ok(())) => true,
            Ok(Err(e)) => {
                let _ = tx.send(WorkerMessage::Log(format!(
                    "  Track {}: export failed: {}",
                    track.index, e
                )));
                false
            }
            Err(e) => {
                let _ = tx.send(WorkerMessage::Log(format!(
                    "  Track {}: export task error: {}",
                    track.index, e
                )));
                false
            }
        };

        if !export_ok {
            let _ = tx.send(WorkerMessage::Progress { done: i + 1, total });
            ctx.request_repaint();
            continue;
        }

        // Wait a bit for Audacity to finish writing
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        if !tmp_path.exists() {
            let _ = tx.send(WorkerMessage::Log(format!(
                "  Track {}: temp file not found after export",
                track.index
            )));
            let _ = tx.send(WorkerMessage::Progress { done: i + 1, total });
            ctx.request_repaint();
            continue;
        }

        // Fingerprint
        let mut updates = TrackUpdate::default();
        let api_key = config.acoustid_api_key.clone();

        match fingerprint_file(&tmp_path, &api_key).await {
            Ok(Some(acoustid_match)) => {
                let _ = tx.send(WorkerMessage::Log(format!(
                    "  Track {}: AcoustID score={:.2} → {} / {}",
                    track.index, acoustid_match.score, acoustid_match.title, acoustid_match.artist
                )));

                updates.acoustid = Some(acoustid_match.recording_id.clone());
                updates.fingerprint_done = Some(true);

                if !acoustid_match.title.is_empty() && track.title.is_empty() {
                    updates.title = Some(acoustid_match.title.clone());
                }
                if !acoustid_match.artist.is_empty() && track.artist.is_empty() {
                    updates.artist = Some(acoustid_match.artist.clone());
                }

                // MusicBrainz lookup
                let mb_result = mb_lookup_recording(
                    &acoustid_match.recording_id,
                    &config.mb_user_agent,
                )
                .await;

                let mb_meta = match mb_result {
                    Ok(Some(m)) => {
                        let _ = tx.send(WorkerMessage::Log(format!(
                            "    MB: {} / {} / {}",
                            m.title, m.artist, m.album
                        )));
                        Some(m)
                    }
                    Ok(None) => {
                        // Try search fallback
                        let search_title = updates.title.clone().unwrap_or_else(|| track.title.clone());
                        let search_artist = updates.artist.clone().unwrap_or_else(|| track.artist.clone());
                        if !search_title.is_empty() {
                            match mb_search(&search_title, &search_artist, &config.mb_user_agent).await {
                                Ok(m) => m,
                                Err(e) => {
                                    warn!("MB search failed: {}", e);
                                    None
                                }
                            }
                        } else {
                            None
                        }
                    }
                    Err(e) => {
                        warn!("MB lookup failed: {}", e);
                        None
                    }
                };

                // Merge MB metadata
                if let Some(ref m) = mb_meta {
                    updates.mb_recording_id = Some(m.recording_id.clone());
                    if updates.title.is_none() && !m.title.is_empty() && track.title.is_empty() {
                        updates.title = Some(m.title.clone());
                    }
                    if updates.artist.is_none() && !m.artist.is_empty() && track.artist.is_empty() {
                        updates.artist = Some(m.artist.clone());
                    }
                    if !m.album.is_empty() && track.album.is_empty() {
                        updates.album = Some(m.album.clone());
                    }
                    if !m.year.is_empty() && track.year.is_empty() {
                        updates.year = Some(m.year.clone());
                    }
                    if !m.track_number.is_empty() && track.track_number.is_empty() {
                        updates.track_number = Some(m.track_number.clone());
                    }
                    if !m.genre.is_empty() && track.genre.is_empty() {
                        updates.genre = Some(m.genre.clone());
                    }
                }

                // Discogs search for album metadata
                let eff_artist = updates.artist.clone().unwrap_or_else(|| track.artist.clone());
                let eff_album = updates.album.clone().unwrap_or_else(|| track.album.clone());

                if !eff_artist.is_empty() && !eff_album.is_empty() {
                    match discogs_search(&eff_artist, &eff_album, &config.discogs_token).await {
                        Ok(Some(d)) => {
                            let _ = tx.send(WorkerMessage::Log(format!(
                                "    Discogs: album_artist={}  genre={}",
                                d.album_artist, d.genre
                            )));
                            updates.discogs_release_id = Some(d.release_id.clone());
                            if updates.album_artist.is_none() && !d.album_artist.is_empty() && track.album_artist.is_empty() {
                                updates.album_artist = Some(d.album_artist);
                            }
                            if updates.genre.is_none() && !d.genre.is_empty() && track.genre.is_empty() {
                                updates.genre = Some(d.genre);
                            }
                            if updates.year.is_none() && !d.year.is_empty() && track.year.is_empty() {
                                updates.year = Some(d.year);
                            }
                        }
                        Ok(None) => {}
                        Err(e) => warn!("Discogs search failed: {}", e),
                    }
                }
            }
            Ok(None) => {
                let _ = tx.send(WorkerMessage::Log(format!(
                    "  Track {}: no AcoustID match",
                    track.index
                )));
                updates.fingerprint_done = Some(false);
            }
            Err(e) => {
                let _ = tx.send(WorkerMessage::Log(format!(
                    "  Track {}: fingerprint error: {}",
                    track.index, e
                )));
            }
        }

        // Clean up temp file
        if tmp_path.exists() {
            if let Err(e) = std::fs::remove_file(&tmp_path) {
                warn!("Failed to remove temp file {:?}: {}", tmp_path, e);
            }
        }

        let _ = tx.send(WorkerMessage::TrackUpdate {
            index: track.index,
            updates,
        });
        let _ = tx.send(WorkerMessage::Progress { done: i + 1, total });
        ctx.request_repaint();
    }

    let _ = tx.send(WorkerMessage::Log("Fingerprinting complete.".into()));
    let _ = tx.send(WorkerMessage::WorkerFinished);
    ctx.request_repaint();
}
