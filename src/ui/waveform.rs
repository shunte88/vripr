use egui::{Color32, FontId, Key, Painter, Pos2, Rect, Stroke, Align2};

// ---------------------------------------------------------------------------
// Public types

/// Active drag state for the waveform panel.
#[derive(Debug, Clone)]
pub enum WaveformDragState {
    /// Dragging a track boundary.
    Boundary { track_vi: usize, is_start: bool },
    /// Drawing a selection band (anchor = fixed end in seconds).
    Selecting { anchor_secs: f64 },
}

/// Events fired back to the caller each frame.
#[derive(Default)]
pub struct WaveformEvent {
    /// A boundary was moved: (vi, is_start, new_time_secs).
    pub drag_update: Option<(usize, bool, f64)>,
    /// Toggle the `pinned` flag on this track vi.
    pub toggle_pin: Option<usize>,
    /// Current active selection in seconds, or None if cleared.
    pub selection: Option<(f64, f64)>,
    /// Play this (start, end) region via Audacity.
    pub play_region: Option<(f64, f64)>,
    /// Stop playback.
    pub stop_playback: bool,
    /// Set track vi's start time to this value.
    pub pin_start: Option<(usize, f64)>,
    /// Set track vi's end time to this value.
    pub pin_end: Option<(usize, f64)>,
}

// ---------------------------------------------------------------------------
// Track colours — Catppuccin Mocha palette.

const TRACK_COLORS: &[(u8, u8, u8)] = &[
    (137, 180, 250), // blue
    (166, 227, 161), // green
    (250, 179, 135), // peach
    (203, 166, 247), // mauve
    (148, 226, 213), // teal
    (245, 194, 231), // pink
    (249, 226, 175), // yellow
    (243, 139, 168), // red
];

// ---------------------------------------------------------------------------
// Main entry point

