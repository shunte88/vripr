# Contributing to VRipr

## Building

```bash
cargo build          # debug build
cargo build --release
cargo test
```

Requires Rust 1.75+. On Linux, also needs the `nix` crate's user feature (handled automatically via `Cargo.toml`).

## Project structure

```
src/
  main.rs             entry point, runtime setup
  app.rs              main application state and egui update loop
  config.rs           settings struct, TOML persistence
  pipe.rs             Audacity scripting pipe (synchronous send/recv)
  track.rs            TrackMeta — the central data type
  tagging.rs          lofty tag writing (ID3/FLAC/Vorbis + DISCOGS_RELEASEID)
  audio/
    mod.rs            silence detector, waveform display compute
  metadata/
    discogs.rs        Discogs search, release fetch, cover art download
    mod.rs            shared helpers (duration comparison, track splitting)
  ui/
    app.rs            (via app.rs) panel layout
    toolbar.rs        toolbar buttons and state
    waveform.rs       waveform panel — rendering and drag interaction
    track_table.rs    editable track grid
    discogs_picker.rs release selection modal
    settings_dialog.rs settings window
    manual_track_dialog.rs add-track dialog
  workers/
    mod.rs            WorkerMessage enum, AppSender type
    export.rs         async export worker (per-track Audacity export + tagging)
```

## Code conventions

- No `unwrap()` in library code — use `anyhow::Result` and `?`.
- Async tasks run on a `tokio` runtime; all Audacity pipe calls go through `spawn_blocking` since the pipe is synchronous blocking I/O.
- Worker → UI communication is via `mpsc::Sender<WorkerMessage>`; the UI drains it in `process_messages()` every frame.
- egui panels are added in this order in `update()`: toolbar → waveform → log (bottom) → cover (right) → central. Order matters because each panel claims space from the remaining area.
- Settings are saved explicitly (Save button) — never auto-saved mid-session.

## Adding a new detection parameter

1. Add field to `Config` in `config.rs` with a sensible default.
2. Add serialisation in `ConfigFile` / `SilenceSection`.
3. Expose in `show_export_section` in `settings_dialog.rs`.
4. Wire into `DetectorConfig` in `app.rs` → `fetch_release_by_candidate` and `rescan`.

## Running tests

```bash
cargo test
```

Integration tests in `tests/` cover tag writing (`test_tagging.rs`), track helpers (`test_track.rs`), and pipe path detection (`test_pipe.rs`). They do not require Audacity to be running.

## Reporting bugs

Please include:
- OS and Audacity version
- The full log panel output (copy with the scroll area selected)
- Whether `mod-script-pipe` is confirmed enabled in Audacity Preferences → Modules
- The Diagnostics output (click the 🩺 Diagnostics button)
