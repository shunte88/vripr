use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct TrackMeta {
    pub index: usize,
    pub start: f64,
    pub end: f64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub genre: String,
    pub track_number: String,
    pub year: String,
    pub acoustid: String,
    pub mb_recording_id: String,
    pub discogs_release_id: String,
    pub fingerprint_done: bool,
    pub export_path: Option<PathBuf>,
    /// User has manually anchored this track — re-scan will preserve its boundaries.
    pub pinned: bool,
}

impl TrackMeta {
    pub fn duration(&self) -> f64 {
        self.end - self.start
    }

    pub fn display_time(&self) -> String {
        let fmt = |secs: f64| -> String {
            let s = secs as u64;
            format!("{}:{:02}", s / 60, s % 60)
        };
        format!("{}–{}", fmt(self.start), fmt(self.end))
    }

    pub fn status_icon(&self) -> &'static str {
        if self.export_path.is_some() {
            "✓"
        } else if self.fingerprint_done {
            "🔍"
        } else {
            ""
        }
    }

    pub fn row_color(&self) -> egui::Color32 {
        if self.export_path.is_some() {
            egui::Color32::from_rgba_unmultiplied(30, 90, 40, 80)
        } else if self.fingerprint_done {
            egui::Color32::from_rgba_unmultiplied(30, 60, 120, 80)
        } else {
            egui::Color32::TRANSPARENT
        }
    }
}
