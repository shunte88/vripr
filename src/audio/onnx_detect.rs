/*
 *  onnx_detect.rs
 *
 *  vripr - The vinyl viper for perfect rippage - Audacity vinyl ripping helper
 *  (c) 2025-26 Stuart Hunter
 *
 *  ONNX-based track boundary detector.
 *
 *  Two model interfaces are supported and auto-detected from the model's input names:
 *
 *  ── VRipr Mel-CNN (recommended for vinyl) ────────────────────────────────────────
 *    Input  "input"  : float32[1, N_MELS, n_frames]  log mel-spectrogram
 *    Output "output" : float32[1, n_frames]           P(music) ∈ 0..1 per frame
 *
 *    Frame parameters (must match training):
 *      N_MELS  = 64
 *      WIN_MS  = 25 ms  (Hann window)
 *      HOP_MS  = 10 ms
 *      F_MIN   = 50 Hz
 *      F_MAX   = sample_rate / 2
 *
 *  ── Silero-VAD v4 ────────────────────────────────────────────────────────────────
 *    Detected when model has an input named "sr".
 *    Requires audio at 16 kHz (resampled internally).
 *    Inputs : "input" [1, 512], "sr" [1] int64, "h"/"c" [2,1,64] float32
 *    Outputs: "output" [1,1], "hn"/"cn" for next state
 *
 *    Download: https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad.onnx
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
 */

use anyhow::{anyhow, Context, Result};
use ort::{inputs, session::Session, value::Tensor};
use rustfft::{num_complex::Complex, FftPlanner};
use std::path::Path;
use tracing::{debug, info, warn};

use super::{DetectedTrack, DetectorDiagnostics};

// ---------------------------------------------------------------------------
// Feature extraction constants — must match training configuration
// ---------------------------------------------------------------------------

pub const N_MELS:   usize = 64;
pub const WIN_MS:   f64   = 25.0;
pub const HOP_MS:   f64   = 10.0;
pub const F_MIN:    f64   = 50.0;
const SILERO_SR:    u32   = 16_000;
const SILERO_CHUNK: usize = 512;   // 32 ms at 16 kHz

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct OnnxDetectorConfig {
    /// P(music) threshold — frames above this are classified as music.
    pub music_threshold: f64,
    /// Minimum run of music frames before a track is accepted, in seconds.
    pub min_sound_secs: f64,
    /// Minimum gap (silence) between tracks in seconds.
    pub min_silence_secs: f64,
    /// Seconds prepended to each detected region.
    pub pre_padding: f64,
    /// Seconds appended to each detected region.
    pub post_padding: f64,
}

impl Default for OnnxDetectorConfig {
    fn default() -> Self {
        Self {
            music_threshold:  0.5,
            min_sound_secs:   2.0,
            min_silence_secs: 0.5,
            pre_padding:      0.1,
            post_padding:     0.1,
        }
    }
}

// ---------------------------------------------------------------------------
// Model kind detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum ModelKind {
    /// VRipr mel-spectrogram CNN / frame-level classifier.
    MelCnn,
    /// Silero-VAD v4 (raw PCM, stateful LSTM, 16 kHz).
    SileroVad,
}

fn detect_model_kind(session: &Session) -> ModelKind {
    let has_sr = session.inputs().iter().any(|i| i.name() == "sr");
    if has_sr {
        info!("ONNX: detected Silero-VAD interface (found 'sr' input)");
        ModelKind::SileroVad
    } else {
        info!("ONNX: detected Mel-CNN interface");
        ModelKind::MelCnn
    }
}

// ---------------------------------------------------------------------------
// Mel filterbank
// ---------------------------------------------------------------------------

fn hz_to_mel(hz: f64) -> f64 { 2595.0 * (1.0 + hz / 700.0).log10() }
fn mel_to_hz(mel: f64) -> f64 { 700.0 * (10_f64.powf(mel / 2595.0) - 1.0) }

