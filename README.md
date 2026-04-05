<p align="center">
  <img src="assets/vripr.webp" alt="VRipr logo" width="160"/>
</p>

<h1 align="center">VRipr — Vinyl Ripper Helper</h1>

<p align="center">
  A desktop companion for digitising vinyl records via Audacity — silence detection, Discogs metadata, waveform editing, and one-click tagged export.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/rust-1.75%2B-orange" alt="Rust 1.75+"/>
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT License"/>
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-lightgrey" alt="Platform"/>
</p>

---

## Overview

VRipr sits alongside Audacity while you rip a vinyl record. It:

1. **Connects** to Audacity via the scripting pipe and exports the current project to a clean analysis WAV — capturing any edits you've already made (needle-drop removal, fades, etc.)
2. **Fetches** the album's track listing and durations from Discogs
3. **Detects** track boundaries with a multi-pass silence scanner, automatically retrying with shorter gap thresholds until the count matches Discogs
4. **Shows** an interactive waveform with draggable track markers so you can fine-tune any boundaries the algorithm gets wrong
5. **Sets labels** in Audacity for a final visual review, then **exports** each track as a tagged FLAC/MP3/WAV/OGG with full metadata including a `DISCOGS_RELEASEID` custom tag and `folder.jpg` cover art

---

## Screenshots

### Main window — 10 tracks detected, ready to export

![Main window](assets/Screenshot%20From%202026-04-04%2012-45-08.png)

The waveform panel shows the full recording with coloured track regions and draggable boundaries. The track table carries Discogs metadata. Cover art is displayed on the right.

### Export in progress

![Export in progress](assets/Screenshot%20From%202026-04-04%2012-49-38.png)

Progress indicator, per-track log output, and export paths are shown in real time.

### Discogs release picker

![Discogs picker](assets/Screenshot%20From%202026-04-04%2021-02-08.png)

Search results include label, format, year, and a 🌐 button to open the release page directly in your browser. Hover over any row for full release details.

### Audacity with VRipr labels

![Audacity with labels](assets/Screenshot%20From%202026-04-04%2012-48-16.png)

After clicking **Set Labels**, Audacity shows a label track with the detected track boundaries. Drag them in Audacity if needed, then switch back to VRipr and click **Export All**.

### Settings

![Settings](assets/Screenshot%20From%202026-04-04%2021-08-58.png)

---

## Prerequisites

| Requirement | Notes |
|---|---|
| **Audacity 3.x** | With `mod-script-pipe` enabled — see below |
| **Rust 1.75+** | Build-time only; no runtime dependency |
| **Discogs account** | Free personal access token required |

### Enabling mod-script-pipe in Audacity

1. Open Audacity → **Edit → Preferences → Modules**
2. Set `mod-script-pipe` to **Enabled**
3. Restart Audacity

VRipr communicates with Audacity through named pipes. If the connection fails, check the **Diagnostics** button — it reports the expected pipe paths.

---

## Installation

### Build from source

```bash
git clone https://github.com/yourname/vripr.git
cd vripr
cargo build --release
./target/release/vripr
```

The release binary has LTO enabled and is self-contained. Copy it anywhere on your `PATH`.

### Linux desktop integration (optional)

```bash
# Install binary
sudo install -Dm755 target/release/vripr /usr/local/bin/vripr

# Desktop entry
cat > ~/.local/share/applications/vripr.desktop <<EOF
[Desktop Entry]
Name=VRipr
Comment=Vinyl Ripper Helper
Exec=vripr
Icon=/path/to/assets/vripr.webp
Type=Application
Categories=Audio;
EOF
```

---

## Configuration

Config is stored at `~/.config/vripr/vripr.toml` (created automatically on first save).

