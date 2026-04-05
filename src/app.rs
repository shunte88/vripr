use std::collections::HashSet;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use std::path::PathBuf;

use crate::audio::{detect_tracks, detect_tracks_spectral, DetectorConfig};
use crate::config::{Config, DetectionMethod};
use crate::metadata::{
    assign_discogs_titles, compare_duration_report, discogs_encode_query,
    discogs_fetch_image, discogs_fetch_release, discogs_search_candidates,
    split_by_discogs_durations_fmt,
    DiscogsCandidate, DiscogsRelease,
};
use crate::pipe::AudacityPipe;
use crate::track::TrackMeta;
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

    // Cover art
    cover_texture: Option<egui::TextureHandle>,
    cover_image_bytes: Option<Vec<u8>>,   // raw bytes kept for folder.jpg export
    pending_cover_bytes: Option<Vec<u8>>, // queued for texture creation on main thread
    custom_cover_path: String,

    // Waveform display
    waveform_samples: Option<Vec<f32>>,
    waveform_duration: f64,
    waveform_drag: Option<crate::ui::WaveformDragState>,
    waveform_selection: Option<(f64, f64)>, // active selection band in seconds
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
        let (worker_tx, worker_rx) = mpsc::channel();

        VriprApp {
            config,
            tracks: Vec::new(),
            pipe: Arc::new(Mutex::new(AudacityPipe::new())),
            pipe_connected: false,
            is_busy: false,
            log_messages: vec![format!(
                "VRipr — Master Vinyl Rippage v{} ({}) ready.",
                crate::build_info::VERSION,
                crate::build_info::BUILD_DATE,
            )],
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
            apply_genre: String::new(),
            apply_year: String::new(),
            progress: None,
            discogs_release: None,
            available_sides: Vec::new(),
            selected_side: None,
            discogs_candidates: Vec::new(),
            discogs_picker_open: false,
            discogs_picker_token: String::new(),
            cover_texture: None,
            cover_image_bytes: None,
            pending_cover_bytes: None,
            custom_cover_path: String::new(),
            waveform_samples: None,
            waveform_duration: 0.0,
            waveform_drag: None,
            waveform_selection: None,
            analysis_wav: None,
        }
    }

    /// Drain and process all pending worker messages.
    fn process_messages(&mut self) {
        while let Ok(msg) = self.worker_rx.try_recv() {
            match msg {
                WorkerMessage::Log(s) => {
                    tracing::info!("[Worker] {}", s);
                    self.log_messages.push(s);
                    if self.log_messages.len() > 2000 {
                        self.log_messages.drain(0..500);
                    }
                }
                WorkerMessage::PipeConnected { info } => {
                    self.pipe_connected = true;
                    self.is_busy = false;
                    self.log_messages.push(format!("Connected to Audacity: {}", info));
                }
                WorkerMessage::PipeDisconnected => {
                    self.pipe_connected = false;
                    self.is_busy = false;
                    self.log_messages.push("Disconnected from Audacity.".into());
                }
                WorkerMessage::PipeError(e) => {
                    self.pipe_connected = false;
                    self.is_busy = false;
                    self.log_messages.push(format!("Pipe error: {}", e));
                }
                WorkerMessage::TracksDetected(tracks) => {
                    self.tracks = tracks;
                    self.selected_rows.clear();
                    self.log_messages.push(format!("Loaded {} track(s).", self.tracks.len()));
                }
                WorkerMessage::TrackUpdate { index, updates } => {
                    self.apply_track_update(index, updates);
                }
                WorkerMessage::Progress { done, total } => {
                    self.progress = Some((done, total));
                }
                WorkerMessage::WorkerError(e) => {
                    self.is_busy = false;
                    self.log_messages.push(format!("Error: {}", e));
                }
                WorkerMessage::WorkerFinished => {
                    self.is_busy = false;
                    self.progress = None;
                }
                WorkerMessage::DiscogsSearchCandidates(candidates) => {
                    self.is_busy = false;
                    if candidates.is_empty() {
                        self.log_messages.push("Discogs: no results found.".into());
                    } else {
                        self.log_messages.push(format!(
                            "Discogs: {} candidate(s) found:", candidates.len()
                        ));
                        for (i, c) in candidates.iter().enumerate() {
                            self.log_messages.push(format!("  {}. {}", i + 1, c.summary()));
                        }
                        self.discogs_candidates  = candidates;
                        self.discogs_picker_open = true;
                    }
                }
                WorkerMessage::DiscogsReleaseFetched(release) => {
                    self.log_messages.push(format!(
                        "Discogs release loaded: {} — {} ({}) — {} tracks",
                        release.album_artist, release.album, release.year, release.tracks.len()
                    ));
                    for side in release.sides() {
                        let side_tracks = release.side_tracks(side);
                        let dur_str = release.side_duration_secs(side)
                            .map(|d| format!("{:.0}s", d))
                            .unwrap_or_else(|| "?".into());
                        self.log_messages.push(format!(
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
                    self.log_messages.push(format!(
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
                self.log_messages.push(format!("Cover art decode failed: {}", e));
            }
        }
    }

    fn load_cover_from_path(&mut self, ctx: &egui::Context, path: &str) {
        match std::fs::read(path) {
            Ok(bytes) => {
                self.cover_image_bytes = Some(bytes.clone());
                self.load_cover_texture(ctx, &bytes);
            }
            Err(e) => self.log_messages.push(format!("Cover art: {}", e)),
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
            if let Some(v) = update.acoustid { track.acoustid = v; }
            if let Some(v) = update.mb_recording_id { track.mb_recording_id = v; }
            if let Some(v) = update.discogs_release_id { track.discogs_release_id = v; }
            if let Some(v) = update.fingerprint_done { track.fingerprint_done = v; }
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
        self.log_messages.push("Disconnected from Audacity.".into());
    }

    fn set_labels(&mut self, ctx: egui::Context) {
        if self.tracks.is_empty() {
            self.log_messages.push("No tracks — fetch a release first.".into());
            return;
        }
        if !self.pipe_connected {
            self.log_messages.push("Not connected to Audacity.".into());
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
                    self.log_messages.push(
                        "Set Artist and/or Album in the apply-all strip before fetching.".into()
                    );
                    return;
                }
            }
        };

        if self.config.discogs_token.is_empty() {
            self.log_messages.push("Discogs token not set — open Settings.".into());
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
            self.log_messages.push(format!("Discogs URL: {}", masked));
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

    fn fetch_release_by_candidate(&mut self, idx: usize, ctx: egui::Context) {
        let Some(c) = self.discogs_candidates.get(idx) else { return };
        let release_id   = c.id.clone();
        let token        = self.discogs_picker_token.clone();
        let tx           = self.worker_tx.clone();
        let config       = self.config.clone();
        let pipe         = self.pipe.clone();
        let analysis_wav = self.analysis_wav.clone();
        let selected_side = self.selected_side;

        self.is_busy = true;
        self.log_messages.push(format!(
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

            // Collect track durations in release order
            let durations: Vec<f64> = release.tracks.iter()
                .map(|t| t.duration_secs.unwrap_or(0.0))
                .collect();
            let valid_durs = durations.iter().filter(|&&d| d > 0.0).count();

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

            let tracks: Vec<TrackMeta> = if valid_durs == 0 {
                let _ = tx.send(WorkerMessage::Log(
                    "No Discogs durations available — generating placeholder tracks.".into()
                ));
                split_by_discogs_durations_fmt(&disc_refs, &release, 0.0, 2.0, &config.track_number_format)

            } else if let Some(path) = audio_path.filter(|p| p.exists()) {
                let _ = tx.send(WorkerMessage::Log(
                    format!("Full-file silence scan: {}", path.display())
                ));

                let expected = disc_refs.len();

                // Warn if expecting many tracks — one-side-per-session is strongly recommended
                if expected > 8 {
                    let _ = tx.send(WorkerMessage::Log(format!(
                        "⚠  Expecting {} tracks — detection is most reliable with one vinyl \
                         side per session (≤6 tracks). Consider recording one side at a time \
                         and using the Side selector for metadata assignment.",
                        expected
                    )));
                }

                // Retry schedule: progressively shorter silence thresholds.
                // IMPORTANT: gap_fill must always be < min_silence or it will
                // bridge the very gap we're trying to detect.
                // Format: (min_silence_secs, min_sound_secs, gap_fill_secs)
                let retry_params: &[(f64, f64, f64)] = &[
                    (0.70, 15.0, 0.25),
                    (0.40, 10.0, 0.15),
                    (0.20,  5.0, 0.07),
                    (0.10,  3.0, 0.04),
                    (0.05,  2.0, 0.02),
                ];

                let mut best_detected: Vec<crate::audio::DetectedTrack> = Vec::new();
                let mut best_diag: Option<crate::audio::DetectorDiagnostics> = None;

                let use_spectral = config.detection_method == DetectionMethod::Spectral;

                for (attempt, &(min_sil, min_snd, gap_fill)) in retry_params.iter().enumerate() {
                    let det_cfg = DetectorConfig {
                        threshold_db:               config.silence_threshold_db,
                        adaptive:                   config.use_adaptive_threshold,
                        adaptive_margin_db:         config.adaptive_margin_db,
                        min_silence_secs:           min_sil,
                        min_sound_secs:             min_snd,
                        gap_fill_secs:              gap_fill,
                        window_ms:                  50,
                        spectral_flatness_threshold: config.spectral_flatness_threshold,
                        ..DetectorConfig::default()
                    };

                    let path2 = path.clone();
                    let tx2   = tx.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        if use_spectral {
                            detect_tracks_spectral(&path2, &det_cfg, &mut |_| {
                                let _ = tx2.send(WorkerMessage::Log(String::new()));
                            })
                        } else {
                            detect_tracks(&path2, &det_cfg, &mut |_| {
                                let _ = tx2.send(WorkerMessage::Log(String::new()));
                            })
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
                            best_diag     = Some(diag);

                            if found == expected {
                                break; // exact match — done
                            }
                        }
                        Ok(Err(e)) => {
                            let _ = tx.send(WorkerMessage::Log(
                                format!("Silence scan failed ({}), using Discogs durations.", e)
                            ));
                            break;
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerMessage::Log(
                                format!("Scan task error ({}), using Discogs durations.", e)
                            ));
                            break;
                        }
                    }
                }

                if best_detected.is_empty() {
                    let _ = tx.send(WorkerMessage::Log(
                        "No sound regions detected — using Discogs durations.".into()
                    ));
                    split_by_discogs_durations_fmt(&disc_refs, &release, 0.0, 2.0, &config.track_number_format)
                } else {
                    let found = best_detected.len();
                    if found != expected {
                        let _ = tx.send(WorkerMessage::Log(format!(
                            "  ⚠ count mismatch after retries: {} detected vs {} Discogs — \
                             adjust threshold in Settings if needed",
                            found, expected
                        )));
                    } else {
                        let _ = tx.send(WorkerMessage::Log(format!(
                            "  ✓ {} track(s) matched Discogs count", found
                        )));
                    }

                    // Pair detected regions with Discogs metadata in order.
                    best_detected.iter().enumerate().map(|(i, dt)| {
                        let dr = disc_refs.get(i);
                        let track_number = match config.track_number_format {
                            crate::config::TrackNumberFormat::Alpha =>
                                dr.map(|t| format!("{}{}", t.side, t.number))
                                  .unwrap_or_else(|| (i + 1).to_string()),
                            crate::config::TrackNumberFormat::Numeric =>
                                (i + 1).to_string(),
                        };
                        TrackMeta {
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
            } else {
                let _ = tx.send(WorkerMessage::Log(
                    "No audio file found — using Discogs durations for track times.".into()
                ));
                split_by_discogs_durations_fmt(&disc_refs, &release, 0.0, 2.0, &config.track_number_format)
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

    fn split_by_durations(&mut self) {
        let Some(release) = self.discogs_release.clone() else { return };

        if self.tracks.is_empty() {
            let disc_refs: Vec<&crate::metadata::DiscogsTrack> =
                release.tracks.iter().collect();
            let new_tracks = split_by_discogs_durations_fmt(
                &disc_refs, &release, 0.0, 2.0, &self.config.track_number_format
            );
            self.log_messages.push(format!(
                "Generated {} track(s) from Discogs durations.",
                new_tracks.len()
            ));
            self.tracks = new_tracks;
            self.selected_rows.clear();
        } else {
            let disc_refs: Vec<&crate::metadata::DiscogsTrack> =
                release.tracks.iter().collect();
            assign_discogs_titles(&mut self.tracks, &disc_refs, &release);
            self.log_messages.push(format!(
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
            self.log_messages.push(format!("Side → {} (no release loaded; fetch a release first)", label));
            return;
        };

        let disc_refs: Vec<&crate::metadata::DiscogsTrack> = match side {
            None    => release.tracks.iter().collect(),
            Some(s) => release.tracks.iter().filter(|t| t.side == s).collect(),
        };

        if disc_refs.is_empty() {
            self.log_messages.push(format!("Side {}: no tracks found in release", side.unwrap_or('?')));
            return;
        }

        crate::metadata::assign_discogs_titles(&mut self.tracks, &disc_refs, release);
        self.log_messages.push(format!(
            "Side filter → {} — reassigned metadata for {} track(s) from {} Discogs track(s).",
            label,
            self.tracks.len().min(disc_refs.len()),
            disc_refs.len(),
        ));
    }

    fn rescan(&mut self, ctx: egui::Context) {
        let Some(wav_path) = self.analysis_wav.clone() else {
            self.log_messages.push("No analysis WAV — connect first.".into());
            return;
        };
        if !wav_path.exists() {
            self.log_messages.push("Analysis WAV not found — reconnect to re-export.".into());
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
            let use_spectral = config.detection_method == DetectionMethod::Spectral;

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
                let path2 = wav_path.clone();
                let tx2   = tx.clone();
                let result = tokio::task::spawn_blocking(move || {
                    if use_spectral {
                        detect_tracks_spectral(&path2, &det_cfg, &mut |_| {
                            let _ = tx2.send(WorkerMessage::Log(String::new()));
                        })
                    } else {
                        detect_tracks(&path2, &det_cfg, &mut |_| {
                            let _ = tx2.send(WorkerMessage::Log(String::new()));
                        })
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
        self.log_messages.push("=== Diagnostics ===".into());
        self.log_messages.push(format!(
            "Config path: {:?}",
            crate::config::config_path()
        ));
        self.log_messages.push(format!(
            "Pipe paths: {:?} | {:?}",
            AudacityPipe::pipe_paths().0,
            AudacityPipe::pipe_paths().1
        ));
        self.log_messages.push(format!(
            "Pipes exist: {}",
            AudacityPipe::check_pipes()
        ));
        self.log_messages.push(format!("Pipe connected: {}", self.pipe_connected));
        self.log_messages.push(format!("Tracks loaded: {}", self.tracks.len()));
        self.log_messages.push(format!("Export dir: {:?}", self.config.export_dir));
        self.log_messages.push(format!(
            "Discogs token set: {}",
            !self.config.discogs_token.is_empty()
        ));
        self.log_messages.push(format!(
            "Cover art: {}",
            if self.cover_texture.is_some() { "loaded" } else { "none" }
        ));
        self.log_messages.push("=== End Diagnostics ===".into());
    }

    fn show_waveform_panel(&mut self, ctx: &egui::Context) {
        // Extract data before any borrows of self (avoids borrow conflicts)
        let wf_data = self.waveform_samples.as_ref().map(|s| {
            (s.clone(), self.waveform_duration, self.waveform_drag.clone())
        });
        let Some((samples, duration, mut drag)) = wf_data else { return };

        let track_bounds: Vec<(usize, f64, f64, bool)> = self.tracks.iter()
            .map(|t| (t.index, t.start, t.end, t.pinned))
            .collect();

        let mut sel = self.waveform_selection;

        let evt = crate::ui::waveform::show_waveform(
            ctx, &samples, duration, &track_bounds, &mut drag, &mut sel,
        );

        // Write back persistent state
        self.waveform_drag      = drag;
        self.waveform_selection = sel;

        // Handle pin toggle
        if let Some(vi) = evt.toggle_pin {
            if let Some(track) = self.tracks.get_mut(vi) {
                track.pinned = !track.pinned;
                self.log_messages.push(format!(
                    "Track {}: {}", track.index,
                    if track.pinned { "pinned 📌" } else { "unpinned" }
                ));
            }
        }

        // Apply boundary drag
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
                        self.log_messages.push("Tracks cleared.".into());
                    }
                    ToolbarAction::FetchDiscogsRelease => self.fetch_discogs_release(ctx.clone()),
                    ToolbarAction::SideChanged(side)  => self.apply_side_filter(side),
                }
            }
        });
    }

    fn show_cover_panel(&mut self, ctx: &egui::Context) {
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
            });
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

            if show_apply_all_strip(
                ui,
                apply_artist,
                apply_album,
                apply_album_artist,
                apply_genre,
                apply_year,
            ) {
                for track in &mut self.tracks {
                    if !apply_artist.is_empty()       { track.artist       = apply_artist.clone(); }
                    if !apply_album.is_empty()        { track.album        = apply_album.clone(); }
                    if !apply_album_artist.is_empty() { track.album_artist = apply_album_artist.clone(); }
                    if !apply_genre.is_empty()        { track.genre        = apply_genre.clone(); }
                    if !apply_year.is_empty()         { track.year         = apply_year.clone(); }
                }
                self.log_messages.push("Applied values to all tracks.".into());
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
                TableAction::Remove(idx) => {
                    if idx < self.tracks.len() {
                        let removed = self.tracks.remove(idx);
                        self.selected_rows.remove(&idx);
                        self.log_messages.push(format!("Removed track: {}", removed.title));
                        for (i, t) in self.tracks.iter_mut().enumerate() {
                            t.index = i + 1;
                            if t.track_number.parse::<usize>().ok() == Some(idx + 1) {
                                t.track_number = (i + 1).to_string();
                            }
                        }
                    }
                }
                TableAction::MoveUp(idx) => {
                    if idx > 0 && idx < self.tracks.len() {
                        self.tracks.swap(idx - 1, idx);
                        self.selected_rows.clear();
                        self.selected_rows.insert(idx - 1);
                    }
                }
                TableAction::MoveDown(idx) => {
                    if idx + 1 < self.tracks.len() {
                        self.tracks.swap(idx, idx + 1);
                        self.selected_rows.clear();
                        self.selected_rows.insert(idx + 1);
                    }
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
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(msg)
                                        .color(egui::Color32::from_rgb(166, 227, 161))
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
            show_settings_dialog(ctx, &mut self.config, &mut self.settings_open);
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
                    self.log_messages.push(format!(
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