/// Render the waveform panel and handle all interaction.
///
/// `track_bounds` is `(display_index, start_secs, end_secs, pinned)`.
/// `drag`       — persistent drag state across frames.
/// `selection`  — persistent selection band (seconds) across frames.
/// `is_playing` — show playing indicator in header.
pub fn show_waveform(
    ctx: &egui::Context,
    samples: &[f32],
    duration_secs: f64,
    track_bounds: &[(usize, f64, f64, bool)],
    drag: &mut Option<WaveformDragState>,
    selection: &mut Option<(f64, f64)>,
    is_playing: bool,
) -> WaveformEvent {
    if duration_secs <= 0.0 { return WaveformEvent::default(); }

    let mut event = WaveformEvent::default();

    egui::TopBottomPanel::top("waveform_panel")
        .resizable(true)
        .default_height(120.0)
        .min_height(80.0)
        .max_height(260.0)
        .frame(egui::Frame::none().fill(Color32::from_rgb(17, 17, 27)))
        .show(ctx, |ui| {
            // Header row
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Waveform")
                        .color(Color32::from_rgb(137, 180, 250))
                        .strong()
                        .size(11.0),
                );

                if is_playing {
                    ui.label(
                        egui::RichText::new("  ▶ Playing…  right-click → Stop")
                            .color(Color32::from_rgb(166, 227, 161))
                            .size(11.0),
                    );
                } else if let Some((s, e)) = *selection {
                    let dur = e - s;
                    ui.label(
                        egui::RichText::new(format!(
                            "  Selection: {} – {}  ({:.2}s)  → Add Track to use it  |  Esc to clear",
                            fmt_time(s), fmt_time(e), dur
                        ))
                        .color(Color32::from_rgb(249, 226, 175))
                        .size(11.0),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(
                            "  drag boundary to adjust  |  drag empty area to select  |  right-click for playback & pin options"
                        )
                        .color(Color32::from_rgb(108, 112, 134))
                        .size(10.0),
                    );
                }

                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    *selection = None;
                }
            });

            let (resp, painter) = ui.allocate_painter(
                ui.available_size(),
                egui::Sense::click_and_drag(),
            );
            let rect = resp.rect;
            if rect.width() < 4.0 { return; }

            let time_to_x = |t: f64| -> f32 {
                rect.left() + (t / duration_secs).clamp(0.0, 1.0) as f32 * rect.width()
            };
            let x_to_time = |x: f32| -> f64 {
                ((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64 * duration_secs
            };

            draw_waveform(&painter, rect, samples);
            draw_track_regions(&painter, rect, track_bounds, time_to_x);

            // Draw selection band
            if let Some((sel_s, sel_e)) = *selection {
                let x0 = time_to_x(sel_s.min(sel_e));
                let x1 = time_to_x(sel_s.max(sel_e));
                painter.rect_filled(
                    Rect::from_min_max(Pos2::new(x0, rect.top()), Pos2::new(x1, rect.bottom())),
                    0.0,
                    Color32::from_rgba_premultiplied(249, 226, 175, 35),
                );
                painter.line_segment(
                    [Pos2::new(x0, rect.top()), Pos2::new(x0, rect.bottom())],
                    Stroke::new(1.5, Color32::from_rgb(249, 226, 175)),
                );
                painter.line_segment(
                    [Pos2::new(x1, rect.top()), Pos2::new(x1, rect.bottom())],
                    Stroke::new(1.5, Color32::from_rgb(249, 226, 175)),
                );
            }

            // Playing indicator bar at top of waveform area
            if is_playing {
                painter.rect_filled(
                    Rect::from_min_max(
                        Pos2::new(rect.left(), rect.top()),
                        Pos2::new(rect.right(), rect.top() + 3.0),
                    ),
                    0.0,
                    Color32::from_rgb(166, 227, 161),
                );
            }

            const SNAP_PX: f32 = 8.0;

            // --- Hover: time label + boundary highlight ---
            if let Some(hp) = resp.hover_pos() {
                let t = x_to_time(hp.x);
                let label_x = (hp.x + 6.0).min(rect.right() - 70.0);
                painter.text(
                    Pos2::new(label_x, rect.bottom() - 16.0),
                    Align2::LEFT_CENTER,
                    fmt_time(t),
                    FontId::monospace(20.0),
                    Color32::from_rgb(205, 214, 244),
                );

                for (vi, &(_, ts, te, _)) in track_bounds.iter().enumerate() {
                    for (is_start, t_val) in [(true, ts), (false, te)] {
                        let bx = time_to_x(t_val);
                        if (hp.x - bx).abs() < SNAP_PX {
                            painter.line_segment(
                                [Pos2::new(bx, rect.top()), Pos2::new(bx, rect.bottom())],
                                Stroke::new(3.0, Color32::WHITE),
                            );
                            painter.text(
                                Pos2::new(bx + 4.0, rect.top() + 14.0),
                                Align2::LEFT_CENTER,
                                format!("T{} {} {}", vi + 1,
                                    if is_start { "▶" } else { "◀" },
                                    fmt_time(t_val)),
                                FontId::proportional(11.0),
                                Color32::WHITE,
                            );
                        }
                    }
                }
            }

            // --- Context menu ---
            resp.context_menu(|ui| {
                let click_pos = ui.input(|i| i.pointer.interact_pos());
                let click_t   = click_pos.map(|p| x_to_time(p.x));

                // Stop playback (shown first when playing)
                if is_playing {
                    if ui.button("⏹ Stop playback").clicked() {
                        event.stop_playback = true;
                        ui.close_menu();
                    }
                    ui.separator();
                }

                // Find which track was right-clicked
                let hit = click_t.and_then(|t| {
                    track_bounds.iter().enumerate().find(|(_, entry)| {
                        let (_, ts, te, _) = **entry;
                        t >= ts && t <= te
                    })
                });

                if let Some((vi, &(tidx, ts, te, pinned))) = hit {
                    // Play
                    if !is_playing {
                        if ui.button(format!("▶ Play Track {}", tidx)).clicked() {
                            event.play_region = Some((ts, te));
                            ui.close_menu();
                        }
                    }

                    // Pin start / end at click position
                    if let Some(t) = click_t {
                        if t > ts && t < te {
                            if ui.button(format!("↦ Pin start of T{} here  ({})", tidx, fmt_time(t))).clicked() {
                                event.pin_start = Some((vi, t));
                                ui.close_menu();
                            }
                            if ui.button(format!("↤ Pin end of T{} here  ({})", tidx, fmt_time(t))).clicked() {
                                event.pin_end = Some((vi, t));
                                ui.close_menu();
                            }
                            ui.separator();
                        }
                    }

                    // Pin/unpin boundary toggle
                    let pin_label = if pinned {
                        format!("📌 Unpin Track {}", tidx)
                    } else {
                        format!("📌 Pin Track {}", tidx)
                    };
                    if ui.button(pin_label).clicked() {
                        event.toggle_pin = Some(vi);
                        ui.close_menu();
                    }
                } else {
                    ui.label(
                        egui::RichText::new("Right-click a track region for options")
                            .color(Color32::from_rgb(108, 112, 134))
                            .size(10.0),
                    );
                }
            });

            // --- Drag start: boundary grab or selection start ---
            if resp.drag_started() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    let mut best_px = SNAP_PX;
                    let mut grabbed: Option<WaveformDragState> = None;
                    for (vi, &(_, ts, te, _)) in track_bounds.iter().enumerate() {
                        for (is_start, t_val) in [(true, ts), (false, te)] {
                            let bx = time_to_x(t_val);
                            let d  = (pos.x - bx).abs();
                            if d < best_px {
                                best_px = d;
                                grabbed = Some(WaveformDragState::Boundary { track_vi: vi, is_start });
                            }
                        }
                    }
                    if grabbed.is_none() {
                        grabbed = Some(WaveformDragState::Selecting {
                            anchor_secs: x_to_time(pos.x),
                        });
                    }
                    *drag = grabbed;
                }
            }

            // --- Ongoing drag ---
            if resp.dragged() {
                if let Some(ref d) = *drag {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        match d {
                            WaveformDragState::Boundary { track_vi, is_start } => {
                                event.drag_update = Some((*track_vi, *is_start, x_to_time(pos.x)));
                            }
                            WaveformDragState::Selecting { anchor_secs } => {
                                let cur    = x_to_time(pos.x);
                                let anchor = *anchor_secs;
                                let s = anchor.min(cur);
                                let e = anchor.max(cur);
                                if e - s > 0.1 {
                                    *selection = Some((s, e));
                                    event.selection = *selection;
                                }
                            }
                        }
                    }
                }
            }

            // --- Drag released ---
            if resp.drag_stopped() {
                *drag = None;
            }
        });

    event
}

