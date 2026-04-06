/*
 *  export_dialog.rs
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
use egui::Context;
use egui_extras::{Column, TableBuilder};

use crate::track::TrackMeta;

pub fn show_export_dialog(
    ctx: &Context,
    tracks: &mut Vec<TrackMeta>,
    open: &mut bool,
) -> Option<Vec<TrackMeta>> {
    let mut result: Option<Vec<TrackMeta>> = None;
    let mut should_close = false;

    egui::Window::new("Export All — Review")
        .open(open)
        .resizable(true)
        .default_size([900.0, 500.0])
        .show(ctx, |ui| {
            ui.label("Review and edit track metadata before exporting:");
            ui.add_space(8.0);

            let available_height = ui.available_height() - 50.0;

            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(35.0))   // #
                .column(Column::initial(100.0))  // Time
                .column(Column::initial(160.0))  // Title
                .column(Column::initial(120.0))  // Artist
                .column(Column::initial(120.0))  // Album
                .column(Column::initial(100.0))  // Album Artist
                .column(Column::initial(80.0))   // Genre
                .column(Column::initial(55.0))   // Year
                .min_scrolled_height(0.0)
                .max_scroll_height(available_height)
                .header(20.0, |mut header| {
                    header.col(|ui| { ui.strong("#"); });
                    header.col(|ui| { ui.strong("Time"); });
                    header.col(|ui| { ui.strong("Title"); });
                    header.col(|ui| { ui.strong("Artist"); });
                    header.col(|ui| { ui.strong("Album"); });
                    header.col(|ui| { ui.strong("Album Artist"); });
                    header.col(|ui| { ui.strong("Genre"); });
                    header.col(|ui| { ui.strong("Year"); });
                })
                .body(|mut body| {
                    for track in tracks.iter_mut() {
                        body.row(22.0, |mut row| {
                            row.col(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut track.track_number)
                                        .desired_width(30.0)
                                );
                            });
                            row.col(|ui| {
                                ui.label(track.display_time());
                            });
                            row.col(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut track.title)
                                        .desired_width(150.0)
                                );
                            });
                            row.col(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut track.artist)
                                        .desired_width(110.0)
                                );
                            });
                            row.col(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut track.album)
                                        .desired_width(110.0)
                                );
                            });
                            row.col(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut track.album_artist)
                                        .desired_width(90.0)
                                );
                            });
                            row.col(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut track.genre)
                                        .desired_width(70.0)
                                );
                            });
                            row.col(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut track.year)
                                        .desired_width(45.0)
                                );
                            });
                        });
                    }
                });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(egui::RichText::new("Export").strong()).clicked() {
                    result = Some(tracks.clone());
                    should_close = true;
                }
                if ui.button("Cancel").clicked() {
                    should_close = true;
                }
            });
        });

    if should_close {
        *open = false;
    }

    result
}
