use std::collections::HashMap;
use std::path::Path;
use std::sync::{OnceLock, RwLock};

// ---------------------------------------------------------------------------
// Global state: (current_source_key, map)
// source_key is "" for the built-in dat, or the file path for a custom file.

struct GenreState {
    source_key: String,
    map: HashMap<String, Vec<String>>,
}

static GENRE_STATE: OnceLock<RwLock<GenreState>> = OnceLock::new();

fn state() -> &'static RwLock<GenreState> {
    GENRE_STATE.get_or_init(|| {
        RwLock::new(GenreState {
            source_key: String::new(),
            map:        build_map(include_str!("../../assets/genre.dat")),
        })
    })
}

// ---------------------------------------------------------------------------
// Public API

/// Reload the genre map from a custom file, or revert to the built-in data.
///
/// - `custom_path = None` → use built-in `assets/genre.dat`
/// - `custom_path = Some(path)` → load from the given file
///
/// This is a no-op if the source hasn't changed since the last load.
pub fn reload_genre_map(custom_path: Option<&Path>) {
    let new_key = custom_path
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();

    // Fast-path: check with a read lock first
    {
        if let Ok(r) = state().read() {
            if r.source_key == new_key {
                return;
            }
        }
    }

    // Source changed — rebuild the map
    let new_map = match custom_path {
        None => build_map(include_str!("../../assets/genre.dat")),
        Some(path) => match std::fs::read_to_string(path) {
            Ok(content) => build_map(&content),
            Err(e) => {
                tracing::warn!("Failed to load custom genre dat {:?}: {}", path, e);
                // Fall back to built-in rather than leaving an empty map
                build_map(include_str!("../../assets/genre.dat"))
            }
        },
    };

    if let Ok(mut w) = state().write() {
        w.source_key = new_key;
        w.map = new_map;
        tracing::debug!(
            "Genre map reloaded ({} entries) from {}",
            w.map.len(),
            if w.source_key.is_empty() { "built-in" } else { &w.source_key }
        );
    }
}

/// Sanitize a semicolon-delimited genre string.
///
/// Each component is looked up in the genre map. Known entries expand to their
/// canonical form(s); unknown entries pass through unchanged. The result is
/// deduplicated while preserving order.
pub fn sanitize_genres(input: &str) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    let state = state().read().unwrap_or_else(|e| e.into_inner());
    let map = &state.map;
    let mut out: Vec<String> = Vec::new();

    for raw in input.split(';') {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Exact match first, then case-insensitive fallback
        let mapped = map.get(trimmed).cloned().or_else(|| {
            let lower = trimmed.to_lowercase();
            map.iter()
                .find(|(k, _)| k.to_lowercase() == lower)
                .map(|(_, v)| v.clone())
        });

        match mapped {
            Some(targets) => {
                for t in targets {
                    if !out.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
                        out.push(t);
                    }
                }
            }
            None => {
                if !out.iter().any(|x| x.eq_ignore_ascii_case(trimmed)) {
                    out.push(trimmed.to_string());
                }
            }
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Parsing

fn build_map(data: &str) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(pipe) = line.find('|') else { continue };
        let key = line[..pipe].trim().to_string();
        let val = line[pipe + 1..].trim();
        if key.is_empty() || val.is_empty() {
            continue;
        }
        let targets: Vec<String> = val
            .split(';')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !targets.is_empty() {
            map.insert(key, targets);
        }
    }
    map
}
