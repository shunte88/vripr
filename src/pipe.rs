/*
 *  pipe.rs
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
use anyhow::{anyhow, Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use tracing::{debug, warn};


pub struct AudacityPipe {
    pub to_path: PathBuf,
    pub from_path: PathBuf,
    to_file: Option<File>,
    from_reader: Option<BufReader<File>>,
}

impl AudacityPipe {
    pub fn new() -> Self {
        let (to_path, from_path) = Self::pipe_paths();
        AudacityPipe {
            to_path,
            from_path,
            to_file: None,
            from_reader: None,
        }
    }

    pub fn pipe_paths() -> (PathBuf, PathBuf) {
        #[cfg(windows)]
        {
            (
                PathBuf::from(r"\\.\pipe\ToSrvPipe"),
                PathBuf::from(r"\\.\pipe\FromSrvPipe"),
            )
        }
        #[cfg(unix)]
        {
            let uid = nix::unistd::getuid().as_raw();
            (
                PathBuf::from(format!("/tmp/audacity_script_pipe.to.{}", uid)),
                PathBuf::from(format!("/tmp/audacity_script_pipe.from.{}", uid)),
            )
        }
        #[cfg(not(any(windows, unix)))]
        {
            (
                PathBuf::from("/tmp/audacity_script_pipe.to.0"),
                PathBuf::from("/tmp/audacity_script_pipe.from.0"),
            )
        }
    }

    pub fn check_pipes() -> bool {
        let (to_path, from_path) = Self::pipe_paths();
        to_path.exists() && from_path.exists()
    }

    pub fn is_connected(&self) -> bool {
        self.to_file.is_some() && self.from_reader.is_some()
    }

    /// Connect to Audacity pipes. Opens write end first, then read end.
    pub fn connect(&mut self) -> Result<()> {
        if !Self::check_pipes() {
            return Err(anyhow!(
                "Audacity pipe files not found at {:?} and {:?}.\n\
                Make sure Audacity is running with mod-script-pipe enabled:\n\
                Edit → Preferences → Modules → mod-script-pipe → Enabled, then restart Audacity.",
                self.to_path,
                self.from_path
            ));
        }

        debug!("Opening write pipe: {:?}", self.to_path);
        let to_file = std::fs::OpenOptions::new()
            .write(true)
            .open(&self.to_path)
            .with_context(|| format!("Failed to open write pipe {:?}", self.to_path))?;

        debug!("Opening read pipe: {:?}", self.from_path);
        let from_file = File::open(&self.from_path)
            .with_context(|| format!("Failed to open read pipe {:?}", self.from_path))?;
        let from_reader = BufReader::new(from_file);

        self.to_file = Some(to_file);
        self.from_reader = Some(from_reader);

        // Drain any stale data
        self.drain_stale();

        debug!("Audacity pipe connected");
        Ok(())
    }

    /// Drain stale data from the read pipe buffer.
    fn drain_stale(&mut self) {
        // On a blocking FIFO we can't easily do a non-blocking read without platform-specific code.
        // We'll just do a quick read with a short timeout via a thread approach if needed.
        // For now, we skip draining — the pipe protocol is synchronous so stale data
        // would only affect reconnects, which is an edge case.
        debug!("Drain stale: skipped (blocking FIFO)");
    }

    pub fn disconnect(&mut self) {
        if let Some(f) = self.to_file.take() {
            drop(f);
        }
        if let Some(r) = self.from_reader.take() {
            drop(r);
        }
        debug!("Audacity pipe disconnected");
    }

    /// Send a command to Audacity and read the response.
    /// Returns (response_body, success).
    pub fn send(&mut self, cmd: &str) -> Result<(String, bool)> {
        let to = self.to_file.as_mut().ok_or_else(|| anyhow!("Pipe not connected"))?;
        let reader = self.from_reader.as_mut().ok_or_else(|| anyhow!("Pipe not connected"))?;

        // Write command followed by newline
        write!(to, "{}\n", cmd)
            .with_context(|| format!("Failed to write command to pipe: {:?}", cmd))?;
        to.flush().context("Failed to flush pipe")?;

        // Read response until sentinel
        let mut lines = Vec::new();
        let success = loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line)
                .context("Failed to read from pipe")?;

            if n == 0 {
                return Err(anyhow!("Audacity closed the pipe (EOF). Has it crashed?"));
            }

            let trimmed = line.trim_end_matches(|c| c == '\n' || c == '\r');

            if trimmed == "BatchCommand finished: OK" {
                break true;
            } else if trimmed == "BatchCommand finished: Failed" {
                break false;
            } else if !trimmed.is_empty() {
                lines.push(trimmed.to_string());
            }
        };

        Ok((lines.join("\n"), success))
    }

    /// Ping Audacity with an empty command to check connectivity.
    #[allow(dead_code)]
    pub fn ping(&mut self) -> bool {
        match self.send("GetInfo: Type=Tracks Format=JSON") {
            Ok((_, success)) => success,
            Err(e) => {
                warn!("Ping failed: {}", e);
                false
            }
        }
    }

    /// Remove all label tracks from the Audacity project.
    ///
    /// Each `LabelSounds` call *adds* a new label track rather than replacing
    /// existing ones, so stale tracks must be cleared before re-running
    /// detection or we will read back old results.
    pub fn clear_label_tracks(&mut self) -> Result<()> {
        loop {
            let raw = match self.send("GetInfo: Type=Tracks Format=JSON") {
                Ok((r, _)) => r,
                Err(_) => break,
            };
            let tracks: serde_json::Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(_) => break,
            };
            let label_idx = tracks
                .as_array()
                .and_then(|arr| {
                    arr.iter()
                        .enumerate()
                        .find(|(_, t)| {
                            t.get("kind").and_then(|v| v.as_str()) == Some("label")
                        })
                        .map(|(i, _)| i)
                });
            match label_idx {
                Some(idx) => {
                    let _ = self.send(&format!(
                        "SelectTracks: Track={} TrackCount=1 Mode=Set",
                        idx
                    ));
                    let _ = self.send("RemoveTracks:");
                }
                None => break,
            }
        }
        debug!("Label tracks cleared");
        Ok(())
    }

    /// Get all labels from Audacity.
    /// Returns Vec<(start, end, label_text)> sorted by start time.
    pub fn get_labels(&mut self) -> Result<Vec<(f64, f64, String)>> {
        let (raw, _) = self.send("GetInfo: Type=Labels Format=JSON")
            .context("Failed to get labels from Audacity")?;

        debug!("Labels raw ({} bytes): {}", raw.len(), &raw[..raw.len().min(400)]);

        if raw.trim().is_empty() || raw.trim() == "null" || raw.trim() == "[]" {
            return Ok(Vec::new());
        }

        let parsed: serde_json::Value = serde_json::from_str(&raw)
            .with_context(|| format!("Failed to parse labels JSON: {}", &raw[..raw.len().min(200)]))?;

        let mut labels = Vec::new();
        // Walk the JSON tree recursively. Any node that is [number, number, ...]
        // is treated as a label triple [start, end, text]. Everything else is
        // recursed into. This handles all known Audacity JSON layouts without
        // assuming a fixed nesting depth.
        Self::collect_labels(&parsed, &mut labels);

        labels.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        debug!("Parsed {} label(s)", labels.len());
        Ok(labels)
    }

    fn collect_labels(val: &serde_json::Value, out: &mut Vec<(f64, f64, String)>) {
        let Some(arr) = val.as_array() else { return };

        // A label triple: first two elements are both numbers (start, end).
        if arr.len() >= 2 && arr[0].is_number() && arr[1].is_number() {
            let start = arr[0].as_f64().unwrap_or(0.0);
            let end   = arr[1].as_f64().unwrap_or(start);
            let text  = arr.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string();
            out.push((start, end, text));
            return;
        }

        // Not a label — recurse into any array children.
        for child in arr {
            if child.is_array() {
                Self::collect_labels(child, out);
            }
        }
    }


    /// Add one label per track (title + time bounds) to Audacity.
    ///
    /// Does NOT clear existing labels first — call `clear_label_tracks` before
    /// this if you want a clean slate.  Audacity creates the label track
    /// automatically on the first `AddLabel:` call if none exists.
    pub fn add_labels_from_tracks(&mut self, tracks: &[crate::track::TrackMeta]) -> Result<()> {
        for (i, track) in tracks.iter().enumerate() {
            let sel = format!(
                "SelectTime: Start={:.3} End={:.3} RelativeTo=ProjectStart",
                track.start, track.end
            );
            self.send(&sel).context("SelectTime for label failed")?;
            self.send("AddLabel:").context("AddLabel failed")?;
            let safe_title = track.title
                .replace('\\', "\\\\")
                .replace('"', "\\\"");
            let set_text = format!("SetLabel: Label={} Text=\"{}\"", i, safe_title);
            self.send(&set_text).context("SetLabel failed")?;
        }
        debug!("Added {} label(s) to Audacity", tracks.len());
        Ok(())
    }

    /// Export the full Audacity project (current state, including edits) to a WAV file.
    /// Used to produce a clean analysis copy that reflects any user edits (e.g. needle-drop removal).
    pub fn export_full_wav(&mut self, path: &std::path::Path) -> Result<()> {
        self.send("SelectAll:").context("SelectAll failed")?;
        let path_str = path.to_string_lossy();
        let cmd = format!("Export2: Filename=\"{}\" NumChannels=2", path_str);
        debug!("export_full_wav: {}", cmd);
        let (_, success) = self.send(&cmd).context("Export2 failed")?;
        if !success {
            warn!("Export2 for analysis WAV returned failure status");
        }
        Ok(())
    }

    /// Select a time region in Audacity.
    pub fn select_time(&mut self, start: f64, end: f64) -> Result<()> {
        let cmd = format!(
            "SelectTime: Start={:.3} End={:.3} RelativeTo=ProjectStart",
            start, end
        );
        self.send(&cmd).context("SelectTime failed")?;
        // Also select the first track
        let cmd2 = "SelectTracks: Track=0 TrackCount=1 Mode=Set";
        self.send(cmd2).context("SelectTracks failed")?;
        Ok(())
    }

    /// Select a time region and start playback in Audacity.
    ///
    /// Audacity plays the selected region and returns immediately — playback
    /// continues asynchronously. Call `stop_playback` to stop it early.
    pub fn play_region(&mut self, start: f64, end: f64) -> Result<()> {
        let cmd = format!(
            "SelectTime: Start={:.3} End={:.3} RelativeTo=ProjectStart",
            start, end
        );
        self.send(&cmd).context("SelectTime for playback failed")?;
        self.send("SelectTracks: Track=0 TrackCount=1 Mode=Set")
            .context("SelectTracks for playback failed")?;
        self.send("Play:").context("Play command failed")?;
        Ok(())
    }

    /// Stop playback in Audacity.
    pub fn stop_playback(&mut self) -> Result<()> {
        self.send("Stop:").context("Stop command failed")?;
        Ok(())
    }

    /// Export the current selection to a file.
    pub fn export_selection(&mut self, path: &std::path::Path, channels: u8) -> Result<()> {
        let path_str = path.to_string_lossy();
        let cmd = format!(
            "Export2: Filename=\"{}\" NumChannels={}",
            path_str, channels
        );
        debug!("Export2 command: {}", cmd);
        let (_, success) = self.send(&cmd)
            .context("Export2 command failed")?;
        if !success {
            warn!("Export2 returned failure for path {:?}", path);
        }
        Ok(())
    }

    /// Get tracks info from Audacity.
    #[allow(dead_code)]
    pub fn get_tracks_info(&mut self) -> Result<serde_json::Value> {
        let (raw, _) = self.send("GetInfo: Type=Tracks Format=JSON")
            .context("GetInfo Tracks failed")?;
        serde_json::from_str(&raw)
            .with_context(|| format!("Failed to parse tracks JSON: {}", &raw[..raw.len().min(200)]))
    }

    /// Get version/info from Audacity via Menus query.
    #[allow(dead_code)]
    pub fn get_version(&mut self) -> Result<String> {
        let (raw, _) = self.send("GetInfo: Type=Menus Format=JSON")
            .context("GetInfo Menus failed")?;
        Ok(raw)
    }

    /// Try to resolve the path of the audio file currently open in Audacity.
    ///
    /// Audacity's `GetInfo: Type=Tracks` includes a `"filename"` field on wave
    /// tracks in recent versions. Returns `None` if Audacity is not connected
    /// or the field is absent.
    #[allow(dead_code)]
    pub fn get_audio_file_path(&mut self) -> Result<Option<std::path::PathBuf>> {
        let (raw, _) = self.send("GetInfo: Type=Tracks Format=JSON")
            .context("GetInfo Tracks failed")?;

        let tracks: serde_json::Value = serde_json::from_str(&raw)
            .unwrap_or(serde_json::Value::Array(vec![]));

        if let Some(arr) = tracks.as_array() {
            for track in arr {
                if track.get("kind").and_then(|v| v.as_str()) == Some("wave") {
                    if let Some(filename) = track.get("filename").and_then(|v| v.as_str()) {
                        if !filename.is_empty() {
                            return Ok(Some(std::path::PathBuf::from(filename)));
                        }
                    }
                }
            }
        }
        Ok(None)
    }
}

impl Default for AudacityPipe {
    fn default() -> Self {
        Self::new()
    }
}
