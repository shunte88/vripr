use egui::Ui;
use egui_extras::{Column, TableBuilder};
use std::collections::HashSet;

use crate::track::TrackMeta;
use crate::workers::AppSender;

#[derive(Debug, Clone, PartialEq)]
pub enum TableAction {
    None,
    Edit(usize),
    Delete(usize),
    AddTrack,
    Export(Vec<usize>),
}

pub fn show_track_table(
    ui: &mut Ui,
    tracks: &mut Vec<TrackMeta>,
    selected_rows: &mut HashSet<usize>,
    _tx: &AppSender,
    _ctx: &egui::Context,
) -> TableAction {
    let mut action = TableAction::None;

    if tracks.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new(
                "No tracks loaded.\nConnect to Audacity and use Detect Silence,\nor Import Labels, or Add Track manually."
            ).color(egui::Color32::from_rgb(108, 112, 134)));
        });
        return action;
    }

    let available_height = ui.available_height();

    let table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(24.0).at_least(24.0))   // Status icon
        .column(Column::initial(40.0).at_least(30.0))   // #
        .column(Column::initial(110.0).at_least(80.0))  // Time
        .column(Column::initial(200.0).at_least(80.0))  // Title
        .column(Column::initial(150.0).at_least(60.0))  // Artist
        .column(Column::initial(150.0).at_least(60.0))  // Album
        .column(Column::initial(130.0).at_least(60.0))  // Album Artist
        .column(Column::initial(90.0).at_least(50.0))   // Genre
        .column(Column::initial(55.0).at_least(40.0))   // Year
        .column(Column::initial(28.0).at_least(24.0))   // Edit
        .column(Column::initial(28.0).at_least(24.0))   // Delete
        .min_scrolled_height(0.0)
        .max_scroll_height(available_height);

    table
        .header(20.0, |mut header| {
            header.col(|ui| { ui.strong(""); });
            header.col(|ui| { ui.strong("#"); });
            header.col(|ui| { ui.strong("Time"); });
            header.col(|ui| { ui.strong("Title"); });
            header.col(|ui| { ui.strong("Artist"); });
            header.col(|ui| { ui.strong("Album"); });
            header.col(|ui| { ui.strong("Album Artist"); });
            header.col(|ui| { ui.strong("Genre"); });
            header.col(|ui| { ui.strong("Year"); });
            header.col(|ui| { ui.strong(""); });
            header.col(|ui| { ui.strong(""); });
        })
        .body(|mut body| {
            let n = tracks.len();
            for row_idx in 0..n {
                let is_selected = selected_rows.contains(&row_idx);
                let row_color = tracks[row_idx].row_color();

                body.row(22.0, |mut row| {
                    // Apply row background color if set
                    if row_color != egui::Color32::TRANSPARENT || is_selected {
                        let fill = if is_selected {
                            egui::Color32::from_rgba_unmultiplied(49, 50, 68, 200)
                        } else {
                            row_color
                        };
                        row.set_selected(is_selected);
                        // The row background is handled by selection highlight
                        let _ = fill;
                    }

                    // Status icon
                    row.col(|ui| {
                        let icon = tracks[row_idx].status_icon();
                        ui.label(icon);
                    });

                    // Track number (#)
                    row.col(|ui| {
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut tracks[row_idx].track_number)
                                .desired_width(30.0)
                        );
                        // Click to select row
                        if response.clicked() {
                            if !ui.input(|i| i.modifiers.ctrl) {
                                selected_rows.clear();
                            }
                            selected_rows.insert(row_idx);
                        }
                    });

                    // Time (read-only)
                    row.col(|ui| {
                        let time = tracks[row_idx].display_time();
                        let response = ui.label(&time);
                        if response.clicked() {
                            if !ui.input(|i| i.modifiers.ctrl) {
                                selected_rows.clear();
                            }
                            selected_rows.insert(row_idx);
                        }
                    });

                    // Title
                    row.col(|ui| {
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut tracks[row_idx].title)
                                .desired_width(190.0)
                        );
                        if response.clicked() {
                            if !ui.input(|i| i.modifiers.ctrl) {
                                selected_rows.clear();
                            }
                            selected_rows.insert(row_idx);
                        }
                    });

                    // Artist
                    row.col(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut tracks[row_idx].artist)
                                .desired_width(140.0)
                        );
                    });

                    // Album
                    row.col(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut tracks[row_idx].album)
                                .desired_width(140.0)
                        );
                    });

                    // Album Artist
                    row.col(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut tracks[row_idx].album_artist)
                                .desired_width(120.0)
                        );
                    });

                    // Genre
                    row.col(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut tracks[row_idx].genre)
                                .desired_width(80.0)
                        );
                    });

                    // Year
                    row.col(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut tracks[row_idx].year)
                                .desired_width(45.0)
                        );
                    });

                    // Edit button
                    row.col(|ui| {
                        if ui.small_button("✏").on_hover_text("Edit track").clicked() {
                            action = TableAction::Edit(row_idx);
                        }
                    });

                    // Delete button
                    row.col(|ui| {
                        if ui.small_button("🗑").on_hover_text("Delete track").clicked() {
                            action = TableAction::Delete(row_idx);
                        }
                    });
                });
            }
        });

    action
}

/// Return value for the apply-all strip.
#[derive(Debug, Default)]
pub struct ApplyAllResult {
    pub apply_clicked:   bool,
    pub fetch_by_catno:  bool,
}

/// Apply-to-all strip above the track table.
pub fn show_apply_all_strip(
    ui: &mut Ui,
    apply_artist: &mut String,
    apply_album: &mut String,
    apply_album_artist: &mut String,
    apply_genre: &mut String,
    apply_year: &mut String,
    apply_catalog: &mut String,
) -> ApplyAllResult {
    let mut result = ApplyAllResult::default();

    ui.horizontal(|ui| {
        ui.label("Artist:");
        ui.add(egui::TextEdit::singleline(apply_artist).desired_width(110.0));
        ui.label("Album:");
        ui.add(egui::TextEdit::singleline(apply_album).desired_width(110.0));
        ui.label("Album Artist:");
        ui.add(egui::TextEdit::singleline(apply_album_artist).desired_width(110.0));
        ui.label("Genre:");
        ui.add(egui::TextEdit::singleline(apply_genre).desired_width(80.0));
        ui.label("Year:");
        ui.add(egui::TextEdit::singleline(apply_year).desired_width(50.0));
        if ui.button("Apply").clicked() {
            result.apply_clicked = true;
        }

        ui.separator();

        ui.label("Cat#:")
            .on_hover_text("Fetch a Discogs release directly by catalogue number");
        let catno_resp = ui.add(
            egui::TextEdit::singleline(apply_catalog)
                .desired_width(90.0)
                .hint_text("e.g. ECM 1064"),
        );
        let fetch_btn = ui.button("🔍 Fetch");
        if fetch_btn.clicked()
            || (catno_resp.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && !apply_catalog.trim().is_empty())
        {
            result.fetch_by_catno = true;
        }
    });

    result
}
