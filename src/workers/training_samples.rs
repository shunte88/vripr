/*
 *  training_samples.rs
 *
 *  Generate WAV snippets around track boundaries for ONNX model training.
 *
 *  Output directory: /data2/vripr_training (created if absent)
 *
 *  Per detected boundary:
 *    s{n:02}_{hash8}.wav / .json  — track start  (silence → sound)
 *    e{n:02}_{hash8}.wav / .json  — track end    (sound → silence)
 *    m{n:02}_{hash8}.wav / .json  — mid-track negative (only if track ≥ 60 s)
 *
 *  All snippets: mono, 16 kHz, 16-bit PCM, peak-normalised.
 *  Window: ±HALF_WIN seconds around the boundary — clamped to file bounds, no zero-padding.
 *  JSON sidecar records the exact boundary position within the snippet.
 *
 *  MIT License — see root Cargo.toml
 */

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use anyhow::{Context, Result, anyhow};
use hound::{SampleFormat, WavSpec, WavWriter};
use serde_json::json;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::track::TrackMeta;
use crate::workers::WorkerMessage;

const TARGET_SR:          u32 = 16_000;
const HALF_WIN:           f64 = 8.0;   // seconds each side — enough context, won't bleed into adjacent tracks
const MIN_TRACK_FOR_MID:  f64 = 60.0;  // minimum track length to emit a mid-track negative sample

/// Generate training snippets for all boundaries in `tracks`.
/// Returns the number of .wav files written.
pub fn generate_training_samples(
    wav_path:   &Path,
    tracks:     &[TrackMeta],
    artist:     &str,
    album:      &str,
    output_dir: &Path,
    tx:         &mpsc::Sender<WorkerMessage>,
) -> Result<usize> {

    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("Cannot create output dir {:?}", output_dir))?;

    let hash = album_hash(artist, album);

    let _ = tx.send(WorkerMessage::Log(format!(
        "Decoding {} for sample extraction…", wav_path.display()
    )));

    let (samples_orig, orig_sr) = decode_mono(wav_path)?;

    if samples_orig.is_empty() {
        return Err(anyhow!("Decoded zero samples from {:?} — unsupported audio format?", wav_path));
    }

    let orig_dur = samples_orig.len() as f64 / orig_sr as f64;
    let _ = tx.send(WorkerMessage::Log(format!(
        "  {:.1}s at {} Hz — resampling to {} Hz…", orig_dur, orig_sr, TARGET_SR
    )));

    let samples   = resample_linear(&samples_orig, orig_sr, TARGET_SR);
    let total_dur = samples.len() as f64 / TARGET_SR as f64;

    let _ = tx.send(WorkerMessage::Log(format!(
        "  {:.1}s resampled, {} 16kHz samples — extracting snippets…", total_dur, samples.len()
    )));

    let mut written = 0usize;

    for (idx, track) in tracks.iter().enumerate() {
        let n = idx + 1;

        // s: track start (silence→sound transition)
        written += write_snippet(
            &samples, total_dur, track.start,
            "start", &format!("s{:02}_{}", n, hash),
            n, artist, album, output_dir, tx,
        )?;

        // e: track end (sound→silence transition)
        written += write_snippet(
            &samples, total_dur, track.end,
            "end", &format!("e{:02}_{}", n, hash),
            n, artist, album, output_dir, tx,
        )?;

        // m: mid-track negative — only when track is long enough that the full
        // window is clear of both boundaries
        let dur = track.end - track.start;
        if dur >= MIN_TRACK_FOR_MID {
            let mid = track.start + dur / 2.0;
            written += write_snippet(
                &samples, total_dur, mid,
                "mid", &format!("m{:02}_{}", n, hash),
                n, artist, album, output_dir, tx,
            )?;
        }
    }

    let _ = tx.send(WorkerMessage::Log(format!(
        "Training samples: {} files written to {}", written, output_dir.display()
    )));

    Ok(written)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Extract, normalise, and write one snippet plus its JSON sidecar.
