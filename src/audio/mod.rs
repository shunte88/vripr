use anyhow::{anyhow, Result};
use rustfft::{num_complex::Complex, FftPlanner};
use std::path::Path;
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parameters for the local silence detector.
#[derive(Debug, Clone)]
pub struct DetectorConfig {
    /// Manual dB threshold. Below this level = silence. (e.g. -40.0)
    pub threshold_db: f64,
    /// If true, compute noise floor from the audio and derive threshold automatically.
    pub adaptive: bool,
    /// When adaptive: threshold = noise_floor_db + this margin. (default 12.0)
    pub adaptive_margin_db: f64,
    /// Hysteresis band in dB. Silence is entered when level drops to
    /// (threshold − hysteresis) and exited when level rises back to threshold.
    /// Prevents rapid toggling on borderline signals. (default 6.0)
    pub hysteresis_db: f64,
    /// Gaps between sounds shorter than this are bridged (handles vinyl pops/crackle).
    pub gap_fill_secs: f64,
    /// Minimum silence duration to count as a track boundary. (user-tunable)
    pub min_silence_secs: f64,
    /// Minimum sound duration to be labelled as a track. (user-tunable)
    pub min_sound_secs: f64,
    /// Seconds added before each detected sound start.
    pub pre_padding: f64,
    /// Seconds added after each detected sound end.
    pub post_padding: f64,
    /// Analysis window size in milliseconds. (default 100)
    pub window_ms: u32,
    /// Spectral flatness above which a frame is considered noise/between-tracks.
    /// Range 0.0–1.0; 0.0 = perfectly tonal, 1.0 = white noise.
    /// Only used by `detect_tracks_spectral`. (default 0.85)
    pub spectral_flatness_threshold: f64,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        DetectorConfig {
            threshold_db:      -40.0,
            adaptive:          false,
            adaptive_margin_db: 12.0,
            hysteresis_db:     6.0,
            gap_fill_secs:     0.2,
            min_silence_secs:  0.8,
            min_sound_secs:    2.0,
            pre_padding:       0.1,
            post_padding:      0.1,
            window_ms:                    100,
        spectral_flatness_threshold:  0.85,
        }
    }
}

/// Detected track region in seconds.
#[derive(Debug, Clone)]
pub struct DetectedTrack {
    pub start: f64,
    pub end: f64,
}