// ---------------------------------------------------------------------------
// Private helpers

fn draw_waveform(painter: &Painter, rect: Rect, samples: &[f32]) {
    let n = samples.len();
    if n == 0 { return; }
    let bar_w = (rect.width() / n as f32).max(0.5);
    let cy    = rect.center().y;
    let max_h = rect.height() * 0.45;

    for (i, &amp) in samples.iter().enumerate() {
        let x = rect.left() + i as f32 * bar_w;
        let h = (amp * max_h).max(1.0);

        // Body bar: amplitude-gradient colour (teal → green → yellow → red).
        let body_color = amplitude_color(amp, 110);
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(x, cy - h),
                Pos2::new((x + bar_w - 0.5).max(x + 0.5), cy + h),
            ),
            0.0,
            body_color,
        );

        // Bright peak cap at the very tip of each bar.
        let cap_h = (h * 0.12).max(1.5).min(4.0);
        let peak_color = amplitude_color(amp, 210);
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(x, cy - h),
                Pos2::new((x + bar_w - 0.5).max(x + 0.5), cy - h + cap_h),
            ),
            0.0,
            peak_color,
        );
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(x, cy + h - cap_h),
                Pos2::new((x + bar_w - 0.5).max(x + 0.5), cy + h),
            ),
            0.0,
            peak_color,
        );
    }
}

/// Map amplitude [0..1] to a Catppuccin-palette gradient colour.
/// `alpha` is the premultiplied alpha to use.
fn amplitude_color(amp: f32, alpha: u8) -> Color32 {
    let t = amp.clamp(0.0, 1.0);
    let (r, g, b) = if t < 0.40 {
        // teal → green
        let s = t / 0.40;
        (lerp(148, 166, s), lerp(226, 227, s), lerp(213, 161, s))
    } else if t < 0.72 {
        // green → yellow (Catppuccin yellow)
        let s = (t - 0.40) / 0.32;
        (lerp(166, 249, s), lerp(227, 226, s), lerp(161, 175, s))
    } else {
        // yellow → red (Catppuccin red)
        let s = (t - 0.72) / 0.28;
        (lerp(249, 243, s), lerp(226, 139, s), lerp(175, 168, s))
    };
    // Scale RGB by alpha fraction to produce premultiplied values.
    let a = alpha as f32 / 255.0;
    Color32::from_rgba_premultiplied(
        (r as f32 * a) as u8,
        (g as f32 * a) as u8,
        (b as f32 * a) as u8,
        alpha,
    )
}

#[inline]
fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

fn draw_track_regions(
    painter: &Painter,
    rect: Rect,
    track_bounds: &[(usize, f64, f64, bool)],
    time_to_x: impl Fn(f64) -> f32,
) {
    for (vi, &(tidx, ts, te, pinned)) in track_bounds.iter().enumerate() {
        let x0 = time_to_x(ts);
        let x1 = time_to_x(te);
        if x1 <= x0 + 1.0 { continue; }

        let (r, g, b) = TRACK_COLORS[vi % TRACK_COLORS.len()];

        let fill = if pinned {
            Color32::from_rgba_premultiplied(60, 50, 10, 80)
        } else {
            Color32::from_rgba_premultiplied(r / 5, g / 5, b / 5, 70)
        };
        let border = if pinned {
            Color32::from_rgb(249, 226, 175)
        } else {
            Color32::from_rgb(r, g, b)
        };

        painter.rect_filled(
            Rect::from_min_max(Pos2::new(x0, rect.top()), Pos2::new(x1, rect.bottom())),
            0.0,
            fill,
        );
        painter.line_segment(
            [Pos2::new(x0, rect.top()), Pos2::new(x0, rect.bottom())],
            Stroke::new(2.0, border),
        );
        painter.line_segment(
            [Pos2::new(x1, rect.top()), Pos2::new(x1, rect.bottom())],
            Stroke::new(1.5, Color32::from_rgba_premultiplied(
                border.r(), border.g(), border.b(), 140,
            )),
        );
        let label = if pinned { format!("📌{}", tidx) } else { tidx.to_string() };
        if x1 - x0 > 18.0 {
            painter.text(
                Pos2::new(x0 + 3.0, rect.top() + 3.0),
                Align2::LEFT_TOP,
                label,
                FontId::proportional(10.0),
                border,
            );
        }
    }
}

fn fmt_time(secs: f64) -> String {
    let m = (secs / 60.0) as u32;
    let s = secs % 60.0;
    format!("{:02}:{:05.2}", m, s)
}
