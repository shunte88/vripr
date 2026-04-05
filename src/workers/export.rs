use std::sync::{Arc, Mutex};

use crate::config::Config;
use crate::pipe::AudacityPipe;
use crate::tagging::write_tags;
use crate::track::TrackMeta;
use crate::workers::{AppSender, TrackUpdate, WorkerMessage};

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

        // Build output path: {export_dir}/{artist}/{album}/{NN} - {title}.{ext}
        let artist_dir  = sanitize_filename(&track.artist);
        let album_dir   = sanitize_filename(&track.album);
        let num = if track.track_number.is_empty() {
            "00".to_string()
        } else {
            track.track_number.parse::<u32>()
                .map(|n| format!("{:02}", n))
                .unwrap_or_else(|_| track.track_number.clone())
        };
        let title_safe = sanitize_filename(&track.title);
        let filename   = format!("{} - {}.{}", num, title_safe, ext);

        let out_path = config.export_dir
            .join(&artist_dir)
            .join(&album_dir)
            .join(&filename);

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
            if let Err(e) = write_tags(&out_path, track) {
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