/// Run the full detection pipeline on an audio file.
///
/// `progress` receives values in 0.0–1.0 as decoding progresses.
pub fn detect_tracks(
    path: &Path,
    cfg: &DetectorConfig,
    progress: &mut impl FnMut(f64),
) -> Result<(Vec<DetectedTrack>, DetectorDiagnostics)> {
    let window_secs = cfg.window_ms as f64 / 1000.0;

    // --- Phase 1: decode to per-window RMS ---
    let (rms_linear, sample_rate, total_frames) =
        decode_rms_windows(path, window_secs, progress)?;

    let total_secs = total_frames as f64 / sample_rate as f64;
    let n_windows = rms_linear.len();
    debug!("Decoded {} windows, {:.1}s total", n_windows, total_secs);

    if n_windows == 0 {
        return Err(anyhow!("Audio file produced no samples"));
    }

    // --- Phase 2: determine threshold ---
    let (threshold_db, noise_floor_db) = if cfg.adaptive {
        let nf = adaptive_noise_floor(&rms_linear);
        let nf_db = linear_to_db(nf);
        let thr_db = nf_db + cfg.adaptive_margin_db;
        info!(
            "Adaptive threshold: noise floor = {:.1} dB, threshold = {:.1} dB",
            nf_db, thr_db
        );
        (thr_db, Some(nf_db))
    } else {
        (cfg.threshold_db, None)
    };

    let thr_lin       = db_to_linear(threshold_db);
    let thr_entry_lin = db_to_linear(threshold_db - cfg.hysteresis_db);

    // --- Phase 3: state machine → raw sound regions ---
    let mut sound_regions: Vec<(f64, f64)> = Vec::new();
    let mut in_sound = false;
    let mut sound_start = 0.0f64;

    for (i, &rms) in rms_linear.iter().enumerate() {
        let t = i as f64 * window_secs;
        if !in_sound {
            if rms >= thr_lin {
                in_sound = true;
                sound_start = t;
            }
        } else if rms < thr_entry_lin {
            in_sound = false;
            sound_regions.push((sound_start, t));
        }
    }
    // close any open region
    if in_sound {
        sound_regions.push((sound_start, total_secs));
    }

    debug!("Raw sound regions: {}", sound_regions.len());

    // --- Phase 4: gap fill — bridge tiny silences (vinyl crackle/pops) ---
    let sound_regions = merge_gaps(sound_regions, cfg.gap_fill_secs);
    debug!("After gap fill: {}", sound_regions.len());

    // --- Phase 5: merge regions separated by less than min_silence_secs ---
    let sound_regions = merge_gaps(sound_regions, cfg.min_silence_secs);
    debug!("After min-silence merge: {}", sound_regions.len());

    // --- Phase 6: filter out sounds shorter than min_sound_secs ---
    let sound_regions: Vec<(f64, f64)> = sound_regions
        .into_iter()
        .filter(|(s, e)| (e - s) >= cfg.min_sound_secs)
        .collect();
    debug!("After min-sound filter: {}", sound_regions.len());

    // --- Phase 7: apply pre/post padding, clamp, de-overlap ---
    let n = sound_regions.len();
    let tracks: Vec<DetectedTrack> = sound_regions
        .into_iter()
        .enumerate()
        .map(|(i, (s, e))| {
            let padded_start = (s - cfg.pre_padding).max(0.0);
            let padded_end   = (e + cfg.post_padding).min(total_secs);
            // Don't overlap previous track's end or next track's start
            let clamped_start = if i == 0 {
                padded_start
            } else {
                padded_start // neighbour clamping handled below
            };
            let clamped_end = padded_end.min(total_secs);
            DetectedTrack { start: clamped_start, end: clamped_end }
        })
        .collect();

    // Clamp neighbours so they don't overlap
    let mut tracks: Vec<DetectedTrack> = tracks;
    for i in 1..tracks.len() {
        if tracks[i].start < tracks[i - 1].end {
            let mid = (tracks[i - 1].end + tracks[i].start) / 2.0;
            tracks[i - 1].end = mid;
            tracks[i].start = mid;
        }
    }

    let diag = DetectorDiagnostics {
        threshold_db,
        noise_floor_db,
        total_secs,
        n_windows,
        window_secs,
    };

    Ok((tracks, diag))
}