/// Build a mel filterbank matrix of shape [n_mels][n_bins].
/// Each row is one triangular filter (linear amplitude scale).
fn build_mel_filterbank(n_mels: usize, n_fft: usize, sr: u32) -> Vec<Vec<f32>> {
    let n_bins = n_fft / 2 + 1;
    let f_max  = sr as f64 / 2.0;
    let mel_min = hz_to_mel(F_MIN);
    let mel_max = hz_to_mel(f_max);

    // n_mels+2 equally spaced mel-scale pivot points
    let mel_pts: Vec<f64> = (0..=(n_mels + 1))
        .map(|i| mel_min + (mel_max - mel_min) * i as f64 / (n_mels + 1) as f64)
        .collect();

    // Pivot points in FFT bin space (floating point)
    let bin_pts: Vec<f64> = mel_pts.iter()
        .map(|&m| mel_to_hz(m) / f_max * (n_bins - 1) as f64)
        .collect();

    let mut filters = vec![vec![0.0f32; n_bins]; n_mels];
    for m in 0..n_mels {
        let (left, centre, right) = (bin_pts[m], bin_pts[m + 1], bin_pts[m + 2]);
        for k in 0..n_bins {
            let kf = k as f64;
            let val = if kf >= left && kf <= centre && centre > left {
                ((kf - left) / (centre - left)) as f32
            } else if kf > centre && kf <= right && right > centre {
                ((right - kf) / (right - centre)) as f32
            } else {
                0.0
            };
            filters[m][k] = val;
        }
    }
    filters
}

// ---------------------------------------------------------------------------
// Audio decoding
// ---------------------------------------------------------------------------

/// Decode an audio file to mono f32 samples.
/// Returns (samples, sample_rate).
fn decode_mono_samples(path: &Path) -> Result<(Vec<f32>, u32)> {
    use symphonia::core::audio::{AudioBufferRef, SampleBuffer, Signal};
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let src = std::fs::File::open(path)
        .with_context(|| format!("Cannot open {:?}", path))?;
    let mss = MediaSourceStream::new(Box::new(src), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .context("Format probe failed")?;

    let mut format = probed.format;
    let track = format.default_track()
        .ok_or_else(|| anyhow!("No default track"))?;
    let sr = track.codec_params.sample_rate
        .ok_or_else(|| anyhow!("Unknown sample rate"))?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("Decoder creation failed")?;

    let mut samples: Vec<f32> = Vec::new();
    let track_id = track.id;

    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) if p.track_id() == track_id => p,
            Ok(_) => continue,
            Err(symphonia::core::errors::Error::IoError(_)) => break,
            Err(symphonia::core::errors::Error::ResetRequired) => break,
            Err(e) => return Err(e.into()),
        };
        let decoded = decoder.decode(&packet)?;
        let spec = *decoded.spec();
        let cap  = decoded.capacity();
        let sb = sample_buf.get_or_insert_with(|| SampleBuffer::new(cap as u64, spec));
        sb.copy_interleaved_ref(decoded);

        let ch = spec.channels.count();
        for frame in sb.samples().chunks_exact(ch) {
            let mono = frame.iter().sum::<f32>() / ch as f32;
            samples.push(mono);
        }
    }

    Ok((samples, sr))
}

// ---------------------------------------------------------------------------
// Log mel-spectrogram extraction
// ---------------------------------------------------------------------------

/// Compute log mel-spectrogram.
/// Returns flat data in row-major [n_frames * N_MELS] order, and n_frames.
fn log_mel_spectrogram(samples: &[f32], sr: u32) -> (Vec<f32>, usize) {
    let win_samples = ((WIN_MS / 1000.0) * sr as f64) as usize;
    let hop_samples = ((HOP_MS / 1000.0) * sr as f64) as usize;
    let n_fft       = win_samples.next_power_of_two();
    let n_bins      = n_fft / 2 + 1;

    let filters = build_mel_filterbank(N_MELS, n_fft, sr);

    // Hann window
    let hann: Vec<f32> = (0..win_samples)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64
                                  / (win_samples - 1) as f64).cos()) as f32)
        .collect();

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n_fft);

    let n_frames = if samples.len() >= win_samples {
        (samples.len() - win_samples) / hop_samples + 1
    } else {
        0
    };

    let mut mel_data = vec![0.0f32; n_frames * N_MELS];

    for frame in 0..n_frames {
        let offset = frame * hop_samples;
        let mut buf: Vec<Complex<f32>> = (0..n_fft)
            .map(|i| Complex {
                re: if i < win_samples { samples[offset + i] * hann[i] } else { 0.0 },
                im: 0.0,
            })
            .collect();
        fft.process(&mut buf);

        let power: Vec<f32> = buf[..n_bins]
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .collect();

        for (m, filter) in filters.iter().enumerate() {
            let energy: f32 = filter.iter().zip(power.iter()).map(|(f, p)| f * p).sum();
            mel_data[frame * N_MELS + m] = energy.max(1e-10_f32).ln();
        }
    }

    (mel_data, n_frames)
}

