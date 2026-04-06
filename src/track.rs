/*
 *  track.rs
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
    #[allow(dead_code)]
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