/// Spectral-flatness track detector.
///
/// Uses the same `DetectorConfig` as `detect_tracks` (threshold, min_silence, gap_fill, etc.)
/// but operates on a combined energy + spectral-flatness signal rather than raw RMS alone.
///
/// A frame is classified as "between tracks" if either:
/// - Its RMS energy falls below the threshold (ordinary silence), OR
/// - Its spectral flatness exceeds `cfg.spectral_flatness_threshold` while energy is still
///   present (loud surface noise that looks spectrally flat, unlike tonal music).
///
/// This makes it significantly more robust than pure RMS on noisy pressings where the
/// inter-track groove noise is loud enough to fool the energy threshold.
pub fn detect_tracks_spectral(
    path: &Path,
    cfg: &DetectorConfig,
    progress: &mut impl FnMut(f64),
) -> Result<(Vec<DetectedTrack>, DetectorDiagnostics)> {
    let window_secs = cfg.window_ms as f64 / 1000.0;

    let (windows, sample_rate, total_frames) =
        decode_spectral_windows(path, window_secs, progress)?;

    let total_secs = total_frames as f64 / sample_rate as f64;
    let n_windows  = windows.len();

    if n_windows == 0 {
        return Err(anyhow!("Audio file produced no samples"));
    }

    // --- Determine energy threshold (same adaptive logic as detect_tracks) ---
    let rms_vals: Vec<f64> = windows.iter().map(|(r, _)| *r).collect();
    let (threshold_db, noise_floor_db) = if cfg.adaptive {
        let nf    = adaptive_noise_floor(&rms_vals);
        let nf_db = linear_to_db(nf);
        let thr   = nf_db + cfg.adaptive_margin_db;
        info!(
            "Spectral detector: adaptive threshold = {:.1} dB (noise floor {:.1} dB)",
            thr, nf_db
        );
        (thr, Some(nf_db))
    } else {
        (cfg.threshold_db, None)
    };

    let thr_lin       = db_to_linear(threshold_db);
    let thr_entry_lin = db_to_linear(threshold_db - cfg.hysteresis_db);
    let flat_thr      = cfg.spectral_flatness_threshold;

    // --- Smooth flatness with a ±3-window rolling average (≈350 ms at 50 ms windows) ---
    let smooth_r = 3usize;
    let flatness_raw: Vec<f64> = windows.iter().map(|(_, f)| *f).collect();
    let flatness: Vec<f64> = (0..n_windows)
        .map(|i| {
            let lo = i.saturating_sub(smooth_r);
            let hi = (i + smooth_r + 1).min(n_windows);
            flatness_raw[lo..hi].iter().sum::<f64>() / (hi - lo) as f64
        })
        .collect();

    // --- State machine ---
    // "between tracks": low energy (silent) OR high flatness with present energy (surface noise)
    let mut sound_regions: Vec<(f64, f64)> = Vec::new();
    let mut in_sound   = false;
    let mut sound_start = 0.0f64;

    for (i, &(rms, _)) in windows.iter().enumerate() {
        let t    = i as f64 * window_secs;
        let flat = flatness[i];

        let is_between = if rms < thr_entry_lin {
            true              // ordinary silence / very quiet
        } else {
            flat > flat_thr   // energetic but spectrally flat = surface noise
        };

        if !in_sound {
            if !is_between {
                in_sound    = true;
                sound_start = t;
            }
        } else if is_between {
            in_sound = false;
            sound_regions.push((sound_start, t));
        }
    }
    if in_sound {
        sound_regions.push((sound_start, total_secs));
    }

    debug!("Spectral raw regions: {}", sound_regions.len());

    // --- Post-processing: transient filter, gap fill, min-silence merge, min-sound filter ---
    // Drop very short "music" bursts (pops / transients that briefly pass the flatness test)
    // before gap-filling — otherwise they get bridged into adjacent regions.
    let min_transient_secs = 0.35;
    let sound_regions: Vec<(f64, f64)> = sound_regions
        .into_iter()
        .filter(|(s, e)| (e - s) >= min_transient_secs)
        .collect();
    debug!("After transient filter: {}", sound_regions.len());

    let sound_regions = merge_gaps(sound_regions, cfg.gap_fill_secs);
    let sound_regions = merge_gaps(sound_regions, cfg.min_silence_secs);
    let sound_regions: Vec<(f64, f64)> = sound_regions
        .into_iter()
        .filter(|(s, e)| (e - s) >= cfg.min_sound_secs)
        .collect();

    let mut tracks: Vec<DetectedTrack> = sound_regions
        .into_iter()
        .map(|(s, e)| DetectedTrack {
            start: (s - cfg.pre_padding).max(0.0),
            end:   (e + cfg.post_padding).min(total_secs),
        })
        .collect();

    for i in 1..tracks.len() {
        if tracks[i].start < tracks[i - 1].end {
            let mid = (tracks[i - 1].end + tracks[i].start) / 2.0;
            tracks[i - 1].end = mid;
            tracks[i].start   = mid;
        }
    }

    let diag = DetectorDiagnostics {
        threshold_db,
        noise_floor_db,
        total_secs,
        n_windows,
        window_secs,
    };

    Ok((tracks, diag))
}

/// Diagnostic info returned alongside detected tracks.
#[derive(Debug, Clone)]
pub struct DetectorDiagnostics {
    pub threshold_db: f64,
    pub noise_floor_db: Option<f64>,
    pub total_secs: f64,
    pub n_windows: usize,
    pub window_secs: f64,
}

/// Produce a downsampled amplitude waveform suitable for display.
///
/// Returns `(bars, duration_secs)` where `bars` has exactly `n_bars` entries
/// in `0.0–1.0` (normalised linear peak amplitude per bucket).
pub fn compute_waveform_display(path: &Path, n_bars: usize) -> Result<(Vec<f32>, f64)> {
    let window_secs = 0.05; // 50 ms windows — fine enough for display
    let (rms, sr, total_frames) = decode_rms_windows(path, window_secs, &mut |_| {})?;
    let total_secs = total_frames as f64 / sr as f64;

    if rms.is_empty() {
        return Ok((vec![0.0f32; n_bars.max(1)], total_secs));
    }

    let peak = rms.iter().cloned().fold(1e-10f64, f64::max);
    let n    = rms.len();
    let n_bars = n_bars.max(1);

    let bars: Vec<f32> = (0..n_bars)
        .map(|b| {
            let lo = b * n / n_bars;
            let hi = ((b + 1) * n / n_bars).min(n);
            if lo >= hi { return 0.0f32; }
            let pk = rms[lo..hi].iter().cloned().fold(0.0f64, f64::max);
            (pk / peak) as f32
        })
        .collect();

    Ok((bars, total_secs))
}

