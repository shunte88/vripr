use egui::{Context, Ui};

use crate::config::{Config, ExportFormat, TrackNumberFormat};

pub fn show_settings_dialog(ctx: &Context, config: &mut Config, open: &mut bool) {
    let mut should_close = false;
    let mut should_save  = false;

    egui::Window::new("Settings")
        .open(open)
        .resizable(true)
        .default_size([520.0, 440.0])
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new(
                    egui::RichText::new("API Keys").color(egui::Color32::from_rgb(137, 180, 250))
                )
                .default_open(true)
                .show(ui, |ui| { show_api_keys_section(ui, config); });

                ui.add_space(8.0);

                egui::CollapsingHeader::new(
                    egui::RichText::new("Export & Detection").color(egui::Color32::from_rgb(137, 180, 250))
                )
                .default_open(true)
                .show(ui, |ui| { show_export_section(ui, config); });

                ui.add_space(8.0);

                egui::CollapsingHeader::new(
                    egui::RichText::new("Defaults").color(egui::Color32::from_rgb(137, 180, 250))
                )
                .default_open(true)
                .show(ui, |ui| { show_defaults_section(ui, config); });

                ui.add_space(16.0);

                ui.horizontal(|ui| {
                    if ui.button(egui::RichText::new("Save").strong()).clicked() {
                        should_save  = true;
                        should_close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        should_close = true;
                    }
                });
            });
        });

    if should_save {
        if let Err(e) = config.save() {
            tracing::warn!("Failed to save config: {}", e);
        }
    }
    if should_close {
        *open = false;
    }
}

fn show_api_keys_section(ui: &mut Ui, config: &mut Config) {
    egui::Grid::new("api_keys_grid")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label("Discogs Token:")
                .on_hover_text("Personal access token from discogs.com → Settings → Developers");
            ui.horizontal(|ui| {
                let id = egui::Id::new("show_discogs_token");
                let mut show: bool = ui.ctx().data_mut(|d| d.get_temp(id).unwrap_or(false));
                ui.add(
                    egui::TextEdit::singleline(&mut config.discogs_token)
                        .password(!show)
                        .desired_width(290.0)
                        .hint_text("Personal token from discogs.com/settings/developers"),
                );
                if ui.small_button(if show { "🙈" } else { "👁" }).clicked() {
                    show = !show;
                    ui.ctx().data_mut(|d| d.insert_temp(id, show));
                }
            });
            ui.end_row();
        });
}

fn show_export_section(ui: &mut Ui, config: &mut Config) {
    egui::Grid::new("export_grid")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            // --- Export ---
            ui.label("Export Format:");
            egui::ComboBox::from_id_source("export_format_combo")
                .selected_text(config.export_format.as_str().to_uppercase())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut config.export_format, ExportFormat::Flac, "FLAC");
                    ui.selectable_value(&mut config.export_format, ExportFormat::Mp3,  "MP3");
                    ui.selectable_value(&mut config.export_format, ExportFormat::Wav,  "WAV");
                    ui.selectable_value(&mut config.export_format, ExportFormat::Ogg,  "OGG");
                });
            ui.end_row();

            ui.label("Export Directory:");
            let mut dir_str = config.export_dir.to_string_lossy().into_owned();
            if ui.add(
                egui::TextEdit::singleline(&mut dir_str).desired_width(310.0)
            ).changed() {
                config.export_dir = std::path::PathBuf::from(&dir_str);
            }
            ui.end_row();

            // --- Detection ---
            ui.separator(); ui.end_row();
            ui.label(egui::RichText::new("Silence Detection").strong()); ui.end_row();

            ui.label("Threshold (dB):")
                .on_hover_text(
                    "Audio below this level is considered silence. \
                     Detection retries automatically with progressively shorter \
                     gaps if the track count doesn't match Discogs.",
                );
            ui.add(egui::Slider::new(&mut config.silence_threshold_db, -80.0..=-10.0).text("dB"));
            ui.end_row();

            ui.label("Min inter-track silence (s):")
                .on_hover_text("First-pass minimum gap duration to register as a track boundary. \
                    Subsequent retry passes shorten this automatically.");
            ui.add(egui::Slider::new(&mut config.silence_min_duration, 0.05..=5.0).text("s"));
            ui.end_row();

            ui.label("Min track duration (s):")
                .on_hover_text("Regions shorter than this are discarded as noise. \
                    Subsequent retry passes lower this automatically.");
            ui.add(egui::Slider::new(&mut config.silence_min_sound_dur, 1.0..=60.0).text("s"));
            ui.end_row();

            ui.label("Adaptive threshold:");
            ui.checkbox(&mut config.use_adaptive_threshold, "Auto-detect noise floor")
                .on_hover_text("Measures the recording's noise floor and sets the threshold \
                    automatically as: noise_floor + margin.");
            ui.end_row();

            if config.use_adaptive_threshold {
                ui.label("Margin above noise floor (dB):");
                ui.add(egui::Slider::new(&mut config.adaptive_margin_db, 3.0..=30.0).text("dB"));
                ui.end_row();
            }
        });
}

fn show_defaults_section(ui: &mut Ui, config: &mut Config) {
    egui::Grid::new("defaults_grid")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label("Default Artist:");
            ui.add(egui::TextEdit::singleline(&mut config.default_artist).desired_width(260.0));
            ui.end_row();

            ui.label("Default Album:");
            ui.add(egui::TextEdit::singleline(&mut config.default_album).desired_width(260.0));
            ui.end_row();

            ui.label("Default Album Artist:");
            ui.add(egui::TextEdit::singleline(&mut config.default_album_artist).desired_width(260.0));
            ui.end_row();

            ui.label("Default Genre:");
            ui.add(egui::TextEdit::singleline(&mut config.default_genre).desired_width(260.0));
            ui.end_row();

            ui.label("Default Year:");
            ui.add(egui::TextEdit::singleline(&mut config.default_year).desired_width(80.0));
            ui.end_row();

            ui.label("Audio File Path:")
                .on_hover_text(
                    "Override path to the source audio file. Leave blank to use the \
                     analysis WAV exported automatically on connect, or the file \
                     currently open in Audacity.",
                );
            ui.add(
                egui::TextEdit::singleline(&mut config.audio_file)
                    .desired_width(310.0)
                    .hint_text("Leave blank — auto-detected on connect"),
            );
            ui.end_row();

            ui.label("Track Number Format:");
            egui::ComboBox::from_id_source("track_number_format_combo")
                .selected_text(config.track_number_format.display_str())
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config.track_number_format,
                        TrackNumberFormat::Alpha,
                        "Alpha (A1, B2 …)",
                    );
                    ui.selectable_value(
                        &mut config.track_number_format,
                        TrackNumberFormat::Numeric,
                        "Numeric (1, 2, 3 …)",
                    );
                });
            ui.end_row();
        });
}
