/*
 *  fonts.rs
 *
 *  vripr - The vinyl viper for perfect rippage - Audacity vinyl ripping helper
 *  (c) 2025-26 Stuart Hunter
 *
 *  Unicode font configuration for egui.
 *
 *  egui ships with a Latin-only subset font. This module discovers and loads
 *  system Noto fonts as fallbacks so that Discogs metadata containing CJK,
 *  Devanagari, Arabic, Hebrew, Thai, or any other Unicode script renders
 *  correctly rather than showing replacement boxes.
 *
 *  All loaded fonts are registered as *fallbacks* after the built-in font, so
 *  the UI appearance for Latin text is unchanged.
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
 */

use egui::{FontData, FontDefinitions, FontFamily};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Font candidates — (internal_name, face_index, candidate_paths)
// ---------------------------------------------------------------------------
// Ordering matters: egui tries each font in the family list in order and uses
// the first one that contains a glyph. We add broad-coverage fonts first so
// they catch the widest range of scripts before the script-specific ones.

struct FontCandidate {
    name:  &'static str,
    index: u32,          // face index inside .ttc collections; 0 for .ttf/.otf
    paths: &'static [&'static str],
}

const FONT_CANDIDATES: &[FontCandidate] = &[
    // ── NotoSans (non-CJK) ──────────────────────────────────────────────────
    // Covers: Latin Extended, Greek, Cyrillic, Devanagari, Bengali, Gujarati,
    // Gurmukhi, Kannada, Malayalam, Oriya, Tamil, Telugu, Sinhala, Tibetan,
    // Myanmar, Georgian, Hangul Jamo, Ethiopic, Cherokee, and more.
    FontCandidate {
        name:  "noto_sans",
        index: 0,
        paths: &[
            // Debian / Ubuntu
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            // Fedora / RHEL
            "/usr/share/fonts/noto-sans/NotoSans-Regular.ttf",
            "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
            // Arch / Manjaro
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            // openSUSE
            "/usr/share/fonts/truetype/NotoSans-Regular.ttf",
            // macOS (Homebrew cask fonts-noto)
            "/Library/Fonts/NotoSans-Regular.ttf",
            // Windows (manually installed)
            "C:\\Windows\\Fonts\\NotoSans-Regular.ttf",
        ],
    },

    // ── NotoSans Devanagari ─────────────────────────────────────────────────
    // Needed for: Sanskrit/Hindi releases (e.g. ॐ U+0950, Devanagari script).
    FontCandidate {
        name:  "noto_sans_devanagari",
        index: 0,
        paths: &[
            "/usr/share/fonts/truetype/noto/NotoSansDevanagari-Regular.ttf",
            "/usr/share/fonts/noto-sans/NotoSansDevanagari-Regular.ttf",
            "/usr/share/fonts/google-noto/NotoSansDevanagari-Regular.ttf",
            "/usr/share/fonts/noto/NotoSansDevanagari-Regular.ttf",
            "/Library/Fonts/NotoSansDevanagari-Regular.ttf",
            "C:\\Windows\\Fonts\\NotoSansDevanagari-Regular.ttf",
        ],
    },

    // ── NotoSans CJK ────────────────────────────────────────────────────────
    // Covers: Chinese (Simplified + Traditional), Japanese, Korean.
    // The .ttc contains all CJK variants; face 0 is the SC (Simplified Chinese)
    // superset that also covers JP and KR glyphs via Unicode Unification.
    FontCandidate {
        name:  "noto_sans_cjk",
        index: 0,
        paths: &[
            // Debian / Ubuntu (fonts-noto-cjk)
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            // Fedora
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            // Arch
            "/usr/share/fonts/noto/NotoSansCJK-Regular.ttc",
            // macOS system CJK fonts (pre-installed)
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            // Windows system CJK
            "C:\\Windows\\Fonts\\msyh.ttc",       // Microsoft YaHei (SC)
            "C:\\Windows\\Fonts\\meiryo.ttc",      // Meiryo (JP)
            "C:\\Windows\\Fonts\\malgun.ttf",      // Malgun Gothic (KR)
        ],
    },

    // ── NotoSans Arabic ─────────────────────────────────────────────────────
    FontCandidate {
        name:  "noto_sans_arabic",
        index: 0,
        paths: &[
            "/usr/share/fonts/truetype/noto/NotoSansArabic-Regular.ttf",
            "/usr/share/fonts/noto/NotoSansArabic-Regular.ttf",
            "/usr/share/fonts/google-noto/NotoSansArabic-Regular.ttf",
            "/Library/Fonts/NotoSansArabic-Regular.ttf",
            "C:\\Windows\\Fonts\\NotoSansArabic-Regular.ttf",
        ],
    },

    // ── NotoSans Hebrew ─────────────────────────────────────────────────────
    FontCandidate {
        name:  "noto_sans_hebrew",
        index: 0,
        paths: &[
            "/usr/share/fonts/truetype/noto/NotoSansHebrew-Regular.ttf",
            "/usr/share/fonts/noto/NotoSansHebrew-Regular.ttf",
            "/Library/Fonts/NotoSansHebrew-Regular.ttf",
            "C:\\Windows\\Fonts\\NotoSansHebrew-Regular.ttf",
        ],
    },

    // ── NotoSans Thai ───────────────────────────────────────────────────────
    FontCandidate {
        name:  "noto_sans_thai",
        index: 0,
        paths: &[
            "/usr/share/fonts/truetype/noto/NotoSansThai-Regular.ttf",
            "/usr/share/fonts/noto/NotoSansThai-Regular.ttf",
            "/Library/Fonts/NotoSansThai-Regular.ttf",
            "C:\\Windows\\Fonts\\NotoSansThai-Regular.ttf",
        ],
    },

    // ── DejaVu Sans ─────────────────────────────────────────────────────────
    // Good broad-coverage fallback present on almost all Linux systems.
    // Covers Latin Extended A/B/C/D, Greek Extended, Cyrillic Extended.
    FontCandidate {
        name:  "dejavu_sans",
        index: 0,
        paths: &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/DejaVuSans.ttf",
        ],
    },

    // ── GNU FreeSans ────────────────────────────────────────────────────────
    // Very broad coverage including many rare/historical scripts.
    FontCandidate {
        name:  "free_sans",
        index: 0,
        paths: &[
            "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
            "/usr/share/fonts/gnu-free/FreeSans.ttf",
            "/usr/share/fonts/freefont/FreeSans.ttf",
        ],
    },
];

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Discover and load system Unicode fonts as egui fallbacks.
///
/// Call once from `App::new()`:
/// ```ignore
/// setup_unicode_fonts(&cc.egui_ctx);
/// ```
///
/// If `extra_font_path` is provided (user-configured in Settings), that font
/// is loaded first and gets the highest glyph-lookup priority among the extras.
pub fn setup_unicode_fonts(ctx: &egui::Context, extra_font_path: Option<&str>) {
    let mut fonts   = FontDefinitions::default();
    let mut n_loaded = 0usize;

    // --- Optional user-specified font (highest priority extra) ---
    if let Some(path) = extra_font_path.filter(|p| !p.is_empty()) {
        match std::fs::read(path) {
            Ok(data) => {
                fonts.font_data.insert(
                    "user_font".to_owned(),
                    FontData::from_owned(data),
                );
                push_fallback(&mut fonts, "user_font");
                info!("Unicode fonts: loaded user font from {}", path);
                n_loaded += 1;
            }
            Err(e) => {
                warn!("Unicode fonts: could not load user font '{}': {}", path, e);
            }
        }
    }

    // --- System candidates ---
    for candidate in FONT_CANDIDATES {
        if fonts.font_data.contains_key(candidate.name) {
            continue; // already loaded (shouldn't happen, but be safe)
        }
        for path in candidate.paths {
            if let Ok(data) = std::fs::read(path) {
                let mut fd = FontData::from_owned(data);
                fd.index = candidate.index;
                fonts.font_data.insert(candidate.name.to_owned(), fd);
                push_fallback(&mut fonts, candidate.name);
                debug!("Unicode fonts: '{}' ← {}", candidate.name, path);
                n_loaded += 1;
                break;
            }
        }
    }

    // --- Also scan user font directories on Linux/macOS ---
    n_loaded += scan_user_fonts(&mut fonts);

    if n_loaded > 0 {
        ctx.set_fonts(fonts);
        info!("Unicode fonts: {} fallback font(s) registered", n_loaded);
    } else {
        warn!(
            "Unicode fonts: no extended Unicode fonts found on this system. \
             Install fonts-noto (Linux) or Noto fonts (macOS/Windows) for full \
             Unicode support. Non-Latin characters will show as □."
        );
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Add `name` to the end of the Proportional fallback list.
fn push_fallback(fonts: &mut FontDefinitions, name: &str) {
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .push(name.to_owned());
}

/// Scan `~/.local/share/fonts` and `~/.fonts` for any `.ttf`/`.otf` not yet
/// covered, loading them as additional fallbacks. Returns number loaded.
fn scan_user_fonts(fonts: &mut FontDefinitions) -> usize {
    let Some(home) = dirs::home_dir() else { return 0 };
    let dirs = [
        home.join(".local/share/fonts"),
        home.join(".fonts"),
    ];

    // Names of scripts/keywords we consider worth adding as fallbacks
    // if not already covered by the candidates above.
    let interesting: &[&str] = &[
        "NotoSans", "NotoSerif", "Noto", "DejaVu", "FreeSans", "FreeSerif",
        "SourceHanSans", "WenQuanYi", "Arial Unicode", "unifont",
    ];

    let already_have: Vec<String> = fonts.font_data.keys().cloned().collect();
    let mut n = 0usize;

    for dir in &dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let ext  = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext.to_lowercase().as_str(), "ttf" | "otf") { continue; }

            let stem = path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            // Skip if not an interesting font
            if !interesting.iter().any(|kw| stem.contains(kw)) { continue; }

            // Derive a stable internal name from the filename
            let name = format!("user_{}", stem.to_lowercase().replace([' ', '-'], "_"));
            if already_have.contains(&name) { continue; }

            if let Ok(data) = std::fs::read(&path) {
                fonts.font_data.insert(name.clone(), FontData::from_owned(data));
                push_fallback(fonts, &name);
                debug!("Unicode fonts: user font '{}' ← {}", name, path.display());
                n += 1;
            }
        }
    }

    n
}