// ---------------------------------------------------------------------------
// Discogs-guided track detection
// ---------------------------------------------------------------------------

/// Configuration for duration-guided boundary finding.
#[derive(Debug, Clone)]
pub struct GuidedDetectorConfig {
    /// How far (seconds) ahead of `cursor` to look for the next sound onset.
    pub onset_search_secs: f64,
    /// How many seconds before the expected track end to begin the offset search.
    /// Keep this small (4–8 s) so the scan doesn't enter quiet mid-track passages.
    pub offset_lookback_secs: f64,
    /// How many seconds past the expected track end to continue the offset search,
    /// to handle rips that run slightly longer than the Discogs duration.
    pub offset_lookahead_secs: f64,
    /// RMS threshold in dB — below = silence.
    pub threshold_db: f64,
    /// Use adaptive noise-floor for threshold.
    pub adaptive: bool,
    /// When adaptive: threshold = noise_floor + margin.
    pub adaptive_margin_db: f64,
    /// Minimum silence duration (seconds) to accept as a track boundary.
    pub min_silence_secs: f64,
    /// Minimum sustained duration (seconds) above threshold before declaring onset.
    /// Protects against vinyl crackle / lead-in pops being mistaken for music.
    pub min_onset_secs: f64,
    /// Number of below-threshold windows tolerated within an onset run without
    /// resetting the counter.  Prevents a single crackle dropout mid-attack from
    /// delaying the detected start.
    pub onset_hysteresis_windows: usize,
    /// Seconds of pre-roll added before the detected start.
    pub pre_padding: f64,
    /// Seconds of post-roll added after the detected end.
    pub post_padding: f64,
    /// Analysis window size in milliseconds (smaller = finer edge resolution).
    pub window_ms: u32,
}

impl Default for GuidedDetectorConfig {
    fn default() -> Self {
        GuidedDetectorConfig {
            onset_search_secs:      30.0,
            offset_lookback_secs:    6.0,
            offset_lookahead_secs:  12.0,
            threshold_db:           -40.0,
            adaptive:               true,
            adaptive_margin_db:     12.0,
            min_silence_secs:        0.8,
            min_onset_secs:          0.1,
            onset_hysteresis_windows: 2,
            pre_padding:             0.1,
            post_padding:            0.1,
            window_ms:               50,
        }
    }
}

