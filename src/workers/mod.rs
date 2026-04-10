/*
 *  mod.rs
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
pub mod export;
pub mod training_samples;

use std::sync::mpsc;

#[derive(Debug)]
#[allow(dead_code)]
pub enum WorkerMessage {
    Log(String),
    PipeConnected { info: String },
    PipeDisconnected,
    PipeError(String),
    TracksDetected(Vec<crate::track::TrackMeta>),
    TrackUpdate { index: usize, updates: TrackUpdate },
    Progress { done: usize, total: usize },
    WorkerError(String),
    WorkerFinished,
    DiscogsReleaseFetched(crate::metadata::DiscogsRelease),
    DiscogsSearchCandidates(Vec<crate::metadata::DiscogsCandidate>),
    CoverArtData(Vec<u8>),
    WaveformReady { path: std::path::PathBuf, samples: Vec<f32>, duration_secs: f64 },
}

#[derive(Debug, Default, Clone)]
pub struct TrackUpdate {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub track_number: Option<String>,
    pub year: Option<String>,
    pub discogs_release_id: Option<String>,
    pub export_path: Option<std::path::PathBuf>,
}

pub type AppSender = mpsc::Sender<WorkerMessage>;