/// The window is clamped to the actual audio bounds — no zero-padding.
/// Returns 1 on success, 0 if center is outside the audio.
fn write_snippet(
    samples:   &[f32],
    total_dur: f64,
    center_s:  f64,
    kind:      &str,
    stem:      &str,
    track_idx: usize,
    artist:    &str,
    album:     &str,
    out_dir:   &Path,
    tx:        &mpsc::Sender<WorkerMessage>,
) -> Result<usize> {
    // Skip if the boundary itself is outside the audio
    if center_s < 0.0 || center_s > total_dur {
        return Ok(0);
    }

    let sr         = TARGET_SR as f64;
    let center_idx = (center_s * sr) as usize;
    let half_idx   = (HALF_WIN * sr) as usize;

    // Clamp window to file bounds — real audio, no zero-padding
    let start_idx  = center_idx.saturating_sub(half_idx);
    let end_idx    = (center_idx + half_idx).min(samples.len());

    if start_idx >= end_idx {
        return Ok(0);
    }

    let window = samples[start_idx..end_idx].to_vec();
    // Where the boundary falls within this snippet
    let boundary_at = (center_idx - start_idx) as f64 / sr;

    let normalised = peak_normalise(window);

    // WAV
    let wav_path = out_dir.join(format!("{}.wav", stem));
    write_wav(&wav_path, &normalised)?;

    // JSON sidecar
    let json_path = out_dir.join(format!("{}.json", stem));
    let meta = json!({
        "kind":             kind,
        "boundary_at_secs": boundary_at,
        "window_secs":      HALF_WIN,
        "track_index":      track_idx,
        "artist":           artist,
        "album":            album,
        "sample_rate":      TARGET_SR,
    });
    std::fs::write(&json_path, serde_json::to_string_pretty(&meta)?)
        .with_context(|| format!("Cannot write {:?}", json_path))?;

    let _ = tx.send(WorkerMessage::Log(format!(
        "  {} ({}, boundary at {:.1}s in {:.1}s window)",
        stem, kind, boundary_at, (end_idx - start_idx) as f64 / sr
    )));

    Ok(1)
}

/// Peak-normalise to ±1.0. Returns the input unchanged if it's silent.
fn peak_normalise(mut samples: Vec<f32>) -> Vec<f32> {
    let peak = samples.iter().copied().fold(0.0f32, |a, s| a.max(s.abs()));
    if peak > 1e-6 {
        let scale = 1.0 / peak;
        for s in &mut samples { *s *= scale; }
    }
    samples
}

/// Write mono f32 samples as 16-bit PCM WAV at TARGET_SR.
fn write_wav(path: &Path, samples: &[f32]) -> Result<()> {
    let spec = WavSpec {
        channels:        1,
        sample_rate:     TARGET_SR,
        bits_per_sample: 16,
        sample_format:   SampleFormat::Int,
    };
    let mut w = WavWriter::create(path, spec)
        .with_context(|| format!("Cannot create WAV {:?}", path))?;
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        w.write_sample(v)?;
    }
    w.finalize().context("WAV finalise failed")?;
    Ok(())
}

/// Decode any audio file to mono f32 at its native sample rate.
/// Uses SampleBuffer<f32> which auto-converts all PCM formats (S16, S24, S32, F32, etc.)
fn decode_mono(path: &Path) -> Result<(Vec<f32>, u32)> {
    use symphonia::core::errors::Error as SErr;

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
        .ok_or_else(|| anyhow!("No default track in {:?}", path))?;
    let sr = track.codec_params.sample_rate
        .ok_or_else(|| anyhow!("Unknown sample rate in {:?}", path))?;
    let n_channels = track.codec_params.channels
        .map(|c| c.count())
        .unwrap_or(1);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("Decoder creation failed")?;

    let track_id = track.id;
    let mut samples: Vec<f32> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) if p.track_id() == track_id => p,
            Ok(_)                              => continue,
            Err(SErr::IoError(_))              => break,
            Err(SErr::ResetRequired)           => break,
            Err(e)                             => return Err(e.into()),
        };

        let decoded = match decoder.decode(&packet) {
            Ok(d)                    => d,
            Err(SErr::DecodeError(_)) => continue,
            Err(e)                   => return Err(e.into()),
        };

        let spec = *decoded.spec();
        if sample_buf.is_none() {
            sample_buf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
        }
        let buf = sample_buf.as_mut().unwrap();
        buf.copy_interleaved_ref(decoded);

        // Interleaved → mono average
        for frame in buf.samples().chunks(n_channels) {
            let mono: f32 = frame.iter().sum::<f32>() / n_channels as f32;
            samples.push(mono);
        }
    }

    Ok((samples, sr))
}

/// Linear resampler — same as onnx_detect.rs so training/inference are processed identically.
fn resample_linear(samples: &[f32], from_sr: u32, to_sr: u32) -> Vec<f32> {
    if from_sr == to_sr { return samples.to_vec(); }
    let ratio   = from_sr as f64 / to_sr as f64;
    let out_len = (samples.len() as f64 / ratio) as usize;
    (0..out_len).map(|i| {
        let src  = i as f64 * ratio;
        let lo   = src as usize;
        let hi   = (lo + 1).min(samples.len().saturating_sub(1));
        let frac = (src - lo as f64) as f32;
        samples[lo] * (1.0 - frac) + samples[hi] * frac
    }).collect()
}

/// 8-character hex hash of artist+album — stable, filesystem-safe identifier.
fn album_hash(artist: &str, album: &str) -> String {
    let mut h = DefaultHasher::new();
    format!("{}{}", artist, album).hash(&mut h);
    format!("{:016x}", h.finish())[..8].to_string()
}

/// Return the default training output directory.
pub fn default_output_dir() -> PathBuf {
    PathBuf::from("/data2/vripr_training")
}