/// Guided detection: use Discogs track durations as an anchor for finding
/// the real silence boundaries in the audio.
///
/// `discogs_durations` — expected duration in seconds for each track, in order.
///  Tracks with duration ≤ 0 are skipped.
///
/// Returns one `DetectedTrack` per duration entry that could be matched.
pub fn detect_tracks_guided(
    path: &Path,
    discogs_durations: &[f64],
    cfg: &GuidedDetectorConfig,
    progress: &mut impl FnMut(f64),
) -> Result<Vec<DetectedTrack>> {
    let window_secs = cfg.window_ms as f64 / 1000.0;

    let (rms, sample_rate, total_frames) =
        decode_rms_windows(path, window_secs, progress)?;
    let total_secs = total_frames as f64 / sample_rate as f64;
    let n = rms.len();

    if n == 0 {
        return Err(anyhow!("Audio file produced no samples"));
    }

    // Threshold
    let threshold_db = if cfg.adaptive {
        let nf    = adaptive_noise_floor(&rms);
        let nf_db = linear_to_db(nf);
        let thr   = nf_db + cfg.adaptive_margin_db;
        info!("Guided: adaptive threshold = {:.1} dB (noise floor {:.1} dB)", thr, nf_db);
        thr
    } else {
        cfg.threshold_db
    };
    let thr = db_to_linear(threshold_db);

    let min_sil_wins   = ((cfg.min_silence_secs / window_secs) as usize).max(1);
    let min_onset_wins = ((cfg.min_onset_secs   / window_secs) as usize).max(1);

    let secs_to_win = |s: f64| -> usize { (s / window_secs) as usize };
    let win_to_secs = |w: usize| -> f64 { w as f64 * window_secs };

    let mut tracks = Vec::new();
    let mut cursor = 0.0f64; // advances to actual_end after each track

    for (ti, &expected_dur) in discogs_durations.iter().enumerate() {
        if expected_dur <= 0.0 { continue; }

        // ---- 1. Find actual start (silence → audio) ------------------------------
        // Search forward from cursor for a sustained onset (not a crackle pop).
        let s_from = secs_to_win(cursor).min(n);
        let s_to   = secs_to_win(cursor + cfg.onset_search_secs).min(n);

        let actual_start = guide_find_onset(
            &rms, s_from, s_to, thr, min_onset_wins, cfg.onset_hysteresis_windows
        )
        .map(win_to_secs)
        .unwrap_or(cursor);

        // ---- 2. Find actual end (audio → silence) --------------------------------
        // expected_end is anchored from the DETECTED start, not the cursor, so
        // per-track Discogs drift doesn't carry forward.
        let expected_end = actual_start + expected_dur;

        // Start the search close to the expected end (not deep inside the track).
        // Clamp so we never search before actual_start.
        let search_start = (expected_end - cfg.offset_lookback_secs)
            .max(actual_start + expected_dur * 0.5) // never earlier than halfway
            .max(actual_start);
        let search_end   = (expected_end + cfg.offset_lookahead_secs).min(total_secs);

        let e_from = secs_to_win(search_start).min(n);
        let e_to   = secs_to_win(search_end).min(n);

        let offset_hit = guide_find_offset(&rms, e_from, e_to, thr, min_sil_wins);
        let actual_end = offset_hit
            .map(win_to_secs)
            .unwrap_or(expected_end)
            .min(total_secs);

        // ---- 3. Apply padding and store -----------------------------------------
        let padded_start = (actual_start - cfg.pre_padding).max(0.0);
        let padded_end   = (actual_end   + cfg.post_padding).min(total_secs);

        debug!(
            "Guided track {}: cursor={:.1}s → start={:.1}s, \
             end={:.1}s ({}), dur={:.1}s (discogs {:.1}s)",
            ti + 1,
            cursor,
            actual_start,
            actual_end,
            if offset_hit.is_some() { "detected" } else { "fallback" },
            actual_end - actual_start,
            expected_dur,
        );

        tracks.push(DetectedTrack { start: padded_start, end: padded_end });

        // Advance cursor to the detected silence boundary.  On fallback, use the
        // search_end so we don't re-scan the same audio on the next track.
        cursor = if offset_hit.is_some() { actual_end } else { search_end };
    }

    // Prevent overlaps
    for i in 1..tracks.len() {
        if tracks[i].start < tracks[i - 1].end {
            let mid = (tracks[i - 1].end + tracks[i].start) / 2.0;
            tracks[i - 1].end = mid;
            tracks[i].start   = mid;
        }
    }

    Ok(tracks)
}

/// Onset-only guided detection: for each Discogs track duration, walk the audio
/// forward from the previous track's end to find where the next track actually starts.
///
/// `end = actual_start + discogs_duration` — no offset search needed.
///
/// Returns `(actual_start, discogs_duration)` for each track with a known duration.
pub fn detect_track_starts(
    path: &Path,
    durations: &[f64],
    cfg: &GuidedDetectorConfig,
    progress: &mut impl FnMut(f64),
) -> Result<Vec<(f64, f64)>> {
    let window_secs = cfg.window_ms as f64 / 1000.0;

    let (rms, sample_rate, total_frames) =
        decode_rms_windows(path, window_secs, progress)?;
    let total_secs = total_frames as f64 / sample_rate as f64;
    let n = rms.len();

    if n == 0 {
        return Err(anyhow!("Audio file produced no samples"));
    }

    let threshold_db = if cfg.adaptive {
        let nf    = adaptive_noise_floor(&rms);
        let nf_db = linear_to_db(nf);
        let thr   = nf_db + cfg.adaptive_margin_db;
        info!("Onset detection: adaptive threshold = {:.1} dB (noise floor {:.1} dB)", thr, nf_db);
        thr
    } else {
        cfg.threshold_db
    };
    let thr = db_to_linear(threshold_db);

    let min_onset_wins = ((cfg.min_onset_secs / window_secs) as usize).max(1);

    let secs_to_win = |s: f64| -> usize { (s / window_secs) as usize };
    let win_to_secs = |w: usize| -> f64 { w as f64 * window_secs };

    let mut results  = Vec::new();
    let mut cursor   = 0.0f64; // advances to (actual_start + dur) after each track

    for (ti, &dur) in durations.iter().enumerate() {
        if dur <= 0.0 { continue; }

        // Search from cursor forward for the first sustained sound onset
        let s_from = secs_to_win(cursor).min(n);
        let s_to   = secs_to_win(cursor + cfg.onset_search_secs).min(n);

        let actual_start = guide_find_onset(
            &rms, s_from, s_to, thr, min_onset_wins, cfg.onset_hysteresis_windows
        )
        .map(|w| (win_to_secs(w) - cfg.pre_padding).max(0.0))
        .unwrap_or(cursor);

        let end = (actual_start + dur).min(total_secs);

        debug!(
            "Onset track {}: cursor={:.1}s → start={:.1}s, end={:.1}s (dur={:.1}s)",
            ti + 1, cursor, actual_start, end, dur
        );

        results.push((actual_start, dur));
        cursor = end;
    }

    Ok(results)
}

