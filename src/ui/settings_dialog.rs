/*
 *  settings.rs
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
use egui::{Context, Ui};

use crate::config::{Config, DetectionMethod, ExportFormat, TrackNumberFormat};

/// Signal returned from the settings dialog to the caller.
#[derive(Default)]
pub struct SettingsResult {
    /// True when Save was clicked — caller should call reload_genre_map if needed.
    pub saved: bool,
}
use crate::workers::export::{validate_path_template, SUPPORTED_TOKENS};

pub fn show_settings_dialog(ctx: &Context, config: &mut Config, open: &mut bool) -> SettingsResult {
    let mut result = SettingsResult::default();
    // Keep a working copy in egui temp storage so edits are only committed on Save.
    // Cancel (or the window X button) discards the working copy without touching config.
    let working_id = egui::Id::new("settings_working_config");

    // Seed the working copy the first time the dialog opens each session.
    let mut working: Config = ctx.data_mut(|d| {
        d.get_temp::<Config>(working_id)
            .unwrap_or_else(|| config.clone())
    });

    let mut should_close  = false;
    let mut should_save   = false;
    let mut was_shown     = false;

    egui::Window::new("Settings")
        .open(open)
        .resizable(true)
        .default_size([520.0, 440.0])
        .show(ctx, |ui| {
            was_shown = true;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new(
                    egui::RichText::new("API Keys").color(egui::Color32::from_rgb(137, 180, 250))
                )
                .default_open(true)
                .show(ui, |ui| { show_api_keys_section(ui, &mut working); });

                ui.add_space(8.0);

                egui::CollapsingHeader::new(
                    egui::RichText::new("Export & Detection").color(egui::Color32::from_rgb(137, 180, 250))
                )
                .default_open(true)
                .show(ui, |ui| { show_export_section(ui, &mut working); });

                ui.add_space(8.0);

                egui::CollapsingHeader::new(
                    egui::RichText::new("Defaults").color(egui::Color32::from_rgb(137, 180, 250))
                )
                .default_open(true)
                .show(ui, |ui| { show_defaults_section(ui, &mut working); });

                ui.add_space(8.0);

                egui::CollapsingHeader::new(
                    egui::RichText::new("Custom Tags").color(egui::Color32::from_rgb(137, 180, 250))
                )
                .default_open(true)
                .show(ui, |ui| { show_custom_tags_section(ui, &mut working); });

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
        *config = working.clone();
        if let Err(e) = config.save() {
            tracing::warn!("Failed to save config: {}", e);
        }
        result.saved = true;
    }

    if should_close || !was_shown {
        ctx.data_mut(|d| d.remove::<Config>(working_id));
        if should_close {
            *open = false;
        }
    } else {
        ctx.data_mut(|d| d.insert_temp(working_id, working));
    }

    result
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

            ui.label("Path Template:")
                .on_hover_text(
                    "Relative path template for exported files (no extension).\n\
                     Tokens: {title} {artist} {album} {album_artist} {genre} {year}\n\
                     {tracknum} {composer} {country} {country_iso} {catalog} {label} {discogs_id}\n\
                     {country_iso} converts 'UK'→'GB', 'Germany'→'DE' etc.\n\
                     Bracket groups like [{country_iso}] are removed when the token is empty.\n\
                     Example: {album_artist}/{album} [{country_iso}][{catalog}]/{tracknum} - {title}"
                );
            ui.vertical(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut config.export_path_template)
                        .desired_width(400.0)
                        .hint_text("{album_artist}/{album}/{tracknum} - {title}"),
                );
                // Live validation feedback
                if config.export_path_template.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_rgb(166, 173, 200),
                        format!(
                            "Tokens: {}",
                            SUPPORTED_TOKENS
                                .iter()
                                .map(|t| format!("{{{}}}", t))
                                .collect::<Vec<_>>()
                                .join("  ")
                        ),
                    );
                } else {
                    let errors = validate_path_template(&config.export_path_template);
                    if errors.is_empty() {
                        ui.colored_label(
                            egui::Color32::from_rgb(166, 227, 161),
                            "✓ All tokens recognised",
                        );
                    } else {
                        for err in &errors {
                            let msg = match &err.suggestion {
                                Some(s) => format!(
                                    "  ✗  Unknown token {{{0}}} — did you mean {{{1}}}?",
                                    err.token, s
                                ),
                                None => format!(
                                    "  ✗  Unknown token {{{0}}}",
                                    err.token
                                ),
                            };
                            ui.colored_label(egui::Color32::from_rgb(243, 139, 168), msg);
                        }
                    }
                }
            });
            ui.end_row();

            ui.label("Album Name Format:")
                .on_hover_text(
                    "Token template written as the Album tag on every exported file.\n\
                     Same tokens as Path Template. Leave blank to use the album name as-is.\n\
                     Example: {album} [{country_iso}][{catalog}]"
                );
            ui.vertical(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut config.album_name_format)
                        .desired_width(400.0)
                        .hint_text("{album} (leave blank to use album name unchanged)"),
                );
                if config.album_name_format.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_rgb(166, 173, 200),
                        "Empty — album tag written verbatim from Discogs / track data",
                    );
                } else {
                    let errors = validate_path_template(&config.album_name_format);
                    if errors.is_empty() {
                        // Show a live preview using a dummy track if we have no real data,
                        // otherwise just confirm the tokens are valid.
                        ui.colored_label(
                            egui::Color32::from_rgb(166, 227, 161),
                            "✓ All tokens recognised",
                        );
                    } else {
                        for err in &errors {
                            let msg = match &err.suggestion {
                                Some(s) => format!(
                                    "  ✗  Unknown token {{{0}}} — did you mean {{{1}}}?",
                                    err.token, s
                                ),
                                None => format!("  ✗  Unknown token {{{0}}}", err.token),
                            };
                            ui.colored_label(egui::Color32::from_rgb(243, 139, 168), msg);
                        }
                    }
                }
            });
            ui.end_row();

            ui.label("Default Comments:")
                .on_hover_text(
                    "Written as the Comment tag on every exported file.\n\
                     Override per-track in the track grid Comments column.\n\
                     Leave blank for no comment tag."
                );
            ui.add(
                egui::TextEdit::singleline(&mut config.default_comments)
                    .desired_width(310.0)
                    .hint_text("e.g. Ripped from vinyl"),
            );
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

            ui.label("Detection Method:")
                .on_hover_text(
                    "RMS: classic energy-based detector — fast, works on clean pressings.\n\
                     Spectral: combines energy + spectral flatness — better for noisy pressings \
                     where inter-track groove noise is loud but spectrally different from music.\n\
                     HMM: Hidden Markov Model over both features — adapts to each recording, \
                     handles momentary level dips without splitting tracks."
                );
            egui::ComboBox::from_id_source("detection_method_combo")
                .selected_text(config.detection_method.display_str())
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config.detection_method,
                        DetectionMethod::Rms,
                        DetectionMethod::Rms.display_str(),
                    );
                    ui.selectable_value(
                        &mut config.detection_method,
                        DetectionMethod::Spectral,
                        DetectionMethod::Spectral.display_str(),
                    );
                    ui.selectable_value(
                        &mut config.detection_method,
                        DetectionMethod::Hmm,
                        DetectionMethod::Hmm.display_str(),
                    );
                    ui.selectable_value(
                        &mut config.detection_method,
                        DetectionMethod::Onnx,
                        DetectionMethod::Onnx.display_str(),
                    );
                });
            ui.end_row();

            if config.detection_method == DetectionMethod::Onnx {
                ui.label("ONNX Model:")
                    .on_hover_text(
                        "Path to a .onnx model file.\n\
                         Supports Mel-CNN (VRipr spec) and Silero-VAD v4 — auto-detected.\n\
                         Leave blank if you have not yet configured a model."
                    );
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut config.onnx_model_path)
                            .desired_width(260.0)
                            .hint_text("path/to/model.onnx"),
                    );
                    if ui.small_button("…").on_hover_text("Browse for .onnx file").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("ONNX model", &["onnx"])
                            .pick_file()
                        {
                            config.onnx_model_path = path.to_string_lossy().into_owned();
                        }
                    }
                    if !config.onnx_model_path.is_empty()
                        && ui.small_button("✕").on_hover_text("Clear model path").clicked()
                    {
                        config.onnx_model_path.clear();
                    }
                });
                ui.end_row();
            }

            if config.detection_method == DetectionMethod::Spectral {
                ui.label("Flatness threshold:")
                    .on_hover_text(
                        "Spectral flatness above which a frame is classified as noise (0 = tonal, 1 = white noise).\n\
                         Inter-track groove noise is typically 0.80–0.95; music 0.10–0.50.\n\
                         Lower = more sensitive (may cut into quiet passages); higher = less sensitive."
                    );
                ui.add(
                    egui::Slider::new(&mut config.spectral_flatness_threshold, 0.5..=0.99)
                        .step_by(0.01)
                );
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

            ui.label("Genre Map File:")
                .on_hover_text("Custom genre.dat file. Leave empty to use the built-in mappings.");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut config.custom_genre_dat)
                        .desired_width(220.0)
                        .hint_text("Built-in (default)"),
                );
                if ui.small_button("…").on_hover_text("Browse for a genre.dat file").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Genre map", &["dat", "txt"])
                        .pick_file()
                    {
                        config.custom_genre_dat = path.to_string_lossy().into_owned();
                    }
                }
                if !config.custom_genre_dat.is_empty()
                    && ui.small_button("✕").on_hover_text("Revert to built-in").clicked()
                {
                    config.custom_genre_dat.clear();
                }
            });
            ui.end_row();

            ui.label("Extra UI Font:")
                .on_hover_text(
                    "Optional extra font loaded as a Unicode fallback for the UI.\n\
                     Useful if a script you need isn't covered by system Noto fonts.\n\
                     Supports .ttf and .otf. Leave blank to rely on auto-discovery.\n\
                     Requires restart to take effect."
                );
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut config.extra_ui_font)
                        .desired_width(220.0)
                        .hint_text("Auto-discover (default)"),
                );
                if ui.small_button("…").on_hover_text("Browse for a font file").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Font", &["ttf", "otf"])
                        .pick_file()
                    {
                        config.extra_ui_font = path.to_string_lossy().into_owned();
                    }
                }
                if !config.extra_ui_font.is_empty()
                    && ui.small_button("✕").on_hover_text("Remove extra font").clicked()
                {
                    config.extra_ui_font.clear();
                }
            });
            ui.end_row();
        });
}

fn show_custom_tags_section(ui: &mut Ui, config: &mut Config) {
    ui.label(
        egui::RichText::new(
            "Up to 3 additional tags written to every exported file. \
             Leave the name blank to skip a slot."
        )
        .weak()
        .small(),
    );
    ui.add_space(4.0);

    egui::Grid::new("custom_tags_grid")
        .num_columns(3)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Tag Name").weak());
            ui.label(egui::RichText::new("Value").weak());
            ui.label("");
            ui.end_row();

            for i in 0..3 {
                let hint_name = match i {
                    0 => "e.g. REPLAYGAIN_TRACK_GAIN",
                    1 => "e.g. REPLAYGAIN_ALBUM_GAIN",
                    _ => "TAG_NAME",
                };
                ui.add(
                    egui::TextEdit::singleline(&mut config.custom_tags[i].0)
                        .desired_width(200.0)
                        .hint_text(hint_name),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut config.custom_tags[i].1)
                        .desired_width(160.0)
                        .hint_text("value"),
                );
                if !config.custom_tags[i].0.is_empty()
                    && ui.small_button("✕").on_hover_text("Clear this tag").clicked()
                {
                    config.custom_tags[i].0.clear();
                    config.custom_tags[i].1.clear();
                }
                ui.end_row();
            }
        });
}
