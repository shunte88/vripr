pub mod export;

use std::sync::mpsc;

#[derive(Debug)]
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