/// Find the first window index in `[from, to)` where at least `min_windows`
/// windows above `threshold` are found within a run.
///
/// `hysteresis` allows that many consecutive below-threshold windows within a
/// run without resetting the counter — this prevents a single crackle dropout
/// during a genuine musical attack from restarting the onset search.
///
/// Returns the index of the *first* above-threshold window in the qualifying
/// run (the true onset edge).
fn guide_find_onset(
    rms: &[f64],
    from: usize,
    to: usize,
    threshold: f64,
    min_windows: usize,
    hysteresis: usize,
) -> Option<usize> {
    let limit       = to.min(rms.len());
    let min_windows = min_windows.max(1);

    let mut run_start:   Option<usize> = None;
    let mut above_count: usize         = 0;
    let mut below_run:   usize         = 0; // consecutive below-threshold windows

    for i in from..limit {
        if rms[i] >= threshold {
            if run_start.is_none() {
                run_start = Some(i);
            }
            above_count += 1;
            below_run    = 0;
            if above_count >= min_windows {
                return run_start;
            }
        } else {
            below_run += 1;
            if below_run > hysteresis {
                // True silence — reset the run entirely
                run_start   = None;
                above_count = 0;
                below_run   = 0;
            }
            // else: within hysteresis tolerance, keep accumulating
        }
    }
    None
}