| Setting | Description |
|---|---|
| **Discogs Token** | Personal access token from [discogs.com/settings/developers](https://www.discogs.com/settings/developers) |
| **Export Format** | FLAC (default), MP3, WAV, OGG |
| **Export Directory** | Root output folder; tracks are written to `{dir}/{Artist}/{Album}/{NN} - {Title}.{ext}` |
| **Silence Threshold** | dB level below which audio is considered silence (first detection pass) |
| **Min inter-track silence** | Shortest gap that registers as a track boundary (first pass; retries shorten this automatically) |
| **Min track duration** | Regions shorter than this are discarded as noise (first pass; retries lower this automatically) |
| **Adaptive threshold** | Measures the recording's noise floor and sets the threshold automatically |
| **Track Number Format** | Alpha (A1, B2 …) for vinyl positions or Numeric (1, 2, 3 …) |

---

## Workflow

### 1. Record and edit in Audacity

Rip your vinyl into Audacity as normal. Remove the needle drop and any leading/trailing silence if you wish — VRipr exports the project at connect time, so your edits are included.

### 2. Connect

Click **Connect** in the toolbar. VRipr opens the Audacity scripting pipe, exports the full project to a temporary WAV, and builds the waveform display. This takes 30–90 seconds for a full LP side.

### 3. Fetch the release

Fill in **Artist** and **Album** in the Apply-to-all strip and click **📀 Fetch Release**. Choose the correct release from the Discogs picker — use the 🌐 button to verify on the Discogs website if unsure.

VRipr runs silence detection against the analysis WAV, using up to five automatic retries with progressively shorter gap thresholds until the detected track count matches Discogs. It logs each pass so you can see what happened.

### 4. Review the waveform

The waveform panel shows coloured regions for each detected track. If any boundary is wrong:

- **Drag** the boundary line left or right to adjust it. Adjacent track boundaries move together.
- **Right-click** a track region to **📌 Pin** it — pinned tracks are preserved when you re-scan.
- **Drag empty space** to draw a selection, then click **➕ Add Track** to insert a new track at exactly that position.
- Click **🔄 Re-scan** to run detection again while keeping pinned tracks.

### 5. Set labels in Audacity

Click **🏷 Set Labels**. VRipr clears any existing label track in Audacity, writes new labels from the track table, then reads them back to confirm they took. Switch to Audacity for a final visual check — drag labels if needed.

### 6. Export

Click **💾 Export All**. VRipr:

- Validates that label count and titles match (non-blocking warning if not)
- Selects each time region in Audacity and exports it via `Export2:`
- Writes full ID3/Vorbis/FLAC tags including `DISCOGS_RELEASEID`
- Deposits `folder.jpg` in the album directory after the first track completes

Output structure:
```
{Export Directory}/
  {Artist}/
    {Album}/
      folder.jpg
      01 - Track Title.flac
      02 - Track Title.flac
      …
```

---

## Detection algorithm

VRipr uses a whole-file RMS silence scanner (not Audacity's `LabelSounds`). The scanner:

1. Decodes the analysis WAV to per-window RMS values (50 ms windows)
2. Applies hysteresis to avoid rapid toggling on borderline signals
3. Bridges small gaps (vinyl crackle, pops) shorter than the gap-fill threshold
4. Requires a minimum silence duration for a gap to count as a track boundary
5. Discards regions shorter than the minimum track duration

If the detected count doesn't match Discogs, it retries up to four more times, halving the minimum silence and track duration each pass, until the count matches or all passes are exhausted.

The gap-fill threshold is always kept below the minimum silence threshold — this prevents the filler from bridging the very gaps it's trying to detect, which is a common failure mode in naive silence detectors.

---

## Audacity compatibility notes

- VRipr uses `Export2:` via the scripting pipe — this requires Audacity 3.x with `mod-script-pipe` enabled.
- The pipe is synchronous: VRipr waits for each command to complete before proceeding.
- Labels are written with `AddLabel:` + `SetLabel:` and verified by reading them back with `GetInfo: Type=Labels`.
- The analysis WAV is written to `/tmp/vripr_analysis_{pid}.wav` and is safe to delete after a session.

---

## Contributing

Bug reports and pull requests are welcome. Please open an issue first for significant changes.

See [CONTRIBUTING.md](CONTRIBUTING.md) for build instructions, code structure, and conventions.

---

## License

MIT — see [LICENSE](LICENSE).
