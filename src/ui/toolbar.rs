/*
 *  toolbar.rs
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
use egui::Ui;

#[derive(Debug, Clone, PartialEq)]
pub struct ToolbarState {
    pub pipe_connected: bool,
    pub is_busy: bool,
    pub has_tracks: bool,
    pub has_selection: bool,
    pub has_discogs_release: bool,
    pub has_analysis_wav: bool,
    /// Vinyl sides present in the loaded release (e.g. ['A','B']). Empty = no release loaded.
    pub available_sides: Vec<char>,
    /// Currently selected side filter. None = all sides (default).
    pub selected_side: Option<char>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolbarAction {
    OpenSettings,
    Quit,
    Connect,
    Disconnect,
    SetLabels,
    ExportAll,
    ExportSelected,
    Diagnostics,
    AddTrack,
    ClearTracks,
    FetchDiscogsRelease,
    Rescan,
    /// User changed the active vinyl side filter.
    SideChanged(Option<char>),
}

pub fn show_toolbar(ui: &mut Ui, state: &ToolbarState) -> Vec<ToolbarAction> {
    let mut actions = Vec::new();

    ui.horizontal(|ui| {
        // Settings
        if ui.button("⚙ Settings").clicked() {
            actions.push(ToolbarAction::OpenSettings);
        }

        ui.separator();

        // Connection status dot
        let (dot_color, dot_text) = if state.pipe_connected {
            (egui::Color32::from_rgb(166, 227, 161), "⬤")
        } else {
            (egui::Color32::from_rgb(243, 139, 168), "⬤")
        };
        ui.colored_label(dot_color, dot_text);

        // Connect/Disconnect
        if state.pipe_connected {
            if ui.button("🔌 Disconnect").clicked() {
                actions.push(ToolbarAction::Disconnect);
            }
        } else {
            let btn = ui.add_enabled(!state.is_busy, egui::Button::new("🔌 Connect"));
            if btn.clicked() {
                actions.push(ToolbarAction::Connect);
            }
        }

        ui.separator();

        // Re-scan with pinned boundaries
        {
            let enabled = state.has_analysis_wav && !state.is_busy;
            let btn = ui.add_enabled(enabled, egui::Button::new("🔄 Re-scan"))
                .on_hover_text(
                    "Re-run silence detection on the analysis WAV, \
                     preserving any pinned track boundaries (right-click tracks in waveform)"
                );
            if btn.clicked() {
                actions.push(ToolbarAction::Rescan);
            }
        }

        // Side selector — only shown when the loaded release has more than one vinyl side
        if state.available_sides.len() > 1 {
            ui.separator();

            let multi_disc = state.available_sides.len() > 2;
            let hover = if multi_disc {
                "Filter processing to a single vinyl side.\n\
                 \n\
                 ⚠ IMPORTANT — process one side per Audacity session.\n\
                 The silence detector works against whatever audio is currently\n\
                 loaded. Recording multiple sides as one continuous file makes\n\
                 detection much harder and unreliable for >1 disc recordings.\n\
                 \n\
                 Workflow: record Side A → detect → export → record Side B → …\n\
                 Use this selector to assign the correct side's metadata."
            } else {
                "Filter processing to a single vinyl side.\n\
                 \n\
                 ⚠ Process one side per Audacity session — record Side A, detect\n\
                 and export, then flip and record Side B as a separate session.\n\
                 This selector assigns the correct side's metadata; it does not\n\
                 make multi-side detection more reliable."
            };

            ui.label("Side:").on_hover_text(hover);

            // Compute display label including disc number for multi-disc releases
            let side_label = match state.selected_side {
                None => "All".to_string(),
                Some(s) => {
                    if multi_disc {
                        let disc = disc_for_side(&state.available_sides, s);
                        format!("Side {} (Disc {})", s, disc)
                    } else {
                        format!("Side {}", s)
                    }
                }
            };

            let mut sel = state.selected_side;
            egui::ComboBox::from_id_source("side_selector")
                .selected_text(side_label)
                .show_ui(ui, |ui| {
                    if ui.selectable_value(&mut sel, None, "All sides").changed() {
                        actions.push(ToolbarAction::SideChanged(None));
                    }
                    if multi_disc {
                        // Group sides into discs (2 sides per vinyl disc)
                        for (disc_idx, chunk) in state.available_sides.chunks(2).enumerate() {
                            ui.separator();
                            ui.add_enabled(
                                false,
                                egui::Label::new(
                                    egui::RichText::new(format!("Disc {}", disc_idx + 1))
                                        .small()
                                        .color(egui::Color32::from_rgb(137, 180, 250)),
                                ),
                            );
                            for &s in chunk {
                                if ui.selectable_value(
                                    &mut sel,
                                    Some(s),
                                    format!("  Side {}", s),
                                ).changed() {
                                    actions.push(ToolbarAction::SideChanged(Some(s)));
                                }
                            }
                        }
                    } else {
                        for &s in &state.available_sides {
                            if ui.selectable_value(&mut sel, Some(s), format!("Side {}", s)).changed() {
                                actions.push(ToolbarAction::SideChanged(Some(s)));
                            }
                        }
                    }
                });
        }

        // Fetch Discogs release + guided detection
        {
            let enabled = !state.is_busy;
            let btn = ui.add_enabled(enabled, egui::Button::new("📀 Fetch Release"))
                .on_hover_text(
                    "Search Discogs, pick a release, then walk the audio to \
                     find actual track boundaries"
                );
            if btn.clicked() {
                actions.push(ToolbarAction::FetchDiscogsRelease);
            }
        }

        ui.separator();

        // Set Labels: push track boundaries into Audacity for review/adjustment
        {
            let enabled = state.has_tracks && state.pipe_connected && !state.is_busy;
            let btn = ui.add_enabled(enabled, egui::Button::new("🏷 Set Labels"))
                .on_hover_text("Write track labels into Audacity — review and adjust before exporting");
            if btn.clicked() {
                actions.push(ToolbarAction::SetLabels);
            }
        }

        // Export All: export all tracks with full metadata
        {
            let enabled = state.has_tracks && state.pipe_connected && !state.is_busy;
            let btn = ui.add_enabled(enabled, egui::Button::new("💾 Export All"))
                .on_hover_text("Export all tracks with full metadata including DISCOGS_RELEASEID");
            if btn.clicked() {
                actions.push(ToolbarAction::ExportAll);
            }
        }

        // Export Selected
        {
            let enabled = state.has_selection && state.pipe_connected && !state.is_busy;
            let btn = ui.add_enabled(enabled, egui::Button::new("💾 Selected"));
            if btn.clicked() {
                actions.push(ToolbarAction::ExportSelected);
            }
        }

        ui.separator();

        // Add Track
        {
            let enabled = !state.is_busy;
            let btn = ui.add_enabled(enabled, egui::Button::new("➕ Add Track"));
            if btn.clicked() {
                actions.push(ToolbarAction::AddTrack);
            }
        }

        // Clear Tracks
        {
            let enabled = state.has_tracks && !state.is_busy;
            let btn = ui.add_enabled(enabled, egui::Button::new("🗑 Clear"));
            if btn.clicked() {
                actions.push(ToolbarAction::ClearTracks);
            }
        }

        // Diagnostics
        if ui.button("🩺 Diagnostics").clicked() {
            actions.push(ToolbarAction::Diagnostics);
        }

        ui.separator();

        // Status
        if state.is_busy {
            ui.spinner();
            ui.label("Working...");
        } else if state.pipe_connected {
            ui.colored_label(egui::Color32::from_rgb(166, 227, 161), "Connected");
        } else {
            ui.colored_label(egui::Color32::from_rgb(243, 139, 168), "Disconnected");
        }

        // Quit pinned to the right
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("✖ Quit").clicked() {
                actions.push(ToolbarAction::Quit);
            }
        });
    });

    actions
}

/// Return the 1-based disc number for a side, assuming 2 sides per vinyl disc.
/// `available_sides` must be in the order they appear on the release.
fn disc_for_side(available_sides: &[char], side: char) -> usize {
    let pos = available_sides.iter().position(|&s| s == side).unwrap_or(0);
    pos / 2 + 1
}
