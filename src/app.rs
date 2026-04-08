/*
 *  app.rs
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
use std::collections::HashSet;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use std::path::PathBuf;

use crate::audio::{
    detect_tracks, detect_tracks_guided, detect_tracks_hmm, detect_tracks_spectral,
    detect_tracks_onnx, OnnxDetectorConfig,
    DetectorConfig, GuidedDetectorConfig,
};
use crate::config::{Config, DetectionMethod};
use crate::metadata::{
    assign_discogs_titles, compare_duration_report, discogs_encode_query,
    title_only_tracks,
    discogs_fetch_image, discogs_fetch_release, discogs_search_candidates, discogs_search_by_catno,
    split_by_discogs_durations_fmt,
    DiscogsCandidate, DiscogsRelease,
};
use crate::pipe::AudacityPipe;
use crate::track::TrackMeta;
use crate::metadata::reload_genre_map;
use crate::ui::{
    ManualTrackInput, TableAction, ToolbarAction, ToolbarState,
    show_discogs_picker, show_manual_track_dialog, show_settings_dialog,
    show_toolbar, show_track_table, show_apply_all_strip,
};
use crate::workers::{TrackUpdate, WorkerMessage};
use crate::workers::export::run_export_worker;

pub struct VriprApp {
    pub config: Config,
    pub tracks: Vec<TrackMeta>,
    pub pipe: Arc<Mutex<AudacityPipe>>,
    pub pipe_connected: bool,
    pub is_busy: bool,
    pub log_messages: Vec<String>,
    log_file: Option<std::fs::File>,
    pub selected_rows: HashSet<usize>,

    // Worker channel
    pub worker_tx: mpsc::Sender<WorkerMessage>,
    pub worker_rx: mpsc::Receiver<WorkerMessage>,

    // Tokio runtime
    pub rt: Arc<tokio::runtime::Runtime>,

    // UI dialog state
    pub settings_open: bool,
    pub manual_track_open: bool,
    pub manual_track_input: ManualTrackInput,

    // Apply-to-all strip state
    apply_artist: String,
    apply_album: String,
    apply_album_artist: String,
    apply_catalog: String,
    apply_genre: String,
    apply_year: String,

    // Progress
    pub progress: Option<(usize, usize)>,

    // Fetched Discogs release (for duration-based splitting)
    pub discogs_release: Option<DiscogsRelease>,
    /// Vinyl sides present in the current release (e.g. ['A','B']). Empty = no release.
    pub available_sides: Vec<char>,
    /// Filter processing to a single vinyl side. None = all sides (normal full-album workflow).
    pub selected_side: Option<char>,

    // Discogs candidate picker
    pub discogs_candidates: Vec<DiscogsCandidate>,
    pub discogs_picker_open: bool,
    discogs_picker_token: String,
    /// When true, auto-accept the sole candidate on the next update() tick.
    discogs_auto_accept: bool,

    // Cover art
    cover_texture: Option<egui::TextureHandle>,
    cover_image_bytes: Option<Vec<u8>>,   // raw bytes kept for folder.jpg export
    pending_cover_bytes: Option<Vec<u8>>, // queued for texture creation on main thread
    custom_cover_path: String,

    /// Index of the track currently open in the edit panel. None = closed.
    editing_track_index: Option<usize>,

    // Waveform display
    waveform_samples: Option<Vec<f32>>,
    waveform_duration: f64,
    waveform_drag: Option<crate::ui::WaveformDragState>,
    waveform_selection: Option<(f64, f64)>, // active selection band in seconds
    /// When Some, Audacity is playing; holds the expected finish time for the indicator.
    play_end: Option<std::time::Instant>,
    /// Path to the WAV exported from Audacity on connect — used for detection.
    pub analysis_wav: Option<std::path::PathBuf>,
}

impl VriprApp {
    pub fn new(cc: &eframe::CreationContext, rt: Arc<tokio::runtime::Runtime>) -> Self {
        // Catppuccin Mocha dark theme
        cc.egui_ctx.set_visuals(egui::Visuals {
            dark_mode: true,
            override_text_color: Some(egui::Color32::from_rgb(205, 214, 244)),
            panel_fill: egui::Color32::from_rgb(30, 30, 46),
            window_fill: egui::Color32::from_rgb(30, 30, 46),
            extreme_bg_color: egui::Color32::from_rgb(17, 17, 27),
            faint_bg_color: egui::Color32::from_rgb(24, 24, 37),
            code_bg_color: egui::Color32::from_rgb(49, 50, 68),
            window_stroke: egui::Stroke::new(1.0, egui::Color32::from_rgb(49, 50, 68)),
            ..egui::Visuals::dark()
        });

        let config = Config::load();

        // Load system Unicode fonts so Discogs metadata with CJK, Devanagari,
        // Arabic, Hebrew, Thai, etc. renders correctly.
        crate::fonts::setup_unicode_fonts(
            &cc.egui_ctx,
            Some(&config.extra_ui_font),
        );

        // Apply custom genre map from persisted config, if set.
        {
            let custom = config.custom_genre_dat.trim();
            if !custom.is_empty() {
                reload_genre_map(Some(std::path::Path::new(custom)));
            }
        }
        let (worker_tx, worker_rx) = mpsc::channel();

        // Open (or create + append) a log file alongside the config.
        let log_file = crate::config::config_path()
            .parent()
            .map(|dir| dir.join("vripr.log"))
            .and_then(|path| {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .ok()
            });

        let mut app = VriprApp {
            config,
            tracks: Vec::new(),
            pipe: Arc::new(Mutex::new(AudacityPipe::new())),
            pipe_connected: false,
            is_busy: false,
            log_messages: Vec::new(),
            log_file,
            selected_rows: HashSet::new(),
            worker_tx,
            worker_rx,
            rt,
            settings_open: false,
            manual_track_open: false,
            manual_track_input: ManualTrackInput::default(),
            apply_artist: String::new(),
            apply_album: String::new(),
            apply_album_artist: String::new(),
            apply_catalog: String::new(),
            apply_genre: String::new(),
            apply_year: String::new(),
            progress: None,
            discogs_release: None,
            available_sides: Vec::new(),
            selected_side: None,
            discogs_candidates: Vec::new(),
            discogs_picker_open: false,
            discogs_auto_accept: false,
            discogs_picker_token: String::new(),
            cover_texture: None,
            cover_image_bytes: None,
            pending_cover_bytes: None,
            custom_cover_path: String::new(),
            editing_track_index: None,
            waveform_samples: None,
            waveform_duration: 0.0,
            waveform_drag: None,
            waveform_selection: None,
            play_end: None,
            analysis_wav: None,
        };

        // Write session separator + startup message to log file and panel.
        {
            use std::io::Write;
            if let Some(ref mut f) = app.log_file {
                let _ = writeln!(f, "\n--- session {} ---",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
            }
        }
        app.push_log(format!(
            "VRipr — Master Vinyl Rippage v{} ({}) ready.",
            crate::build_info::VERSION,
            crate::build_info::BUILD_DATE,
        ));
        app
    }

    /// Push a message to the UI log panel and append it to the log file (if open).
    fn push_log(&mut self, msg: String) {
        use std::io::Write;
        if let Some(ref mut f) = self.log_file {
            let _ = writeln!(f, "[{}] {}",
                chrono::Local::now().format("%H:%M:%S"), msg);
        }
        self.log_messages.push(msg);
    }

    fn process_messages(&mut self) {
        while let Ok(msg) = self.worker_rx.try_recv() {
            match msg {
                WorkerMessage::Log(s) => {
                    if s.is_empty() { continue; }
                    tracing::info!("[Worker] {}", s);
                    self.push_log(s);
                    if self.log_messages.len() > 2000 {
                        self.log_messages.drain(0..500);
                    }
                }
                WorkerMessage::PipeConnected { info } => {
                    self.pipe_connected = true;
                    self.is_busy = false;
                    self.push_log(format!("Connected to Audacity: {}", info));
                }
                WorkerMessage::PipeDisconnected => {
                    self.pipe_connected = false;
                    self.is_busy = false;
                    self.push_log("Disconnected from Audacity.".into());
                }
                WorkerMessage::PipeError(e) => {
                    self.pipe_connected = false;
                    self.is_busy = false;
                    self.push_log(format!("Pipe error: {}", e));
                }
                WorkerMessage::TracksDetected(tracks) => {
                    self.tracks = tracks;
                    self.selected_rows.clear();
                    self.push_log(format!("Loaded {} track(s).", self.tracks.len()));
                }
                WorkerMessage::TrackUpdate { index, updates } => {
                    self.apply_track_update(index, updates);
                }
                WorkerMessage::Progress { done, total } => {
                    self.progress = Some((done, total));
                }
                WorkerMessage::WorkerError(e) => {
                    self.is_busy = false;
                    self.push_log(format!("✗ {}", e));
                }
                WorkerMessage::WorkerFinished => {
                    self.is_busy = false;
                    self.progress = None;
                }
                WorkerMessage::DiscogsSearchCandidates(candidates) => {
                    self.is_busy = false;
                    if candidates.is_empty() {
                        self.push_log("Discogs: no results found.".into());
                    } else if candidates.len() == 1 {
                        self.push_log(format!(
                            "Discogs: 1 result — auto-selecting: {}", candidates[0].summary()
                        ));
                        self.discogs_candidates  = candidates;
                        self.discogs_auto_accept = true;
                    } else {
                        self.push_log(format!(
                            "Discogs: {} candidate(s) found:", candidates.len()
                        ));
                        for (i, c) in candidates.iter().enumerate() {
                            self.push_log(format!("  {}. {}", i + 1, c.summary()));
                        }
                        self.discogs_candidates  = candidates;
                        self.discogs_picker_open = true;
                    }
                }
                WorkerMessage::DiscogsReleaseFetched(release) => {
                    self.push_log(format!(
                        "Discogs release loaded: {} — {} ({}) — {} tracks",
                        release.album_artist, release.album, release.year, release.tracks.len()
                    ));
                    for side in release.sides() {
                        let side_tracks = release.side_tracks(side);
                        let dur_str = release.side_duration_secs(side)
                            .map(|d| format!("{:.0}s", d))
                            .unwrap_or_else(|| "?".into());
                        self.push_log(format!(
                            "  Side {}: {} tracks, {}",
                            side, side_tracks.len(), dur_str
                        ));
                    }
                    self.available_sides = release.sides();
                    self.selected_side   = None; // reset to "All" on each new release
                    self.is_busy = false;
                    self.discogs_release = Some(release);
                }
                WorkerMessage::CoverArtData(bytes) => {
                    self.cover_image_bytes   = Some(bytes.clone());
                    self.pending_cover_bytes = Some(bytes);
                }
                WorkerMessage::WaveformReady { path, samples, duration_secs } => {
                    self.push_log(format!(
                        "Waveform ready: {:.0}s, {} bars", duration_secs, samples.len()
                    ));
                    self.analysis_wav      = Some(path);
                    self.waveform_samples  = Some(samples);
                    self.waveform_duration = duration_secs;
                    self.waveform_drag     = None;
                    self.is_busy           = false;
                }
            }
        }
    }

    fn load_cover_texture(&mut self, ctx: &egui::Context, bytes: &[u8]) {
        match image::load_from_memory(bytes) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    rgba.as_raw(),
                );
                self.cover_texture = Some(ctx.load_texture(
                    "cover_art",
                    color_image,
                    egui::TextureOptions::LINEAR,
                ));
            }
            Err(e) => {
                self.push_log(format!("Cover art decode failed: {}", e));
            }
        }
    }

    fn load_cover_from_path(&mut self, ctx: &egui::Context, path: &str) {
        match std::fs::read(path) {
            Ok(bytes) => {
                self.cover_image_bytes = Some(bytes.clone());
                self.load_cover_texture(ctx, &bytes);
            }
            Err(e) => self.push_log(format!("Cover art: {}", e)),
        }
    }

    fn apply_track_update(&mut self, index: usize, update: TrackUpdate) {
        if let Some(track) = self.tracks.iter_mut().find(|t| t.index == index) {
            if let Some(v) = update.title { if !v.is_empty() { track.title = v; } }
            if let Some(v) = update.artist { if !v.is_empty() { track.artist = v; } }
            if let Some(v) = update.album { if !v.is_empty() { track.album = v; } }
            if let Some(v) = update.album_artist { if !v.is_empty() { track.album_artist = v; } }
            if let Some(v) = update.genre { if !v.is_empty() { track.genre = v; } }
            if let Some(v) = update.track_number { if !v.is_empty() { track.track_number = v; } }
            if let Some(v) = update.year { if !v.is_empty() { track.year = v; } }
            if let Some(v) = update.discogs_release_id { track.discogs_release_id = v; }
            if let Some(v) = update.export_path { track.export_path = Some(v); }
        }
    }

    fn connect_to_audacity(&mut self, ctx: egui::Context) {
        self.is_busy = true;
        let tx   = self.worker_tx.clone();
        let pipe = self.pipe.clone();

        // Use a per-process temp path so we don't collide across instances.
        let wav_path = std::path::PathBuf::from(format!(
            "/tmp/vripr_analysis_{}.wav",
            std::process::id()
        ));

        self.rt.spawn(async move {
            // --- 1. Open the pipe ---
            let _ = tx.send(WorkerMessage::Log("Connecting to Audacity...".into()));
            let connect_result = tokio::task::spawn_blocking({
                let pipe = pipe.clone();
                move || {
                    let mut g = pipe.lock().map_err(|e| anyhow::anyhow!("{}", e))?;
                    if g.is_connected() { g.disconnect(); }
                    g.connect()
                }
            }).await;

            match connect_result {
                Ok(Ok(())) => {
                    let _ = tx.send(WorkerMessage::Log("Pipe opened.".into()));
                    let _ = tx.send(WorkerMessage::PipeConnected { info: "Connected".into() });
                }
                Ok(Err(e)) => {
                    let _ = tx.send(WorkerMessage::PipeError(format!("{}", e)));
                    ctx.request_repaint();
                    return;
                }
                Err(e) => {
                    let _ = tx.send(WorkerMessage::PipeError(format!("Task error: {}", e)));
                    ctx.request_repaint();
                    return;
                }
            }
            ctx.request_repaint();

            // --- 2. Export project to WAV (captures user edits, e.g. needle-drop removal) ---
            let _ = tx.send(WorkerMessage::Log(format!(
                "Exporting project to WAV for analysis — this may take a minute…  {}",
                wav_path.display()
            )));
            let export_result = tokio::task::spawn_blocking({
                let pipe = pipe.clone();
                let path = wav_path.clone();
                move || {
                    let mut g = pipe.lock().map_err(|e| anyhow::anyhow!("{}", e))?;
                    g.export_full_wav(&path)
                }
            }).await;

            match export_result {
                Ok(Ok(())) => {
                    let _ = tx.send(WorkerMessage::Log("WAV export complete.".into()));
                }
                Ok(Err(e)) => {
                    let _ = tx.send(WorkerMessage::Log(format!("WAV export failed: {e} — detection will fall back to source file.")));
                    let _ = tx.send(WorkerMessage::WorkerFinished);
                    ctx.request_repaint();
                    return;
                }
                Err(e) => {
                    let _ = tx.send(WorkerMessage::Log(format!("WAV export task error: {e}")));
                    let _ = tx.send(WorkerMessage::WorkerFinished);
                    ctx.request_repaint();
                    return;
                }
            }
            ctx.request_repaint();

            // --- 3. Decode waveform display data ---
            let _ = tx.send(WorkerMessage::Log("Computing waveform display…".into()));
            let wf_result = tokio::task::spawn_blocking({
                let path = wav_path.clone();
                move || crate::audio::compute_waveform_display(&path, 2000)
            }).await;

            match wf_result {
                Ok(Ok((samples, duration_secs))) => {
                    let _ = tx.send(WorkerMessage::WaveformReady {
                        path: wav_path,
                        samples,
                        duration_secs,
                    });
                    // WaveformReady handler sets is_busy = false
                }
                Ok(Err(e)) => {
                    let _ = tx.send(WorkerMessage::Log(format!("Waveform compute failed: {e}")));
                    let _ = tx.send(WorkerMessage::WorkerFinished);
                }
                Err(e) => {
                    let _ = tx.send(WorkerMessage::Log(format!("Waveform task error: {e}")));
                    let _ = tx.send(WorkerMessage::WorkerFinished);
                }
            }

            ctx.request_repaint();
        });
    }

    fn disconnect_from_audacity(&mut self) {
        if let Ok(mut pipe) = self.pipe.lock() {
            pipe.disconnect();
        }
        self.pipe_connected = false;
        self.push_log("Disconnected from Audacity.".into());
    }

    fn set_labels(&mut self, ctx: egui::Context) {
        if self.tracks.is_empty() {
            self.push_log("No tracks — fetch a release first.".into());
            return;
        }
        if !self.pipe_connected {
            self.push_log("Not connected to Audacity.".into());
            return;
        }
        self.is_busy = true;
        let pipe   = self.pipe.clone();
        let tx     = self.worker_tx.clone();
        let tracks = self.tracks.clone();

        self.rt.spawn(async move {
            let tx2 = tx.clone();
            let result = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                let mut g = pipe.lock().map_err(|e| anyhow::anyhow!("{}", e))?;

                // Phase 1: clear loop — retry until Audacity confirms empty
                for attempt in 1usize..=5 {
                    g.clear_label_tracks()?;
                    let remaining = g.get_labels()?;
                    if remaining.is_empty() {
                        let msg = if attempt == 1 {
                            "Label track cleared.".into()
                        } else {
                            format!("Label track cleared (attempt {}).", attempt)
                        };
                        let _ = tx2.send(WorkerMessage::Log(msg));
                        break;
                    }
                    if attempt == 5 {
                        return Err(anyhow::anyhow!(
                            "Could not clear all labels after 5 attempts ({} remain)",
                            remaining.len()
                        ));
                    }
                    let _ = tx2.send(WorkerMessage::Log(format!(
                        "  {} label(s) still present, retrying clear...", remaining.len()
                    )));
                }

                // Phase 2: add new labels
                let _ = tx2.send(WorkerMessage::Log(
                    format!("Writing {} label(s)...", tracks.len())
                ));
                g.add_labels_from_tracks(&tracks)?;

                // Phase 3: verify labels took
                let labels = g.get_labels()?;
                if labels.len() == tracks.len() {
                    let _ = tx2.send(WorkerMessage::Log(format!(
                        "Labels verified: {}/{} confirmed in Audacity.",
                        labels.len(), tracks.len()
                    )));
                } else {
                    let _ = tx2.send(WorkerMessage::Log(format!(
                        "Label mismatch: sent {}, Audacity reports {}.",
                        tracks.len(), labels.len()
                    )));
                }
                for (i, (_, _, title)) in labels.iter().enumerate() {
                    let _ = tx2.send(WorkerMessage::Log(format!("  {}: \"{}\"", i + 1, title)));
                }

                Ok(())
            }).await;

            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => { let _ = tx.send(WorkerMessage::Log(format!("Label error: {}", e))); }
                Err(e)     => { let _ = tx.send(WorkerMessage::Log(format!("Label task error: {}", e))); }
            }
            let _ = tx.send(WorkerMessage::WorkerFinished);
            ctx.request_repaint();
        });
    }

    fn export_all(&mut self, ctx: egui::Context) {
        if self.tracks.is_empty() || !self.pipe_connected {
            return;
        }
        self.is_busy = true;
        let pipe         = self.pipe.clone();
        let config       = self.config.clone();
        let tx           = self.worker_tx.clone();
        let tracks       = self.tracks.clone();
        let cover_bytes  = self.cover_image_bytes.clone();
        self.rt.spawn(run_export_worker(tracks, pipe, config, tx, ctx, cover_bytes));
    }

    fn export_selected(&mut self, ctx: egui::Context) {
        if self.selected_rows.is_empty() || !self.pipe_connected {
            return;
        }
        self.is_busy = true;
        let tracks: Vec<TrackMeta> = self.selected_rows.iter()
            .filter_map(|&i| self.tracks.get(i).cloned())
            .collect();
        let pipe        = self.pipe.clone();
        let config      = self.config.clone();
        let tx          = self.worker_tx.clone();
        let cover_bytes = self.cover_image_bytes.clone();
        self.rt.spawn(run_export_worker(tracks, pipe, config, tx, ctx, cover_bytes));
    }

    fn fetch_discogs_release(&mut self, ctx: egui::Context) {
        let query = {
            let artist = self.apply_artist.trim().to_string();
            let album  = self.apply_album.trim().to_string();
            match (artist.is_empty(), album.is_empty()) {
                (false, false) => format!("{} {}", artist, album),
                (false, true)  => artist,
                (true,  false) => album,
                (true,  true)  => {
                    self.push_log(
                        "⚠ Set Artist and/or Album in the apply-all strip before fetching.".into()
                    );
                    return;
                }
            }
        };

        if self.config.discogs_token.is_empty() {
            self.push_log("⚠ Discogs token not set — open Settings.".into());
            return;
        }

        self.is_busy = true;
        self.discogs_picker_token = self.config.discogs_token.clone();
        let token = self.config.discogs_token.clone();
        let tx    = self.worker_tx.clone();

        let _ = tx.send(WorkerMessage::Log(format!("Searching Discogs: \"{}\"", query)));
        {
            let masked = format!(
                "https://api.discogs.com/database/search?q={}&type=release&per_page=10&token=***",
                discogs_encode_query(&query)
            );
            self.push_log(format!("Discogs URL: {}", masked));
        }

        self.rt.spawn(async move {
            match discogs_search_candidates(&query, &token, 10).await {
                Ok(candidates) => {
                    let _ = tx.send(WorkerMessage::Log(format!(
                        "Discogs returned {} candidate(s)", candidates.len()
                    )));
                    let _ = tx.send(WorkerMessage::DiscogsSearchCandidates(candidates));
                }
                Err(e) => {
                    let _ = tx.send(WorkerMessage::WorkerError(
                        format!("Discogs search error: {}", e)
                    ));
                }
            }
            ctx.request_repaint();
        });
    }

    fn fetch_discogs_by_catno(&mut self, catno: String, ctx: egui::Context) {
        if self.config.discogs_token.is_empty() {
            self.push_log("⚠ Discogs token not set — open Settings.".into());
            return;
        }
        if self.is_busy { return; }

        self.is_busy = true;
        self.discogs_picker_token = self.config.discogs_token.clone();
        let token = self.config.discogs_token.clone();
        let tx    = self.worker_tx.clone();

        self.push_log(format!("Searching Discogs by catalogue number: {}", catno));

        self.rt.spawn(async move {
            match discogs_search_by_catno(&catno, &token, 10).await {
                Ok(candidates) => {
                    let _ = tx.send(WorkerMessage::Log(format!(
                        "Discogs catno search returned {} candidate(s)", candidates.len()
                    )));
                    let _ = tx.send(WorkerMessage::DiscogsSearchCandidates(candidates));
                }
                Err(e) => {
                    let _ = tx.send(WorkerMessage::WorkerError(
                        format!("Discogs catno search error: {}", e)
                    ));
                }
            }
            ctx.request_repaint();
        });
    }

    fn fetch_release_by_candidate(&mut self, idx: usize, ctx: egui::Context) {
        let Some(c) = self.discogs_candidates.get(idx) else { return };
        let release_id   = c.id.clone();
        let token        = self.discogs_picker_token.clone();
        let tx           = self.worker_tx.clone();
        let config       = self.config.clone();
        let _pipe         = self.pipe.clone();
        let analysis_wav = self.analysis_wav.clone();
        let selected_side = self.selected_side;

        self.is_busy = true;
        self.push_log(format!(
            "Fetching release #{}: {} — {}",
            release_id, c.artist, c.album
        ));

        self.rt.spawn(async move {
            // --- 1. Fetch full Discogs release ---
            let release = match discogs_fetch_release(&release_id, &token).await {
                Ok(Some(r)) => r,
                Ok(None) => {
                    let _ = tx.send(WorkerMessage::Log(
                        format!("Discogs: release {} not found", release_id)
                    ));
                    let _ = tx.send(WorkerMessage::WorkerFinished);
                    ctx.request_repaint();
                    return;
                }
                Err(e) => {
                    let _ = tx.send(WorkerMessage::WorkerError(format!("Discogs fetch: {}", e)));
                    ctx.request_repaint();
                    return;
                }
            };

            let _ = tx.send(WorkerMessage::DiscogsReleaseFetched(release.clone()));

            // --- 2. Fetch cover art (non-fatal) ---
            if let Some(img_url) = &release.cover_image_url {
                let url = img_url.clone();
                match discogs_fetch_image(&url).await {
                    Ok(bytes) => {
                        let _ = tx.send(WorkerMessage::CoverArtData(bytes));
                    }
                    Err(e) => {
                        let _ = tx.send(WorkerMessage::Log(
                            format!("Cover art: {}", e)
                        ));
                    }
                }
            }

            // --- 3. Resolve audio file path ---
            // Use the analysis WAV exported on connect (reflects user edits).
            let audio_path: Option<PathBuf> = analysis_wav
                .as_ref()
                .filter(|wav| wav.exists())
                .map(|wav| {
                    let _ = tx.send(WorkerMessage::Log(
                        format!("Using analysis WAV: {}", wav.display())
                    ));
                    wav.clone()
                });

            // --- 4. Detect actual track starts (onset walk) or fall back to synthetic ---
            let disc_refs: Vec<&crate::metadata::DiscogsTrack> = match selected_side {
                None    => release.tracks.iter().collect(),
                Some(s) => {
                    let _ = tx.send(WorkerMessage::Log(
                        format!("Side filter active: processing Side {} only.", s)
                    ));
                    release.tracks.iter().filter(|t| t.side == s).collect()
                }
            };

            // Count how many disc_refs have valid durations (after side filtering)
            let valid_disc_durs = disc_refs.iter()
                .filter(|t| t.duration_secs.map(|d| d > 0.0).unwrap_or(false))
                .count();

            let tracks: Vec<TrackMeta> = if let Some(path) = audio_path.filter(|p| p.exists()) {
                // === Audio file available — always attempt detection ===
                let expected = disc_refs.len();

                if expected > 8 {
                    let _ = tx.send(WorkerMessage::Log(format!(
                        "⚠  Expecting {} tracks — detection is most reliable with one vinyl \
                         side per session (≤6 tracks). Consider recording one side at a time \
                         and using the Side selector for metadata assignment.",
                        expected
                    )));
                }

                // Step 1: duration-guided detection when every expected track has a known duration.
                // Uses Discogs durations as anchors to locate real silence boundaries —
                // more reliable than a blind scan when timing information is available.
                let mut best_detected: Vec<crate::audio::DetectedTrack> = Vec::new();

                if valid_disc_durs == expected && expected > 0 {
                    let disc_durs: Vec<f64> = disc_refs.iter()
                        .map(|t| t.duration_secs.unwrap_or(0.0))
                        .collect();
                    let guided_cfg = GuidedDetectorConfig {
                        threshold_db:       config.silence_threshold_db,
                        adaptive:           config.use_adaptive_threshold,
                        adaptive_margin_db: config.adaptive_margin_db,
                        ..GuidedDetectorConfig::default()
                    };
                    let path2 = path.clone();
                    let _ = tx.send(WorkerMessage::Log(
                        "Guided detection: using Discogs durations as anchors…".into()
                    ));
                    let guided_result = tokio::task::spawn_blocking(move || {
                        detect_tracks_guided(&path2, &disc_durs, &guided_cfg, &mut |_| {})
                    }).await;

                    match guided_result {
                        Ok(Ok(detected)) if detected.len() == expected => {
                            let _ = tx.send(WorkerMessage::Log(format!(
                                "  ✓ guided: {} track(s) located", expected
                            )));
                            best_detected = detected;
                        }
                        Ok(Ok(detected)) => {
                            let _ = tx.send(WorkerMessage::Log(format!(
                                "  guided: {} vs {} expected — falling back to scan",
                                detected.len(), expected
                            )));
                        }
                        Ok(Err(e)) => {
                            let _ = tx.send(WorkerMessage::Log(format!(
                                "  guided failed ({}), falling back to scan", e
                            )));
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerMessage::Log(format!(
                                "  guided task error ({}), falling back to scan", e
                            )));
                        }
                    }
                }

                // Step 2: blind retry loop if guided didn't nail the count.
                if best_detected.len() != expected {
                    // Retry schedule: progressively shorter silence thresholds.
                    // gap_fill must always be < min_silence.
                    // Format: (min_silence_secs, min_sound_secs, gap_fill_secs)
                    let retry_params: &[(f64, f64, f64)] = &[
                        (0.70, 15.0, 0.25),
                        (0.40, 10.0, 0.15),
                        (0.20,  5.0, 0.07),
                        (0.10,  3.0, 0.04),
                        (0.05,  2.0, 0.02),
                    ];
                    let det_method      = config.detection_method.clone();
                    let onnx_model_path = config.onnx_model_path.clone();
                    let onnx_cfg        = OnnxDetectorConfig {
                        min_sound_secs:   config.silence_min_sound_dur,
                        min_silence_secs: config.silence_min_duration,
                        ..OnnxDetectorConfig::default()
                    };

                    // ONNX: single-pass only — no threshold retry loop.
                    if det_method == DetectionMethod::Onnx {
                        let _ = tx.send(WorkerMessage::Log(
                            "Scanning audio (ONNX AI detector)…".to_string()
                        ));
                        if onnx_model_path.is_empty() {
                            let _ = tx.send(WorkerMessage::Log(
                                "⚠  No ONNX model configured — set path in Settings → Detection Method.".to_string()
                            ));
                        } else {
                            let path2      = path.clone();
                            let tx2        = tx.clone();
                            let model_path = std::path::PathBuf::from(&onnx_model_path);
                            let result = tokio::task::spawn_blocking(move || {
                                let tx3 = tx2;
                                let mut last_pct = 0u8;
                                let progress_cb = &mut |p: f64| {
                                    let pct = (p * 100.0) as u8;
                                    if pct >= last_pct + 10 {
                                        last_pct = pct;
                                        let _ = tx3.send(WorkerMessage::Log(format!(
                                            "  ONNX inference… {}%", pct
                                        )));
                                    }
                                };
                                detect_tracks_onnx(&path2, &model_path, &onnx_cfg, progress_cb)
                            }).await;
                            match result {
                                Ok(Ok((detected, diag))) => {
                                    let _ = tx.send(WorkerMessage::Log(format!(
                                        "ONNX: {} region(s) in {:.0}s",
                                        detected.len(), diag.total_secs
                                    )));
                                    best_detected = detected;
                                }
                                Ok(Err(e)) => {
                                    let _ = tx.send(WorkerMessage::Log(format!(
                                        "⚠  ONNX detection failed: {}", e
                                    )));
                                }
                                Err(e) => {
                                    let _ = tx.send(WorkerMessage::Log(format!(
                                        "⚠  ONNX task error: {}", e
                                    )));
                                }
                            }
                        }
                        // Skip the threshold retry loop entirely for ONNX.
                        // Fall through to pairing logic with whatever was found.
                    } else {

                    for (attempt, &(min_sil, min_snd, gap_fill)) in retry_params.iter().enumerate() {
                        let det_cfg = DetectorConfig {
                            threshold_db:                config.silence_threshold_db,
                            adaptive:                    config.use_adaptive_threshold,
                            adaptive_margin_db:          config.adaptive_margin_db,
                            min_silence_secs:            min_sil,
                            min_sound_secs:              min_snd,
                            gap_fill_secs:               gap_fill,
                            window_ms:                   50,
                            spectral_flatness_threshold: config.spectral_flatness_threshold,
                            ..DetectorConfig::default()
                        };

                        if attempt == 0 {
                            let _ = tx.send(WorkerMessage::Log(format!(
                                "Scanning audio ({} detector)…", det_method.display_str()
                            )));
                        }

                        let path2  = path.clone();
                        let tx2    = tx.clone();
                        let method = det_method.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            let mut last_pct = 0u8;
                            let progress_cb = &mut |p: f64| {
                                let pct = (p * 100.0) as u8;
                                if pct >= last_pct + 10 {
                                    last_pct = pct;
                                    let _ = tx2.send(WorkerMessage::Log(format!(
                                        "  decoding… {}%", pct
                                    )));
                                }
                            };
                            match method {
                                DetectionMethod::Spectral => detect_tracks_spectral(&path2, &det_cfg, progress_cb),
                                DetectionMethod::Hmm      => detect_tracks_hmm(&path2, &det_cfg, progress_cb),
                                DetectionMethod::Rms | DetectionMethod::Onnx
                                                      => detect_tracks(&path2, &det_cfg, progress_cb),
                            }
                        }).await;

                        match result {
                            Ok(Ok((detected, diag))) => {
                                let found = detected.len();
                                if attempt == 0 {
                                    let _ = tx.send(WorkerMessage::Log(format!(
                                        "Scan: {} region(s) in {:.0}s (threshold {:.1} dB{})",
                                        found, diag.total_secs, diag.threshold_db,
                                        diag.noise_floor_db
                                            .map(|nf| format!(", noise floor {:.1} dB", nf))
                                            .unwrap_or_default(),
                                    )));
                                } else {
                                    let _ = tx.send(WorkerMessage::Log(format!(
                                        "  retry {}: min_silence={:.2}s gap_fill={:.2}s min_sound={:.0}s → {} region(s)",
                                        attempt, min_sil, gap_fill, min_snd, found
                                    )));
                                }
                                best_detected = detected;
                                if found == expected { break; }
                            }
                            Ok(Err(e)) => {
                                let _ = tx.send(WorkerMessage::Log(
                                    format!("Silence scan failed: {}", e)
                                ));
                                break;
                            }
                            Err(e) => {
                                let _ = tx.send(WorkerMessage::Log(
                                    format!("Scan task error: {}", e)
                                ));
                                break;
                            }
                        }
                    }
                    } // end else (non-ONNX retry loop)
                } // end if best_detected.len() != expected

                // Step 3: resolve count and select final output.
                if best_detected.is_empty() {
                    if valid_disc_durs > 0 {
                        let _ = tx.send(WorkerMessage::Log(
                            "No sound regions detected — using Discogs durations.".into()
                        ));
                        split_by_discogs_durations_fmt(&disc_refs, &release, 0.0, 2.0, &config.track_number_format)
                    } else {
                        let _ = tx.send(WorkerMessage::Log(
                            "No sound regions detected and no durations — creating title placeholders.".into()
                        ));
                        title_only_tracks(&disc_refs, &release, &config.track_number_format)
                    }
                } else {
                    let found = best_detected.len();
                    let use_duration_fallback = if found == expected {
                        let _ = tx.send(WorkerMessage::Log(format!(
                            "  ✓ {} track(s) matched Discogs count", found
                        )));
                        false
                    } else if found > expected {
                        let _ = tx.send(WorkerMessage::Log(format!(
                            "  ⚠ {} detected vs {} expected — discarding {} extra region(s)",
                            found, expected, found - expected
                        )));
                        best_detected.truncate(expected);
                        false
                    } else {
                        // Too few after all attempts — durations are more reliable than partial detection.
                        let _ = tx.send(WorkerMessage::Log(format!(
                            "  ⚠ only {} of {} tracks found after all attempts — {}",
                            found, expected,
                            if valid_disc_durs > 0 { "falling back to Discogs durations" }
                            else { "using title placeholders" }
                        )));
                        true
                    };

                    if use_duration_fallback {
                        if valid_disc_durs > 0 {
                            split_by_discogs_durations_fmt(&disc_refs, &release, 0.0, 2.0, &config.track_number_format)
                        } else {
                            title_only_tracks(&disc_refs, &release, &config.track_number_format)
                        }
                    } else {
                        pair_detected_with_meta(&best_detected, &disc_refs, &release, &config.track_number_format)
                    }
                }

            } else {
                // === No audio file ===
                if valid_disc_durs > 0 {
                    let _ = tx.send(WorkerMessage::Log(
                        "No audio file — using Discogs durations for track times.".into()
                    ));
                    split_by_discogs_durations_fmt(&disc_refs, &release, 0.0, 2.0, &config.track_number_format)
                } else {
                    let _ = tx.send(WorkerMessage::Log(
                        "No audio file and no durations — creating title-only placeholders.".into()
                    ));
                    title_only_tracks(&disc_refs, &release, &config.track_number_format)
                }
            };

            // --- 5. Auto-log duration comparison ---
            let (report, _) = compare_duration_report(&tracks, &disc_refs, 5.0);
            for line in report.lines() {
                if !line.is_empty() {
                    let _ = tx.send(WorkerMessage::Log(line.to_string()));
                }
            }

            let _ = tx.send(WorkerMessage::TracksDetected(tracks));
            let _ = tx.send(WorkerMessage::WorkerFinished);
            ctx.request_repaint();
        });
    }

    #[allow(dead_code)]
    fn split_by_durations(&mut self) {
        let Some(release) = self.discogs_release.clone() else { return };

        if self.tracks.is_empty() {
            let disc_refs: Vec<&crate::metadata::DiscogsTrack> =
                release.tracks.iter().collect();
            let new_tracks = split_by_discogs_durations_fmt(
                &disc_refs, &release, 0.0, 2.0, &self.config.track_number_format
            );
            self.push_log(format!(
                "Generated {} track(s) from Discogs durations.",
                new_tracks.len()
            ));
            self.tracks = new_tracks;
            self.selected_rows.clear();
        } else {
            let disc_refs: Vec<&crate::metadata::DiscogsTrack> =
                release.tracks.iter().collect();
            assign_discogs_titles(&mut self.tracks, &disc_refs, &release);
            self.push_log(format!(
                "Assigned Discogs metadata to {} track(s).",
                self.tracks.len().min(disc_refs.len())
            ));
        }
    }

    /// Apply a vinyl side filter — re-assigns Discogs metadata to detected tracks
    /// without re-running audio detection. Useful when ripping sides out of order.
    fn apply_side_filter(&mut self, side: Option<char>) {
        self.selected_side = side;
        let label = side.map_or_else(|| "All sides".to_string(), |s| format!("Side {}", s));

        let Some(release) = &self.discogs_release else {
            self.push_log(format!("Side → {} (no release loaded; fetch a release first)", label));
            return;
        };

        let disc_refs: Vec<&crate::metadata::DiscogsTrack> = match side {
            None    => release.tracks.iter().collect(),
            Some(s) => release.tracks.iter().filter(|t| t.side == s).collect(),
        };

        if disc_refs.is_empty() {
            self.push_log(format!("Side {}: no tracks found in release", side.unwrap_or('?')));
            return;
        }

        crate::metadata::assign_discogs_titles(&mut self.tracks, &disc_refs, release);
        self.push_log(format!(
            "Side filter → {} — reassigned metadata for {} track(s) from {} Discogs track(s).",
            label,
            self.tracks.len().min(disc_refs.len()),
            disc_refs.len(),
        ));
    }

    fn rescan(&mut self, ctx: egui::Context) {
        let Some(wav_path) = self.analysis_wav.clone() else {
            self.push_log("No analysis WAV — connect first.".into());
            return;
        };
        if !wav_path.exists() {
            self.push_log("Analysis WAV not found — reconnect to re-export.".into());
            return;
        }

        self.is_busy = true;
        let tx           = self.worker_tx.clone();
        let config       = self.config.clone();
        let pinned_tracks: Vec<TrackMeta> = self.tracks.iter()
            .filter(|t| t.pinned)
            .cloned()
            .collect();
        let all_tracks   = self.tracks.clone();
        let release      = self.discogs_release.clone();
        let track_fmt    = config.track_number_format.clone();

        self.rt.spawn(async move {
            let _ = tx.send(WorkerMessage::Log(format!(
                "Re-scanning ({} pinned track(s) preserved)…", pinned_tracks.len()
            )));

            let expected = all_tracks.len();

            let retry_params: &[(f64, f64, f64)] = &[
                (0.70, 15.0, 0.25),
                (0.40, 10.0, 0.15),
                (0.20,  5.0, 0.07),
                (0.10,  3.0, 0.04),
                (0.05,  2.0, 0.02),
            ];

            let mut best: Vec<crate::audio::DetectedTrack> = Vec::new();
            let det_method      = config.detection_method.clone();
            let onnx_model_path = config.onnx_model_path.clone();
            let onnx_cfg        = OnnxDetectorConfig {
                min_sound_secs:   config.silence_min_sound_dur,
                min_silence_secs: config.silence_min_duration,
                ..OnnxDetectorConfig::default()
            };

            if det_method == DetectionMethod::Onnx {
                let _ = tx.send(WorkerMessage::Log(
                    "Re-scanning audio (ONNX AI detector)…".to_string()
                ));
                if onnx_model_path.is_empty() {
                    let _ = tx.send(WorkerMessage::Log(
                        "⚠  No ONNX model configured — set path in Settings → Detection Method.".to_string()
                    ));
                } else {
                    let path2      = wav_path.clone();
                    let tx2        = tx.clone();
                    let model_path = std::path::PathBuf::from(&onnx_model_path);
                    let result = tokio::task::spawn_blocking(move || {
                        let tx3 = tx2;
                        let mut last_pct = 0u8;
                        let progress_cb = &mut |p: f64| {
                            let pct = (p * 100.0) as u8;
                            if pct >= last_pct + 10 {
                                last_pct = pct;
                                let _ = tx3.send(WorkerMessage::Log(format!(
                                    "  ONNX inference… {}%", pct
                                )));
                            }
                        };
                        detect_tracks_onnx(&path2, &model_path, &onnx_cfg, progress_cb)
                    }).await;
                    match result {
                        Ok(Ok((detected, diag))) => {
                            let _ = tx.send(WorkerMessage::Log(format!(
                                "ONNX re-scan: {} region(s) in {:.0}s",
                                detected.len(), diag.total_secs
                            )));
                            best = detected;
                        }
                        Ok(Err(e)) => {
                            let _ = tx.send(WorkerMessage::Log(format!("ONNX re-scan failed: {e}")));
                            let _ = tx.send(WorkerMessage::WorkerFinished);
                            ctx.request_repaint();
                            return;
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerMessage::Log(format!("ONNX re-scan task error: {e}")));
                            let _ = tx.send(WorkerMessage::WorkerFinished);
                            ctx.request_repaint();
                            return;
                        }
                    }
                }
            } else {

            for (attempt, &(min_sil, min_snd, gap_fill)) in retry_params.iter().enumerate() {
                let det_cfg = DetectorConfig {
                    threshold_db:                config.silence_threshold_db,
                    adaptive:                    config.use_adaptive_threshold,
                    adaptive_margin_db:          config.adaptive_margin_db,
                    min_silence_secs:            min_sil,
                    min_sound_secs:              min_snd,
                    gap_fill_secs:               gap_fill,
                    window_ms:                   50,
                    spectral_flatness_threshold: config.spectral_flatness_threshold,
                    ..DetectorConfig::default()
                };
                if attempt == 0 {
                    let _ = tx.send(WorkerMessage::Log(format!(
                        "Re-scanning audio ({} detector)…", det_method.display_str()
                    )));
                }

                let path2  = wav_path.clone();
                let tx2    = tx.clone();
                let method = det_method.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let mut last_pct = 0u8;
                    let progress_cb = &mut |p: f64| {
                        let pct = (p * 100.0) as u8;
                        if pct >= last_pct + 10 {
                            last_pct = pct;
                            let _ = tx2.send(WorkerMessage::Log(format!(
                                "  decoding… {}%", pct
                            )));
                        }
                    };
                    match method {
                        DetectionMethod::Spectral => detect_tracks_spectral(&path2, &det_cfg, progress_cb),
                        DetectionMethod::Hmm      => detect_tracks_hmm(&path2, &det_cfg, progress_cb),
                        DetectionMethod::Rms | DetectionMethod::Onnx
                                              => detect_tracks(&path2, &det_cfg, progress_cb),
                    }
                }).await;

                match result {
                    Ok(Ok((detected, _diag))) => {
                        let found = detected.len();
                        if attempt == 0 {
                            let _ = tx.send(WorkerMessage::Log(format!("Re-scan: {} region(s)", found)));
                        } else {
                            let _ = tx.send(WorkerMessage::Log(format!(
                                "  retry {}: min_silence={:.2}s → {} region(s)", attempt, min_sil, found
                            )));
                        }
                        best = detected;
                        if found == expected { break; }
                    }
                    Ok(Err(e)) => {
                        let _ = tx.send(WorkerMessage::Log(format!("Re-scan failed: {e}")));
                        let _ = tx.send(WorkerMessage::WorkerFinished);
                        ctx.request_repaint();
                        return;
                    }
                    Err(e) => {
                        let _ = tx.send(WorkerMessage::Log(format!("Re-scan task error: {e}")));
                        let _ = tx.send(WorkerMessage::WorkerFinished);
                        ctx.request_repaint();
                        return;
                    }
                }
            }
            } // end else (non-ONNX retry loop)

            // Merge detected regions with pinned tracks.
            // For each detected region, if a pinned track overlaps it by >50%, keep the pinned.
            let mut merged: Vec<TrackMeta> = Vec::new();

            for dt in &best {
                let dur = dt.end - dt.start;
                // Check overlap with any pinned track
                let pinned_match = pinned_tracks.iter().find(|p| {
                    let overlap_start = dt.start.max(p.start);
                    let overlap_end   = dt.end.min(p.end);
                    let overlap = (overlap_end - overlap_start).max(0.0);
                    overlap / dur.max(0.001) > 0.5
                });

                if let Some(p) = pinned_match {
                    merged.push(p.clone());
                } else {
                    // Find the closest original (non-pinned) track for metadata
                    let meta = all_tracks.iter().find(|t| !t.pinned && {
                        let overlap_start = dt.start.max(t.start);
                        let overlap_end   = dt.end.min(t.end);
                        overlap_end > overlap_start
                    });
                    merged.push(TrackMeta {
                        start: dt.start,
                        end:   dt.end,
                        title:             meta.map(|t| t.title.clone()).unwrap_or_default(),
                        artist:            meta.map(|t| t.artist.clone()).unwrap_or_default(),
                        album:             meta.map(|t| t.album.clone()).unwrap_or_default(),
                        album_artist:      meta.map(|t| t.album_artist.clone()).unwrap_or_default(),
                        year:              meta.map(|t| t.year.clone()).unwrap_or_default(),
                        genre:             meta.map(|t| t.genre.clone()).unwrap_or_default(),
                        discogs_release_id: meta.map(|t| t.discogs_release_id.clone()).unwrap_or_default(),
                        country:           meta.map(|t| t.country.clone()).unwrap_or_default(),
                        catalog:           meta.map(|t| t.catalog.clone()).unwrap_or_default(),
                        label:             meta.map(|t| t.label.clone()).unwrap_or_default(),
                        pinned: false,
                        ..Default::default()
                    });
                }
            }

            // Add any pinned tracks that detection missed entirely
            for p in &pinned_tracks {
                if !merged.iter().any(|m| m.pinned && m.start == p.start) {
                    merged.push(p.clone());
                }
            }

            // If we ended up with more tracks than expected, drop the extras from the tail
            // (keep pinned tracks if possible — sort first so pinned ones rank by position).
            if merged.len() > expected {
                merged.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
                let _ = tx.send(WorkerMessage::Log(format!(
                    "  ⚠ {} merged vs {} expected — discarding {} extra region(s)",
                    merged.len(), expected, merged.len() - expected
                )));
                merged.truncate(expected);
            }

            // Sort by start time and re-assign indices + track numbers
            merged.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
            for (i, t) in merged.iter_mut().enumerate() {
                t.index = i + 1;
                if !t.pinned {
                    // Re-assign track number from release if available
                    let track_num = release.as_ref()
                        .and_then(|r| r.tracks.get(i))
                        .map(|dt| match track_fmt {
                            crate::config::TrackNumberFormat::Alpha =>
                                format!("{}{}", dt.side, dt.number),
                            crate::config::TrackNumberFormat::Numeric =>
                                (i + 1).to_string(),
                        })
                        .unwrap_or_else(|| (i + 1).to_string());
                    t.track_number = track_num;
                }
            }

            let _ = tx.send(WorkerMessage::Log(format!(
                "Re-scan complete: {} track(s)", merged.len()
            )));
            let _ = tx.send(WorkerMessage::TracksDetected(merged));
            let _ = tx.send(WorkerMessage::WorkerFinished);
            ctx.request_repaint();
        });
    }

    fn run_diagnostics(&mut self) {
        self.push_log("=== Diagnostics ===".into());
        self.push_log(format!(
            "Config path: {:?}",
            crate::config::config_path()
        ));
        self.push_log(format!(
            "Pipe paths: {:?} | {:?}",
            AudacityPipe::pipe_paths().0,
            AudacityPipe::pipe_paths().1
        ));
        self.push_log(format!(
            "Pipes exist: {}",
            AudacityPipe::check_pipes()
        ));
        self.push_log(format!("Pipe connected: {}", self.pipe_connected));
        self.push_log(format!("Tracks loaded: {}", self.tracks.len()));
        self.push_log(format!("Export dir: {:?}", self.config.export_dir));
        self.push_log(format!(
            "Discogs token set: {}",
            !self.config.discogs_token.is_empty()
        ));
        self.push_log(format!(
            "Cover art: {}",
            if self.cover_texture.is_some() { "loaded" } else { "none" }
        ));
        self.push_log("=== End Diagnostics ===".into());
    }

    fn show_waveform_panel(&mut self, ctx: &egui::Context) {
        // Auto-clear playing state when the expected duration has elapsed
        if let Some(end) = self.play_end {
            if std::time::Instant::now() >= end {
                self.play_end = None;
            }
        }
        let is_playing = self.play_end.is_some();

        let wf_data = self.waveform_samples.as_ref().map(|s| {
            (s.clone(), self.waveform_duration, self.waveform_drag.clone())
        });
        let Some((samples, duration, mut drag)) = wf_data else { return };

        let track_bounds: Vec<(usize, f64, f64, bool)> = self.tracks.iter()
            .map(|t| (t.index, t.start, t.end, t.pinned))
            .collect();

        let mut sel = self.waveform_selection;

        let evt = crate::ui::waveform::show_waveform(
            ctx, &samples, duration, &track_bounds, &mut drag, &mut sel, is_playing,
        );

        self.waveform_drag      = drag;
        self.waveform_selection = sel;

        // Pin toggle
        if let Some(vi) = evt.toggle_pin {
            if let Some(track) = self.tracks.get_mut(vi) {
                track.pinned = !track.pinned;
                let msg = format!("Track {}: {}", track.index,
                    if track.pinned { "pinned 📌" } else { "unpinned" });
                self.push_log(msg);
            }
        }

        // Boundary drag
        if let Some((vi, is_start, new_time)) = evt.drag_update {
            if is_start {
                if let Some(track) = self.tracks.get_mut(vi) {
                    track.start = new_time.max(0.0).min(track.end - 0.5);
                    let new_start = track.start;
                    if vi > 0 {
                        if let Some(prev) = self.tracks.get_mut(vi - 1) {
                            prev.end = new_start;
                        }
                    }
                }
            } else {
                let next_start = self.tracks.get(vi + 1).map(|t| t.start);
                let max_end    = next_start.unwrap_or(duration);
                if let Some(track) = self.tracks.get_mut(vi) {
                    track.end = new_time.max(track.start + 0.5).min(max_end);
                    let new_end = track.end;
                    if let Some(next) = self.tracks.get_mut(vi + 1) {
                        next.start = new_end;
                    }
                }
            }
        }

        // Pin start (right-click menu)
        if let Some((vi, t)) = evt.pin_start {
            if let Some(track) = self.tracks.get_mut(vi) {
                track.start = t.max(0.0).min(track.end - 0.5);
                let new_start = track.start;
                if vi > 0 {
                    if let Some(prev) = self.tracks.get_mut(vi - 1) {
                        prev.end = new_start;
                    }
                }
                self.push_log(format!(
                    "Track {} start → {}", self.tracks[vi].index, fmt_secs(new_start)
                ));
            }
        }

        // Pin end (right-click menu)
        if let Some((vi, t)) = evt.pin_end {
            if let Some(track) = self.tracks.get_mut(vi) {
                track.end = t.max(track.start + 0.5);
                let new_end = track.end;
                if let Some(next) = self.tracks.get_mut(vi + 1) {
                    if next.start < new_end {
                        next.start = new_end + 1.0;
                    }
                }
                self.push_log(format!(
                    "Track {} end → {}", self.tracks[vi].index, fmt_secs(new_end)
                ));
            }
        }

        // Stop playback (right-click menu)
        if evt.stop_playback {
            if let Ok(mut pipe) = self.pipe.lock() {
                let _ = pipe.stop_playback();
            }
            self.play_end = None;
        }

        // Play region (right-click menu)
        if let Some((start, end)) = evt.play_region {
            if self.pipe_connected {
                let play_result = self.pipe.lock().ok()
                    .map(|mut pipe| pipe.play_region(start, end));
                match play_result {
                    Some(Ok(_)) => {
                        let dur = std::time::Duration::from_secs_f64(end - start + 0.5);
                        self.play_end = Some(std::time::Instant::now() + dur);
                    }
                    Some(Err(e)) => {
                        self.push_log(format!("⚠ Playback failed: {e}"));
                    }
                    None => {}
                }
            } else {
                self.push_log("⚠ Connect to Audacity to use playback".into());
            }
        }
    }

    fn show_toolbar_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            let state = ToolbarState {
                pipe_connected: self.pipe_connected,
                is_busy: self.is_busy,
                has_tracks: !self.tracks.is_empty(),
                has_selection: !self.selected_rows.is_empty(),
                has_discogs_release: self.discogs_release.is_some(),
                has_analysis_wav: self.analysis_wav.as_ref().map(|p| p.exists()).unwrap_or(false),
                available_sides: self.available_sides.clone(),
                selected_side: self.selected_side,
            };

            let actions = show_toolbar(ui, &state);

            for action in actions {
                match action {
                    ToolbarAction::OpenSettings  => self.settings_open = true,
                    ToolbarAction::Quit          => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    ToolbarAction::Connect       => self.connect_to_audacity(ctx.clone()),
                    ToolbarAction::Disconnect    => self.disconnect_from_audacity(),
                    ToolbarAction::SetLabels      => self.set_labels(ctx.clone()),
                    ToolbarAction::ExportAll      => self.export_all(ctx.clone()),
                    ToolbarAction::ExportSelected => self.export_selected(ctx.clone()),
                    ToolbarAction::Diagnostics   => self.run_diagnostics(),
                    ToolbarAction::AddTrack      => {
                        // Pre-populate with waveform selection if one exists
                        if let Some((s, e)) = self.waveform_selection {
                            self.manual_track_input.start = format!("{:.3}", s);
                            self.manual_track_input.end   = format!("{:.3}", e);
                        }
                        self.manual_track_open = true;
                    }
                    ToolbarAction::Rescan        => self.rescan(ctx.clone()),
                    ToolbarAction::ClearTracks   => {
                        self.tracks.clear();
                        self.selected_rows.clear();
                        self.push_log("Tracks cleared.".into());
                    }
                    ToolbarAction::FetchDiscogsRelease => self.fetch_discogs_release(ctx.clone()),
                    ToolbarAction::SideChanged(side)  => self.apply_side_filter(side),
                }
            }
        });
    }

    fn show_cover_panel(&mut self, ctx: &egui::Context) {
        let mut delete_idx: Option<usize> = None;

        egui::SidePanel::right("cover_art_panel")
            .resizable(true)
            .default_width(220.0)
            .min_width(100.0)
            .max_width(420.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Cover Art")
                        .color(egui::Color32::from_rgb(137, 180, 250))
                        .strong()
                );
                ui.add_space(4.0);

                if let Some(tex) = &self.cover_texture {
                    let size = tex.size_vec2();
                    let max_w = ui.available_width();
                    let aspect = size.y / size.x;
                    let display_w = max_w.min(size.x);
                    let display_h = display_w * aspect;
                    ui.image((tex.id(), egui::vec2(display_w, display_h)));
                } else {
                    let placeholder_size = egui::vec2(
                        ui.available_width(),
                        (ui.available_width()).min(180.0)
                    );
                    let (rect, _) = ui.allocate_exact_size(placeholder_size, egui::Sense::hover());
                    ui.painter().rect_filled(
                        rect,
                        4.0,
                        egui::Color32::from_rgb(49, 50, 68),
                    );
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "No cover art",
                        egui::FontId::proportional(13.0),
                        egui::Color32::from_rgb(108, 112, 134),
                    );
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label("Custom cover:");

                let path_resp = ui.add(
                    egui::TextEdit::singleline(&mut self.custom_cover_path)
                        .hint_text("Path to image file")
                        .desired_width(ui.available_width()),
                );
                let load_clicked = ui.button("Load").clicked();
                if load_clicked
                    || (path_resp.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    let path = self.custom_cover_path.clone();
                    if !path.is_empty() {
                        self.load_cover_from_path(ctx, &path);
                    }
                }

                // Track editor
                if let Some(idx) = self.editing_track_index {
                    if idx < self.tracks.len() {
                        ui.add_space(8.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.strong(
                                egui::RichText::new(format!("Track {}", self.tracks[idx].index))
                                    .color(egui::Color32::from_rgb(137, 180, 250))
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("✕").on_hover_text("Close editor").clicked() {
                                    self.editing_track_index = None;
                                }
                            });
                        });

                        egui::ScrollArea::vertical()
                            .id_source("track_editor_scroll")
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                const LBL: f32 = 78.0;
                                const SM:  f32 = 54.0;
                                const GAP: f32 = 4.0;

                                macro_rules! field_row {
                                    ($label:expr, $field:expr) => {{
                                        ui.horizontal(|ui| {
                                            ui.add_sized([LBL, 20.0], egui::Label::new($label));
                                            let w = (ui.available_width() - GAP).max(40.0);
                                            ui.add_sized([w, 20.0],
                                                egui::TextEdit::singleline($field));
                                        });
                                    }};
                                }
                                macro_rules! small_row {
                                    ($label:expr, $field:expr) => {{
                                        ui.horizontal(|ui| {
                                            ui.add_sized([LBL, 20.0], egui::Label::new($label));
                                            ui.add_sized([SM, 20.0],
                                                egui::TextEdit::singleline($field));
                                        });
                                    }};
                                }

                                field_row!("Title",        &mut self.tracks[idx].title);
                                field_row!("Artist",       &mut self.tracks[idx].artist);
                                field_row!("Album",        &mut self.tracks[idx].album);
                                field_row!("Album Artist", &mut self.tracks[idx].album_artist);
                                field_row!("Genre",        &mut self.tracks[idx].genre);
                                field_row!("Composer",     &mut self.tracks[idx].composer);
                                field_row!("Comments",     &mut self.tracks[idx].comments);
                                small_row!("Year",         &mut self.tracks[idx].year);
                                small_row!("Track #",      &mut self.tracks[idx].track_number);

                                ui.horizontal(|ui| {
                                    ui.add_sized([LBL, 20.0], egui::Label::new("Pinned"));
                                    ui.checkbox(&mut self.tracks[idx].pinned, "");
                                });

                                ui.add_space(2.0);
                                ui.separator();

                                // Start / End as MM:SS text fields
                                let track_end   = self.tracks[idx].end;
                                let track_start = self.tracks[idx].start;
                                let max_end     = self.waveform_duration.max(track_start + 0.1);

                                ui.horizontal(|ui| {
                                    ui.add_sized([LBL, 20.0], egui::Label::new("Start"));
                                    let w = (ui.available_width() - GAP).max(60.0);
                                    let id = egui::Id::new(("edit_start", idx));
                                    let resp = time_text_edit(ui, &mut self.tracks[idx].start, id, w);
                                    if resp.lost_focus() {
                                        // Clamp after parse
                                        self.tracks[idx].start = self.tracks[idx].start
                                            .max(0.0)
                                            .min((track_end - 0.1).max(0.0));
                                    }
                                });

                                ui.horizontal(|ui| {
                                    ui.add_sized([LBL, 20.0], egui::Label::new("End"));
                                    let w = (ui.available_width() - GAP).max(60.0);
                                    let id = egui::Id::new(("edit_end", idx));
                                    let resp = time_text_edit(ui, &mut self.tracks[idx].end, id, w);
                                    if resp.lost_focus() {
                                        self.tracks[idx].end = self.tracks[idx].end
                                            .max(track_start + 0.1)
                                            .min(max_end);
                                        let new_end = self.tracks[idx].end;
                                        // Push the next track's start forward if it now overlaps
                                        if let Some(next) = self.tracks.get_mut(idx + 1) {
                                            if next.start < new_end {
                                                next.start = new_end + 1.0;
                                            }
                                        }
                                    }
                                });

                                ui.horizontal(|ui| {
                                    ui.add_sized([LBL, 20.0], egui::Label::new("Duration"));
                                    let w = (ui.available_width() - GAP).max(60.0);
                                    let id = egui::Id::new(("edit_dur", idx));
                                    let mut dur = self.tracks[idx].end - self.tracks[idx].start;
                                    let resp = time_text_edit(ui, &mut dur, id, w);
                                    if resp.lost_focus() {
                                        let dur = dur.max(0.1);
                                        let new_end = (self.tracks[idx].start + dur)
                                            .min(max_end);
                                        self.tracks[idx].end = new_end;
                                        // Cascade: push next track's start forward if it overlaps
                                        if let Some(next) = self.tracks.get_mut(idx + 1) {
                                            if next.start < new_end {
                                                next.start = new_end + 1.0;
                                            }
                                        }
                                    }
                                });

                                ui.add_space(4.0);
                                ui.separator();
                                ui.horizontal(|ui| {
                                    if ui.button(
                                        egui::RichText::new("🗑 Delete")
                                            .color(egui::Color32::from_rgb(243, 139, 168))
                                    ).clicked() {
                                        delete_idx = Some(idx);
                                    }
                                });
                            });
                    } else {
                        self.editing_track_index = None;
                    }
                }
            });

        if let Some(idx) = delete_idx {
            if idx < self.tracks.len() {
                let removed = self.tracks.remove(idx);
                self.selected_rows.remove(&idx);
                self.editing_track_index = None;
                self.push_log(format!("Removed track: {}", removed.title));
                for (i, t) in self.tracks.iter_mut().enumerate() {
                    t.index = i + 1;
                }
            }
        }
    }

    fn show_central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Apply-to-all strip
            ui.add_space(4.0);
            let apply_artist       = &mut self.apply_artist;
            let apply_album        = &mut self.apply_album;
            let apply_album_artist = &mut self.apply_album_artist;
            let apply_genre        = &mut self.apply_genre;
            let apply_year         = &mut self.apply_year;
            let apply_catalog      = &mut self.apply_catalog;

            let strip = show_apply_all_strip(
                ui,
                apply_artist,
                apply_album,
                apply_album_artist,
                apply_genre,
                apply_year,
                apply_catalog,
            );

            let catno_for_fetch = apply_catalog.trim().to_string();

            if strip.apply_clicked {
                for track in &mut self.tracks {
                    if !apply_artist.is_empty()       { track.artist       = apply_artist.clone(); }
                    if !apply_album.is_empty()        { track.album        = apply_album.clone(); }
                    if !apply_album_artist.is_empty() { track.album_artist = apply_album_artist.clone(); }
                    if !apply_genre.is_empty()        { track.genre        = apply_genre.clone(); }
                    if !apply_year.is_empty()         { track.year         = apply_year.clone(); }
                }
                self.push_log("Applied values to all tracks.".into());
            }

            if strip.fetch_by_catno {
                let catno = catno_for_fetch;
                if !catno.is_empty() {
                    self.fetch_discogs_by_catno(catno, ctx.clone());
                }
            }

            ui.add_space(4.0);

            // Progress bar
            if let Some((done, total)) = self.progress {
                let progress = if total > 0 { done as f32 / total as f32 } else { 0.0 };
                ui.add(
                    egui::ProgressBar::new(progress)
                        .text(format!("{}/{}", done, total))
                        .desired_width(f32::INFINITY)
                );
                ui.add_space(4.0);
            }

            // Track table
            let action = show_track_table(
                ui,
                &mut self.tracks,
                &mut self.selected_rows,
                &self.worker_tx,
                ctx,
            );

            match action {
                TableAction::Edit(idx) => {
                    self.editing_track_index = Some(idx);
                }
                TableAction::Delete(idx) => {
                    self.tracks.remove(idx);
                    // Reindex track numbers
                    for (i, t) in self.tracks.iter_mut().enumerate() {
                        t.track_number = (i + 1).to_string();
                    }
                    // Close edit panel if it was open for this or a later track
                    if let Some(edit_idx) = self.editing_track_index {
                        if edit_idx >= idx {
                            self.editing_track_index = None;
                        }
                    }
                    self.selected_rows.retain(|&r| r != idx);
                }
                TableAction::AddTrack => {
                    self.manual_track_open = true;
                }
                _ => {}
            }
        });
    }

    fn show_log_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("log_panel")
            .resizable(true)
            .default_height(140.0)
            .min_height(60.0)
            .max_height(300.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Log")
                            .color(egui::Color32::from_rgb(137, 180, 250))
                            .strong()
                    );
                    if ui.small_button("Clear").clicked() {
                        self.log_messages.clear();
                    }
                });

                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for msg in &self.log_messages {
                            let color = if msg.starts_with('✗') {
                                egui::Color32::from_rgb(243, 139, 168) // red — error
                            } else if msg.starts_with('⚠') {
                                egui::Color32::from_rgb(249, 226, 175) // yellow — warning
                            } else {
                                egui::Color32::from_rgb(166, 227, 161) // green — info
                            };
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(msg)
                                        .color(color)
                                        .monospace()
                                        .size(12.0)
                                )
                                .wrap()
                            );
                        }
                    });
            });
    }

    fn show_dialogs(&mut self, ctx: &egui::Context) {
        // Discogs release picker
        if self.discogs_picker_open {
            if let Some(idx) = show_discogs_picker(
                ctx,
                &self.discogs_candidates,
                &mut self.discogs_picker_open,
            ) {
                self.fetch_release_by_candidate(idx, ctx.clone());
            }
        }

        // Settings dialog
        if self.settings_open {
            let sr = show_settings_dialog(ctx, &mut self.config, &mut self.settings_open);
            if sr.saved {
                let custom = self.config.custom_genre_dat.trim();
                let path = if custom.is_empty() {
                    None
                } else {
                    Some(std::path::Path::new(custom))
                };
                reload_genre_map(path);
            }
        }

        // Manual track dialog
        if self.manual_track_open {
            if show_manual_track_dialog(ctx, &mut self.manual_track_input, &mut self.manual_track_open) {
                if let Ok((start, end)) = self.manual_track_input.validate() {
                    let index = self.tracks.len() + 1;
                    let track = TrackMeta {
                        index,
                        start,
                        end,
                        title: self.manual_track_input.title.clone(),
                        artist: self.manual_track_input.artist.clone(),
                        album: self.manual_track_input.album.clone(),
                        track_number: index.to_string(),
                        ..Default::default()
                    };
                    self.push_log(format!(
                        "Added track: {} ({})",
                        track.title, track.display_time()
                    ));
                    self.tracks.push(track);
                    self.manual_track_input.clear();
                }
            }
        }
    }
}

impl eframe::App for VriprApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_messages();

        if self.discogs_auto_accept {
            self.discogs_auto_accept = false;
            self.fetch_release_by_candidate(0, ctx.clone());
        }

        // Convert any pending cover art bytes into a GPU texture
        if let Some(bytes) = self.pending_cover_bytes.take() {
            self.load_cover_texture(ctx, &bytes);
        }

        self.show_toolbar_panel(ctx);
        self.show_waveform_panel(ctx);
        self.show_log_panel(ctx);
        // Cover panel must be added before CentralPanel
        self.show_cover_panel(ctx);
        self.show_central_panel(ctx);
        self.show_dialogs(ctx);

        if self.is_busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

fn fmt_secs(secs: f64) -> String {
    let m = (secs / 60.0) as u32;
    let s = secs % 60.0;
    format!("{:02}:{:05.2}", m, s)
}

/// Format seconds as MM:SS.ss
fn fmt_mmss(secs: f64) -> String {
    fmt_secs(secs)
}

/// Parse "MM:SS.ss" or "SS.ss" into seconds. Returns None on invalid input.
fn parse_mmss(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some(colon) = s.find(':') {
        let mins: f64 = s[..colon].trim().parse().ok()?;
        let secs: f64 = s[colon + 1..].trim().parse().ok()?;
        if secs >= 60.0 { return None; }
        Some(mins * 60.0 + secs)
    } else {
        s.parse().ok()
    }
}

/// Pair a slice of detected audio regions with Discogs track metadata, in order.
fn pair_detected_with_meta(
    detected: &[crate::audio::DetectedTrack],
    disc_refs: &[&crate::metadata::DiscogsTrack],
    release: &crate::metadata::DiscogsRelease,
    track_number_format: &crate::config::TrackNumberFormat,
) -> Vec<crate::track::TrackMeta> {
    detected.iter().enumerate().map(|(i, dt)| {
        let dr = disc_refs.get(i);
        let track_number = match track_number_format {
            crate::config::TrackNumberFormat::Alpha =>
                dr.map(|t| format!("{}{}", t.side, t.number))
                  .unwrap_or_else(|| (i + 1).to_string()),
            crate::config::TrackNumberFormat::Numeric =>
                (i + 1).to_string(),
        };
        crate::track::TrackMeta {
            index:              i + 1,
            start:              dt.start,
            end:                dt.end,
            title:              dr.map(|t| t.title.clone()).unwrap_or_default(),
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

/// A text field that displays and accepts time in MM:SS.ss format.
/// Stores seconds as f64. Returns the inner response.
fn time_text_edit(
    ui: &mut egui::Ui,
    value: &mut f64,
    id: egui::Id,
    width: f32,
) -> egui::Response {
    let mut buf: String = ui.data(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| fmt_mmss(*value));

    let resp = ui.add_sized(
        [width, 20.0],
        egui::TextEdit::singleline(&mut buf).hint_text("MM:SS.ss"),
    );

    if resp.gained_focus() {
        buf = fmt_mmss(*value);
    }

    if resp.lost_focus() {
        if let Some(parsed) = parse_mmss(&buf) {
            *value = parsed;
        }
        // Clear the buffer so next render shows the live value
        ui.data_mut(|d| d.remove::<String>(id));
    } else if resp.has_focus() {
        // Only persist the buffer while the field is actively being edited
        ui.data_mut(|d| d.insert_temp(id, buf));
    } else {
        // Not focused — drop any stale buffer so switching tracks always shows fresh data
        ui.data_mut(|d| d.remove::<String>(id));
    }

    resp
}
