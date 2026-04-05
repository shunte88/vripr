use egui::{Context, RichText, ScrollArea};

use crate::metadata::DiscogsCandidate;

/// Show a modal window listing Discogs search candidates.
///
/// Returns `Some(index)` when the user clicks a release, `None` otherwise.
/// `open` is set to `false` when the user dismisses the window.
pub fn show_discogs_picker(
    ctx: &Context,
    candidates: &[DiscogsCandidate],
    open: &mut bool,
) -> Option<usize> {
    let mut picked  = None;
    let mut dismiss = false;

    egui::Window::new("Select Discogs Release")
        .resizable(true)
        .default_size([720.0, 440.0])
        .collapsible(false)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(format!("{} result(s) — click to select", candidates.len()))
                    .color(egui::Color32::from_rgb(166, 173, 200)),
            );
            ui.add_space(4.0);

            ScrollArea::vertical().show(ui, |ui| {
                for (i, c) in candidates.iter().enumerate() {
                    ui.horizontal(|ui| {
                        // Main selectable row
                        let resp = ui
                            .selectable_label(
                                false,
                                RichText::new(row_text(i, c)).monospace().size(12.5),
                            )
                            .on_hover_ui(|ui| candidate_tooltip(ui, c));

                        if resp.clicked() {
                            picked  = Some(i);
                            dismiss = true;
                        }

                        // Open release page button
                        let discogs_url = release_url(c);
                        if !discogs_url.is_empty() {
                            if ui.small_button("🌐").on_hover_text("Open in browser").clicked() {
                                open_url(&discogs_url);
                            }
                        }
                    });

                    if i + 1 < candidates.len() {
                        ui.separator();
                    }
                }
            });

            ui.add_space(6.0);
            if ui.button("Cancel").clicked() {
                dismiss = true;
            }
        });

    if dismiss {
        *open = false;
    }

    picked
}

fn release_url(c: &DiscogsCandidate) -> String {
    if !c.uri.is_empty() {
        // uri is typically "/release/12345-Slug" — prepend domain
        if c.uri.starts_with('/') {
            format!("https://www.discogs.com{}", c.uri)
        } else {
            c.uri.clone()
        }
    } else if !c.id.is_empty() {
        format!("https://www.discogs.com/release/{}", c.id)
    } else {
        String::new()
    }
}

fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/c", "start", url]).spawn();
}

fn row_text(idx: usize, c: &DiscogsCandidate) -> String {
    let artist = if c.artist.is_empty() { "?" } else { &c.artist };
    let album  = if c.album.is_empty()  { &c.raw_title } else { &c.album };
    let year   = if c.year.is_empty()   { "----" } else { &c.year };
    let label  = if c.label.is_empty()  { "" } else { &c.label };
    let fmt    = if c.format.is_empty() { "" } else { &c.format };
    let tracks = c.track_count.map(|n| format!(" {n}trk")).unwrap_or_default();

    let mut s = format!("{:>2}. {:30}  {:30}  {}{}", idx + 1, artist, album, year, tracks);
    if !label.is_empty() { s.push_str(&format!("  [{}]", label)); }
    if !fmt.is_empty()   { s.push_str(&format!("  {}", fmt)); }
    s
}

fn candidate_tooltip(ui: &mut egui::Ui, c: &DiscogsCandidate) {
    let url = release_url(c);
    egui::Grid::new("tt_grid")
        .num_columns(2)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            ui.label("ID:");
            ui.horizontal(|ui| {
                ui.label(&c.id);
                if !url.is_empty() {
                    if ui.small_button("Open").clicked() {
                        open_url(&url);
                    }
                }
            });
            ui.end_row();
            ui.label("Artist:");  ui.label(&c.artist);   ui.end_row();
            ui.label("Album:");   ui.label(&c.album);    ui.end_row();
            ui.label("Year:");    ui.label(&c.year);     ui.end_row();
            if let Some(n) = c.track_count {
                ui.label("Tracks:"); ui.label(n.to_string()); ui.end_row();
            }
            ui.label("Label:");   ui.label(&c.label);    ui.end_row();
            ui.label("Format:");  ui.label(&c.format);   ui.end_row();
            ui.label("Country:"); ui.label(&c.country);  ui.end_row();
            ui.label("Cat#:");    ui.label(&c.catno);    ui.end_row();
            if !url.is_empty() {
                ui.label("URL:");
                ui.label(RichText::new(&url).small().color(egui::Color32::from_rgb(137, 180, 250)));
                ui.end_row();
            }
        });
}
