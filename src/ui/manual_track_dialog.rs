use egui::Context;

#[derive(Debug, Clone, Default)]
pub struct ManualTrackInput {
    pub start: String,
    pub end: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub error: String,
}

impl ManualTrackInput {
    pub fn clear(&mut self) {
        self.start.clear();
        self.end.clear();
        self.title.clear();
        self.artist.clear();
        self.album.clear();
        self.error.clear();
    }

    pub fn validate(&self) -> Result<(f64, f64), String> {
        let start: f64 = self.start.parse().map_err(|_| "Invalid start time".to_string())?;
        let end: f64 = self.end.parse().map_err(|_| "Invalid end time".to_string())?;
        if start >= end {
            return Err("Start time must be less than end time".to_string());
        }
        Ok((start, end))
    }
}

/// Returns true when the Add button is clicked with valid input.
pub fn show_manual_track_dialog(
    ctx: &Context,
    input: &mut ManualTrackInput,
    open: &mut bool,
) -> bool {
    let mut added = false;
    let mut should_close = false;

    egui::Window::new("Add Track Manually")
        .open(open)
        .resizable(false)
        .default_size([360.0, 280.0])
        .show(ctx, |ui| {
            egui::Grid::new("manual_track_grid")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Start time (s):");
                    ui.add(
                        egui::TextEdit::singleline(&mut input.start)
                            .desired_width(150.0)
                            .hint_text("e.g. 0.0 or 65.5"),
                    );
                    ui.end_row();

                    ui.label("End time (s):");
                    ui.add(
                        egui::TextEdit::singleline(&mut input.end)
                            .desired_width(150.0)
                            .hint_text("e.g. 180.0"),
                    );
                    ui.end_row();

                    ui.label("Title:");
                    ui.add(
                        egui::TextEdit::singleline(&mut input.title)
                            .desired_width(200.0),
                    );
                    ui.end_row();

                    ui.label("Artist:");
                    ui.add(
                        egui::TextEdit::singleline(&mut input.artist)
                            .desired_width(200.0),
                    );
                    ui.end_row();

                    ui.label("Album:");
                    ui.add(
                        egui::TextEdit::singleline(&mut input.album)
                            .desired_width(200.0),
                    );
                    ui.end_row();
                });

            if !input.error.is_empty() {
                ui.add_space(4.0);
                ui.colored_label(
                    egui::Color32::from_rgb(243, 139, 168),
                    &input.error.clone(),
                );
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(egui::RichText::new("Add").strong()).clicked() {
                    match input.validate() {
                        Ok(_) => {
                            input.error.clear();
                            added = true;
                            should_close = true;
                        }
                        Err(e) => {
                            input.error = e;
                        }
                    }
                }
                if ui.button("Cancel").clicked() {
                    should_close = true;
                }
            });
        });

    if should_close {
        *open = false;
    }

    added
}
