/*
 *  config.rs
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
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum ExportFormat {
    Flac,
    Mp3,
    Wav,
    Ogg,
}

impl ExportFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExportFormat::Flac => "flac",
            ExportFormat::Mp3  => "mp3",
            ExportFormat::Wav  => "wav",
            ExportFormat::Ogg  => "ogg",
        }
    }

    pub fn extension(&self) -> &'static str {
        self.as_str()
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "mp3" => ExportFormat::Mp3,
            "wav" => ExportFormat::Wav,
            "ogg" => ExportFormat::Ogg,
            _     => ExportFormat::Flac,
        }
    }
}

/// Which detection algorithm to use for finding track boundaries.
#[derive(Debug, Clone, PartialEq)]
pub enum DetectionMethod {
    /// Classic whole-file RMS energy scanner.
    Rms,
    /// Spectral flatness scanner — distinguishes music (tonal) from groove noise (flat).
    /// Better for noisy pressings where silence is loud but spectrally different from music.
    Spectral,
    /// Hidden Markov Model over (RMS, flatness) features.
    /// Self-estimates emission parameters from the audio, then uses Viterbi decoding.
    /// More robust to short dips in level mid-track than threshold-based methods.
    Hmm,
}

impl DetectionMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            DetectionMethod::Rms      => "rms",
            DetectionMethod::Spectral => "spectral",
            DetectionMethod::Hmm      => "hmm",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "spectral" => DetectionMethod::Spectral,
            "hmm"      => DetectionMethod::Hmm,
            _          => DetectionMethod::Rms,
        }
    }
    pub fn display_str(&self) -> &'static str {
        match self {
            DetectionMethod::Rms      => "RMS (default)",
            DetectionMethod::Spectral => "Spectral (noise-aware)",
            DetectionMethod::Hmm      => "HMM (adaptive)",
        }
    }
}

/// How track numbers are formatted when populating the track table from Discogs.
#[derive(Debug, Clone, PartialEq)]
pub enum TrackNumberFormat {
    /// Vinyl-position style: A1, B2, C3 …
    Alpha,
    /// Sequential integers: 1, 2, 3 …
    Numeric,
}

impl TrackNumberFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrackNumberFormat::Alpha   => "alpha",
            TrackNumberFormat::Numeric => "numeric",
        }
    }

    pub fn display_str(&self) -> &'static str {
        match self {
            TrackNumberFormat::Alpha   => "Alpha (A1, B2 …)",
            TrackNumberFormat::Numeric => "Numeric (1, 2, 3 …)",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "numeric" | "num" => TrackNumberFormat::Numeric,
            _                 => TrackNumberFormat::Alpha,
        }
    }
}

// ---------------------------------------------------------------------------
// TOML file representation (serde-friendly, sectioned)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    api: ApiSection,
    #[serde(default)]
    export: ExportSection,
    #[serde(default)]
    silence: SilenceSection,
    #[serde(default)]
    defaults: DefaultsSection,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiSection {
    #[serde(default)]
    discogs_token: String,
}

impl Default for ApiSection {
    fn default() -> Self {
        Self {
            discogs_token: String::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ExportSection {
    #[serde(default = "default_format")]
    format: String,
    #[serde(default = "default_export_dir_str")]
    dir: String,
    #[serde(default = "default_path_template")]
    path_template: String,
    #[serde(default)]
    default_comments: String,
    #[serde(default)]
    album_name_format: String,
}

impl Default for ExportSection {
    fn default() -> Self {
        Self {
            format:            default_format(),
            dir:               default_export_dir_str(),
            path_template:     default_path_template(),
            default_comments:  String::new(),
            album_name_format: String::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SilenceSection {
    #[serde(default = "default_threshold_db")]
    threshold_db: f64,
    #[serde(default = "default_min_duration")]
    min_duration: f64,
    #[serde(default = "default_min_sound_dur")]
    min_sound_dur: f64,
    #[serde(default)]
    adaptive: bool,
    #[serde(default = "default_adaptive_margin_db")]
    adaptive_margin_db: f64,
    #[serde(default = "default_detection_method_str")]
    method: String,
    #[serde(default = "default_flatness_threshold")]
    flatness_threshold: f64,
}

impl Default for SilenceSection {
    fn default() -> Self {
        Self {
            threshold_db:       default_threshold_db(),
            min_duration:       default_min_duration(),
            min_sound_dur:      default_min_sound_dur(),
            adaptive:           false,
            adaptive_margin_db: default_adaptive_margin_db(),
            method:             default_detection_method_str(),
            flatness_threshold: default_flatness_threshold(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DefaultsSection {
    #[serde(default)]
    artist: String,
    #[serde(default)]
    album: String,
    #[serde(default)]
    album_artist: String,
    #[serde(default)]
    genre: String,
    #[serde(default)]
    year: String,
    #[serde(default = "default_track_number_format")]
    track_number_format: String,
    #[serde(default)]
    custom_genre_dat: String,
}

impl Default for DefaultsSection {
    fn default() -> Self {
        Self {
            artist:              String::new(),
            album:               String::new(),
            album_artist:        String::new(),
            genre:               String::new(),
            year:                String::new(),
            track_number_format: default_track_number_format(),
            custom_genre_dat:    String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Default value functions (required by serde default attributes)
// ---------------------------------------------------------------------------

fn default_track_number_format() -> String { "alpha".to_string() }

fn default_format()         -> String { "flac".to_string() }
fn default_detection_method_str() -> String { "rms".to_string() }
fn default_flatness_threshold()   -> f64    { 0.85 }
fn default_path_template() -> String {
    "{album_artist}/{album}/{tracknum} - {title}".to_string()
}
fn default_threshold_db() -> f64    { -40.0 }
fn default_min_duration() -> f64    { 1.5 }
fn default_min_sound_dur() -> f64   { 3.0 }
fn default_adaptive_margin_db() -> f64    { 12.0 }

fn default_export_dir_str() -> String {
    dirs::audio_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Music")
        })
        .join("Vinyl")
        .to_string_lossy()
        .into_owned()
}

// ---------------------------------------------------------------------------
// Public Config struct
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Config {
    pub discogs_token: String,
    pub export_format: ExportFormat,
    pub export_dir: PathBuf,
    pub export_path_template: String,
    pub default_comments: String,
    pub album_name_format: String,
    pub silence_threshold_db: f64,
    pub silence_min_duration: f64,
    pub silence_min_sound_dur: f64,
    pub use_adaptive_threshold: bool,
    pub adaptive_margin_db: f64,
    pub detection_method: DetectionMethod,
    pub spectral_flatness_threshold: f64,
    pub default_artist: String,
    pub default_album: String,
    pub default_album_artist: String,
    pub default_genre: String,
    pub default_year: String,
    pub track_number_format: TrackNumberFormat,
    /// Path to a custom genre.dat file. Empty string = use the built-in mappings.
    pub custom_genre_dat: String,
}

impl Default for Config {
    fn default() -> Self {
        Config::from_file(ConfigFile {
            api: ApiSection::default(),
            export: ExportSection::default(),
            silence: SilenceSection::default(),
            defaults: DefaultsSection::default(),
        })
    }
}

impl Config {
    fn from_file(f: ConfigFile) -> Self {
        Config {
            discogs_token:        f.api.discogs_token,
            export_format:        ExportFormat::from_str(&f.export.format),
            export_dir:           PathBuf::from(&f.export.dir),
            export_path_template: f.export.path_template,
            default_comments:     f.export.default_comments,
            album_name_format:    f.export.album_name_format,
            silence_threshold_db:    f.silence.threshold_db,
            silence_min_duration:    f.silence.min_duration,
            silence_min_sound_dur:   f.silence.min_sound_dur,
            use_adaptive_threshold:  f.silence.adaptive,
            adaptive_margin_db:      f.silence.adaptive_margin_db,
            detection_method:        DetectionMethod::from_str(&f.silence.method),
            spectral_flatness_threshold: f.silence.flatness_threshold,
            default_artist:         f.defaults.artist,
            default_album:          f.defaults.album,
            default_album_artist:   f.defaults.album_artist,
            default_genre:          f.defaults.genre,
            default_year:           f.defaults.year,
            track_number_format:    TrackNumberFormat::from_str(&f.defaults.track_number_format),
            custom_genre_dat:       f.defaults.custom_genre_dat,
        }
    }

    fn to_file(&self) -> ConfigFile {
        ConfigFile {
            api: ApiSection {
                discogs_token: self.discogs_token.clone(),
            },
            export: ExportSection {
                format:           self.export_format.as_str().to_string(),
                dir:              self.export_dir.to_string_lossy().into_owned(),
                path_template:     self.export_path_template.clone(),
                default_comments:  self.default_comments.clone(),
                album_name_format: self.album_name_format.clone(),
            },
            silence: SilenceSection {
                threshold_db:       self.silence_threshold_db,
                min_duration:       self.silence_min_duration,
                min_sound_dur:      self.silence_min_sound_dur,
                adaptive:           self.use_adaptive_threshold,
                adaptive_margin_db: self.adaptive_margin_db,
                method:             self.detection_method.as_str().to_string(),
                flatness_threshold: self.spectral_flatness_threshold,
            },
            defaults: DefaultsSection {
                artist:              self.default_artist.clone(),
                album:               self.default_album.clone(),
                album_artist:        self.default_album_artist.clone(),
                genre:               self.default_genre.clone(),
                year:                self.default_year.clone(),
                track_number_format: self.track_number_format.as_str().to_string(),
                custom_genre_dat:    self.custom_genre_dat.clone(),
            },
        }
    }

    pub fn load() -> Self {
        let path = config_path();
        debug!("Loading config from {:?}", path);

        if !path.exists() {
            debug!("Config file not found, using defaults");
            return Config::default();
        }

        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                warn!("Failed to read config file: {}", e);
                return Config::default();
            }
        };

        match toml::from_str::<ConfigFile>(&text) {
            Ok(f) => Config::from_file(f),
            Err(e) => {
                warn!("Failed to parse config TOML: {}", e);
                Config::default()
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory {:?}", parent))?;
        }

        let text = toml::to_string_pretty(&self.to_file())
            .context("Failed to serialise config to TOML")?;

        std::fs::write(&path, text)
            .with_context(|| format!("Failed to write config to {:?}", path))?;

        debug!("Config saved to {:?}", path);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Path helper (public so app.rs diagnostics can display it)
// ---------------------------------------------------------------------------

pub fn config_path() -> PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        config_dir.join("vripr").join("vripr.toml")
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".vripr")
            .join("vripr.toml")
    }
}
