use egui::Ui;

#[derive(Debug, Clone, PartialEq)]
pub struct ToolbarState {
    pub pipe_connected: bool,
    pub is_busy: bool,
    pub has_tracks: bool,
    pub has_selection: bool,
    pub has_discogs_release: bool,
    pub has_analysis_wav: bool,
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