// ---------------------------------------------------------------------------
// Linear resampler (for silero-VAD 16 kHz requirement)
// ---------------------------------------------------------------------------

fn resample_linear(samples: &[f32], from_sr: u32, to_sr: u32) -> Vec<f32> {
    if from_sr == to_sr { return samples.to_vec(); }
    let ratio = from_sr as f64 / to_sr as f64;
    let out_len = (samples.len() as f64 / ratio) as usize;
    (0..out_len).map(|i| {
        let src  = i as f64 * ratio;
        let lo   = src as usize;
        let hi   = (lo + 1).min(samples.len() - 1);
        let frac = (src - lo as f64) as f32;
        samples[lo] * (1.0 - frac) + samples[hi] * frac
    }).collect()
}

// ---------------------------------------------------------------------------
// Inference — Mel-CNN
// ---------------------------------------------------------------------------

/// Run Mel-CNN inference.
///
/// Expected model:
///   input  "input"  float32[1, N_MELS, n_frames]
///   output "output" float32[1, n_frames]  (or float32[n_frames])
fn infer_mel_cnn(session: &mut Session, mel_data: &[f32], n_frames: usize) -> Result<Vec<f32>> {
    if n_frames == 0 { return Ok(Vec::new()); }

    // Reshape [n_frames, N_MELS] → [1, N_MELS, n_frames] by transposing
    let mut transposed = vec![0.0f32; N_MELS * n_frames];
    for f in 0..n_frames {
        for m in 0..N_MELS {
            transposed[m * n_frames + f] = mel_data[f * N_MELS + m];
        }
    }

    let shape = vec![1i64, N_MELS as i64, n_frames as i64];
    let tensor = Tensor::<f32>::from_array((shape, transposed))
        .context("Failed to create mel input tensor")?;

    let outputs = session
        .run(inputs!["input" => tensor])
        .context("Mel-CNN inference failed")?;

    let arr = outputs["output"]
        .try_extract_array::<f32>()
        .context("Failed to extract Mel-CNN output")?;

    let probs: Vec<f32> = arr.iter().cloned().collect();
    debug!("Mel-CNN: {} frames → {} probabilities", n_frames, probs.len());
    Ok(probs)
}

// ---------------------------------------------------------------------------
// Inference — Silero-VAD v4
// ---------------------------------------------------------------------------

/// Run Silero-VAD v4 inference on 16 kHz audio.
/// Returns one P(speech) value per 32 ms chunk.
fn infer_silero_vad(session: &mut Session, samples_16k: &[f32]) -> Result<Vec<f32>> {
    // Initial LSTM state — zeros, shape [2, 1, 64]
    let lstm_shape = vec![2i64, 1, 64];
    let zeros = vec![0.0f32; 2 * 1 * 64];
    let mut h_data = zeros.clone();
    let mut c_data = zeros;

    let mut probs: Vec<f32> = Vec::new();
    let n_chunks = samples_16k.len().div_ceil(SILERO_CHUNK);

    for (ci, chunk) in samples_16k.chunks(SILERO_CHUNK).enumerate() {
        let mut padded = chunk.to_vec();
        padded.resize(SILERO_CHUNK, 0.0);

        let input_tensor = Tensor::<f32>::from_array((vec![1i64, SILERO_CHUNK as i64], padded))
            .context("Silero input tensor")?;
        let sr_tensor = Tensor::<i64>::from_array((vec![1i64], vec![SILERO_SR as i64]))
            .context("Silero sr tensor")?;
        let h_tensor = Tensor::<f32>::from_array((lstm_shape.clone(), h_data.clone()))
            .context("Silero h tensor")?;
        let c_tensor = Tensor::<f32>::from_array((lstm_shape.clone(), c_data.clone()))
            .context("Silero c tensor")?;

        let outputs = session
            .run(inputs![
                "input" => input_tensor,
                "sr"    => sr_tensor,
                "h"     => h_tensor,
                "c"     => c_tensor
            ])
            .context("Silero-VAD inference failed")?;

        // Speech probability for this chunk
        let out_arr = outputs["output"]
            .try_extract_array::<f32>()
            .context("Silero output extract")?;
        probs.push(*out_arr.iter().next().unwrap_or(&0.0));

        // Update LSTM state
        h_data = outputs["hn"]
            .try_extract_array::<f32>()
            .context("Silero hn extract")?
            .iter().cloned().collect();
        c_data = outputs["cn"]
            .try_extract_array::<f32>()
            .context("Silero cn extract")?
            .iter().cloned().collect();

        debug!("Silero chunk {}/{}: p={:.3}", ci + 1, n_chunks, probs.last().unwrap_or(&0.0));
    }

    info!("Silero-VAD: {} chunks processed", probs.len());
    Ok(probs)
}