/// Find the first window index in `[from, to)` where `min_windows` consecutive
/// windows are all below `threshold` — i.e., where a silence region begins.
fn guide_find_offset(
    rms: &[f64],
    from: usize,
    to: usize,
    threshold: f64,
    min_windows: usize,
) -> Option<usize> {
    let limit = to.min(rms.len());
    let mut silent_run = 0usize;
    let mut run_start  = None;

    for i in from..limit {
        if rms[i] < threshold {
            if silent_run == 0 { run_start = Some(i); }
            silent_run += 1;
            if silent_run >= min_windows {
                return run_start;
            }
        } else {
            silent_run = 0;
            run_start  = None;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Merge adjacent regions where the gap between them is < `max_gap_secs`.
fn merge_gaps(mut regions: Vec<(f64, f64)>, max_gap_secs: f64) -> Vec<(f64, f64)> {
    if regions.len() < 2 {
        return regions;
    }
    let mut merged: Vec<(f64, f64)> = Vec::with_capacity(regions.len());
    merged.push(regions.remove(0));
    for (s, e) in regions {
        let last = merged.last_mut().unwrap();
        if s - last.1 < max_gap_secs {
            last.1 = last.1.max(e); // extend
        } else {
            merged.push((s, e));
        }
    }
    merged
}

fn db_to_linear(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

fn linear_to_db(lin: f64) -> f64 {
    if lin <= 0.0 { return -120.0; }
    20.0 * lin.log10()
}

/// Estimate the noise floor as the 3rd-percentile RMS value.
///
/// The 10th percentile is unreliable for dense LPs where music occupies 90 %+
/// of the runtime — it lands inside quiet music, not in the inter-track groove.
/// The 3rd percentile is low enough to capture true groove noise while still
/// being robust to brief absolute-silence sections.  Interpolated to avoid
/// integer-truncation bias.
fn adaptive_noise_floor(rms: &[f64]) -> f64 {
    let mut sorted: Vec<f64> = rms.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    if n == 0 { return 1e-10; }

    // Interpolated 3rd-percentile
    let frac_idx = (n - 1) as f64 * 0.03;
    let lo  = frac_idx.floor() as usize;
    let hi  = (lo + 1).min(n - 1);
    let t   = frac_idx - lo as f64;
    let val = sorted[lo] * (1.0 - t) + sorted[hi] * t;
    val.max(1e-10)
}

// ---------------------------------------------------------------------------
// Audio decoding via Symphonia
// ---------------------------------------------------------------------------

/// Decode an audio file and return per-window RMS (linear), sample rate,
/// and total frame count.
fn decode_rms_windows(
    path: &Path,
    window_secs: f64,
    progress: &mut impl FnMut(f64),
) -> Result<(Vec<f64>, u32, u64)> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::errors::Error as SErr;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let src = std::fs::File::open(path)
        .map_err(|e| anyhow!("Cannot open audio file {:?}: {}", path, e))?;
    let mss = MediaSourceStream::new(Box::new(src), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| anyhow!("Unsupported audio format: {}", e))?;

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("No decodable audio track found in {:?}", path))?;

    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let n_channels  = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);
    let total_frames = track.codec_params.n_frames.unwrap_or(0);
    let track_id    = track.id;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| anyhow!("Cannot create decoder: {}", e))?;

    let frames_per_window = ((sample_rate as f64) * window_secs) as usize;
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    // Accumulate squared samples for the current window
    let mut win_sum_sq = 0.0f64;
    let mut win_frames = 0usize;
    let mut rms_windows: Vec<f64> = Vec::new();
    let mut total_decoded: u64 = 0;

    loop {
        let packet = match format.next_packet() {
            Ok(p)                      => p,
            Err(SErr::IoError(_))      => break,
            Err(SErr::ResetRequired)   => { decoder.reset(); continue; }
            Err(e)                     => return Err(e.into()),
        };

        if packet.track_id() != track_id { continue; }

        let decoded = match decoder.decode(&packet) {
            Ok(d)                          => d,
            Err(SErr::DecodeError(_))      => continue,
            Err(SErr::IoError(_))          => break,
            Err(e)                         => return Err(e.into()),
        };

        let spec = *decoded.spec();
        if sample_buf.is_none() {
            sample_buf = Some(SampleBuffer::<f32>::new(
                decoded.capacity() as u64,
                spec,
            ));
        }
        let buf = sample_buf.as_mut().unwrap();
        buf.copy_interleaved_ref(decoded);

        // Interleaved samples → per-frame mono average → accumulate RMS
        for frame in buf.samples().chunks(n_channels) {
            let mono: f64 = frame.iter().map(|&s| s as f64).sum::<f64>()
                / n_channels as f64;
            win_sum_sq += mono * mono;
            win_frames += 1;
            total_decoded += 1;

            if win_frames >= frames_per_window {
                let rms = (win_sum_sq / win_frames as f64).sqrt();
                rms_windows.push(rms);
                win_sum_sq = 0.0;
                win_frames = 0;
            }
        }

        if total_frames > 0 {
            progress((total_decoded as f64 / total_frames as f64).min(1.0));
        }
    }

    // Flush any partial window at end of file
    if win_frames > 0 {
        let rms = (win_sum_sq / win_frames as f64).sqrt();
        rms_windows.push(rms);
    }

    progress(1.0);
    Ok((rms_windows, sample_rate, total_decoded))
}

