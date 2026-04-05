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
    pub composer: String,
    pub comments: String,
    pub discogs_release_id: String,
    pub country: String,
    pub catalog: String,
    pub label: String,
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
        let dur = self.end - self.start;
        let d = dur as u64;
        format!("{}–{} ({}:{:02})", fmt(self.start), fmt(self.end), d / 60, d % 60)
    }

    pub fn status_icon(&self) -> &'static str {
        if self.export_path.is_some() { "✓" } else { "" }
    }

    pub fn row_color(&self) -> egui::Color32 {
        if self.export_path.is_some() {
            egui::Color32::from_rgba_unmultiplied(30, 90, 40, 80)
        } else {
            egui::Color32::TRANSPARENT
        }
    }
}