// ---------------------------------------------------------------------------
// Post-processing: probabilities → DetectedTrack regions
// ---------------------------------------------------------------------------

fn probs_to_tracks(probs: &[f32], frame_secs: f64, cfg: &OnnxDetectorConfig) -> Vec<DetectedTrack> {
    if probs.is_empty() { return Vec::new(); }

    let min_sound_frames   = ((cfg.min_sound_secs   / frame_secs).round() as usize).max(1);
    let min_silence_frames = ((cfg.min_silence_secs / frame_secs).round() as usize).max(1);
    let threshold = cfg.music_threshold as f32;

    // Binarise
    let mut is_sound: Vec<bool> = probs.iter().map(|&p| p >= threshold).collect();

    // Bridge short silence gaps
    let mut i = 0;
    while i < is_sound.len() {
        if !is_sound[i] {
            let start = i;
            while i < is_sound.len() && !is_sound[i] { i += 1; }
            if i - start < min_silence_frames {
                for j in start..i { is_sound[j] = true; }
            }
        } else {
            i += 1;
        }
    }

    // Extract regions, discarding short ones
    let mut tracks = Vec::new();
    let mut i = 0;
    while i < is_sound.len() {
        if is_sound[i] {
            let start_frame = i;
            while i < is_sound.len() && is_sound[i] { i += 1; }
            let end_frame = i;
            if end_frame - start_frame >= min_sound_frames {
                let start = (start_frame as f64 * frame_secs - cfg.pre_padding).max(0.0);
                let end   = end_frame as f64 * frame_secs + cfg.post_padding;
                tracks.push(DetectedTrack { start, end });
            }
        } else {
            i += 1;
        }
    }

    tracks
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Detect track boundaries using an ONNX model.
///
/// `model_path` — path to a `.onnx` file (Mel-CNN or Silero-VAD, auto-detected).
///
/// Returns the same `(tracks, diagnostics)` tuple as the other detectors.
pub fn detect_tracks_onnx(
    path:       &Path,
    model_path: &Path,
    cfg:        &OnnxDetectorConfig,
    progress:   &mut impl FnMut(f64),
) -> Result<(Vec<DetectedTrack>, DetectorDiagnostics)> {

    progress(0.05);
    let mut session = Session::builder()
        .context("ORT session builder failed")?
        .commit_from_file(model_path)
        .with_context(|| format!("Failed to load ONNX model from {:?}", model_path))?;

    let kind = detect_model_kind(&session);
    info!("ONNX detector: {:?}", kind);

    progress(0.1);
    let (samples, sr) = decode_mono_samples(path)?;
    let total_secs = samples.len() as f64 / sr as f64;
    info!("ONNX: decoded {:.1}s at {} Hz", total_secs, sr);

    progress(0.3);
    let (probs, frame_secs): (Vec<f32>, f64) = match kind {
        ModelKind::MelCnn => {
            let (mel_data, n_frames) = log_mel_spectrogram(&samples, sr);
            progress(0.6);
            let p = infer_mel_cnn(&mut session, &mel_data, n_frames)?;
            (p, HOP_MS / 1000.0)
        }
        ModelKind::SileroVad => {
            let samples_16k = resample_linear(&samples, sr, SILERO_SR);
            progress(0.5);
            let p = infer_silero_vad(&mut session, &samples_16k)?;
            (p, SILERO_CHUNK as f64 / SILERO_SR as f64)
        }
    };

    progress(0.85);
    let tracks = probs_to_tracks(&probs, frame_secs, cfg);
    info!("ONNX: {} track(s) detected", tracks.len());
    progress(1.0);

    Ok((tracks, DetectorDiagnostics {
        threshold_db:    -(cfg.music_threshold * 100.0),  // synthetic placeholder
        noise_floor_db:  None,
        total_secs,
        n_windows:       probs.len(),
        window_secs:     frame_secs,
    }))
}