/// Decode an audio file and return per-window `(rms, spectral_flatness)`, sample rate,
/// and total frame count.
///
/// Each window is Hann-windowed before FFT.  Spectral flatness is computed over the
/// positive-frequency half-spectrum (DC..Nyquist) as `geometric_mean / arithmetic_mean`
/// of the magnitude bins.
fn decode_spectral_windows(
    path: &Path,
    window_secs: f64,
    progress: &mut impl FnMut(f64),
) -> Result<(Vec<(f64, f64)>, u32, u64)> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::errors::Error as SErr;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let src = std::fs::File::open(path)
        .map_err(|e| anyhow!("Cannot open audio file {:?}: {}", path, e))?;
    let mss = MediaSourceStream::new(Box::new(src), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| anyhow!("Unsupported audio format: {}", e))?;

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("No decodable audio track found in {:?}", path))?;

    let sample_rate  = track.codec_params.sample_rate.unwrap_or(44100);
    let n_channels   = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);
    let total_frames = track.codec_params.n_frames.unwrap_or(0);
    let track_id     = track.id;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| anyhow!("Cannot create decoder: {}", e))?;

    let frames_per_window = ((sample_rate as f64) * window_secs) as usize;
    let fft_size = frames_per_window.next_power_of_two();

    // Pre-compute Hann window coefficients
    let hann: Vec<f32> = (0..frames_per_window)
        .map(|i| {
            let x = 2.0 * std::f64::consts::PI * i as f64 / (frames_per_window - 1) as f64;
            (0.5 - 0.5 * x.cos()) as f32
        })
        .collect();

    let mut planner: FftPlanner<f32> = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);

    let mut sample_buf: Option<SampleBuffer<f32>> = None;
    let mut win_buf: Vec<f32> = Vec::with_capacity(frames_per_window);
    let mut windows: Vec<(f64, f64)> = Vec::new();
    let mut total_decoded: u64 = 0;

    loop {
        let packet = match format.next_packet() {
            Ok(p)                    => p,
            Err(SErr::IoError(_))    => break,
            Err(SErr::ResetRequired) => { decoder.reset(); continue; }
            Err(e)                   => return Err(e.into()),
        };

        if packet.track_id() != track_id { continue; }

        let decoded = match decoder.decode(&packet) {
            Ok(d)                     => d,
            Err(SErr::DecodeError(_)) => continue,
            Err(SErr::IoError(_))     => break,
            Err(e)                    => return Err(e.into()),
        };

        let spec = *decoded.spec();
        if sample_buf.is_none() {
            sample_buf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
        }
        let buf = sample_buf.as_mut().unwrap();
        buf.copy_interleaved_ref(decoded);

        for frame in buf.samples().chunks(n_channels) {
            let mono: f32 = frame.iter().map(|&s| s).sum::<f32>() / n_channels as f32;
            win_buf.push(mono);
            total_decoded += 1;

            if win_buf.len() >= frames_per_window {
                // Compute RMS
                let rms = {
                    let ss: f64 = win_buf.iter().map(|&s| (s as f64) * (s as f64)).sum();
                    (ss / win_buf.len() as f64).sqrt()
                };

                // Apply Hann window and zero-pad to fft_size
                let mut fft_buf: Vec<Complex<f32>> = (0..fft_size)
                    .map(|i| {
                        let s = if i < frames_per_window {
                            win_buf[i] * hann[i]
                        } else {
                            0.0
                        };
                        Complex { re: s, im: 0.0 }
                    })
                    .collect();

                fft.process(&mut fft_buf);

                // Magnitude of positive-frequency bins (DC .. Nyquist inclusive)
                let n_bins = fft_size / 2 + 1;
                let magnitudes: Vec<f32> = fft_buf[..n_bins]
                    .iter()
                    .map(|c| c.norm())
                    .collect();

                let flatness = compute_spectral_flatness(&magnitudes);

                windows.push((rms, flatness));
                win_buf.clear();
            }
        }

        if total_frames > 0 {
            progress((total_decoded as f64 / total_frames as f64).min(1.0));
        }
    }

    // Flush partial window
    if !win_buf.is_empty() {
        let rms = {
            let ss: f64 = win_buf.iter().map(|&s| (s as f64) * (s as f64)).sum();
            (ss / win_buf.len() as f64).sqrt()
        };
        // Too short for a reliable FFT — carry rms forward, flatness=0 (treat as music)
        windows.push((rms, 0.0));
    }

    progress(1.0);
    Ok((windows, sample_rate, total_decoded))
}

/// Spectral flatness = geometric_mean(|X|) / arithmetic_mean(|X|).
///
/// Returned range: 0.0 (perfectly tonal) to 1.0 (white noise).
/// Returns 1.0 for pure silence (all bins zero) so silent frames are
/// classified as "between tracks" by the spectral detector.
fn compute_spectral_flatness(magnitudes: &[f32]) -> f64 {
    if magnitudes.is_empty() { return 1.0; }

    let arith: f64 = magnitudes.iter().map(|&m| m as f64).sum::<f64>()
        / magnitudes.len() as f64;

    if arith < 1e-10 {
        return 1.0; // silence → treat as between-tracks
    }

    // Geometric mean via log-space to avoid underflow
    let log_mean: f64 = magnitudes
        .iter()
        .map(|&m| {
            let v = m as f64;
            if v > 1e-10 { v.ln() } else { -23.0 } // ln(1e-10) ≈ -23
        })
        .sum::<f64>()
        / magnitudes.len() as f64;

    let geo = log_mean.exp();
    (geo / arith).clamp(0.0, 1.0)
}
