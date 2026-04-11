<p align="center">
  <img src="assets/vripr.webp" alt="VRipr logo" width="160"/>
</p>

<h1 align="center">VRipr — Master Vinyl Rippage</h1>

<p align="center">
  A desktop companion for digitising vinyl records via Audacity — silence detection, Discogs metadata, waveform editing, and one-click tagged export.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/rust-1.87%2B-orange" alt="Rust 1.87+"/>
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT License"/>
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-lightgrey" alt="Platform"/>
  <img src="version.svg" alt="Version"/>
</p>

<p align="center">
  <a href="https://www.buymeacoffee.com/shunte88">
    <img src="assets/bmc-red-button.svg" alt="Buy me a coffee" height="40"/>
  </a>
</p>

---

## Overview

VRipr sits alongside Audacity while you rip a vinyl record. It:

1. **Connects** to Audacity via the scripting pipe and exports the current project to a clean analysis WAV — capturing any edits you've already made (needle-drop removal, fades, etc.)
2. **Fetches** the album's track listing and durations from Discogs
3. **Detects** track boundaries using one of three algorithms — RMS energy, Spectral flatness, or an adaptive HMM — with multi-pass retries and Discogs-guided anchoring when durations are available
4. **Shows** an interactive waveform with draggable track markers and a right-click context menu for auditioning and pinning boundaries in Audacity
5. **Sets labels** in Audacity for a final visual review, then **exports** each track as a tagged FLAC/MP3/WAV/OGG with full metadata including multi-value `GENRE` and `ARTIST` tags, a `DISCOGS_RELEASEID` custom tag, and `folder.jpg` cover art

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
| **Rust 1.87+** | Build-time only; no runtime dependency |
| **Discogs account** | Free personal access token required |

### Enabling mod-script-pipe in Audacity

1. Open Audacity → **Edit → Preferences → Modules**
2. Set `mod-script-pipe` to **Enabled**
3. Restart Audacity

VRipr communicates with Audacity through named pipes. If the connection fails, check the **Diagnostics** button — it reports the expected pipe paths.

---

## Installation

### Pre-built binaries

GitHub Actions builds a native binary for every push to `main` and on every `v*` tag:

| Platform | Asset |
|---|---|
| Linux x86_64 | `vripr-linux-x86_64` |
| macOS Apple Silicon | `vripr-macos-arm64` |
| macOS Intel | `vripr-macos-x86_64` |
| Windows x86_64 | `vripr-windows-x86_64.exe` |

Download from the [Releases](../../releases) page. All binaries are self-contained — no runtime dependencies, no system libraries required.

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
Comment=Master Vinyl Rippage
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
| **Export Directory** | Root output folder; the template is joined to this path |
| **Path Template** | Relative path template for exported files — see [Path Template](#path-template) below |
| **Album Name Format** | Token template for the Album metadata tag — see [Album Name Format](#album-name-format) below; leave blank to use the album name unchanged |
| **Default Comments** | Comment tag embedded in all exported files; overridden per-track in the edit panel |
| **Silence Threshold** | dB level below which audio is considered silence (first detection pass) |
| **Min inter-track silence** | Shortest gap that registers as a track boundary (first pass; retries shorten this automatically) |
| **Min track duration** | Regions shorter than this are discarded as noise (first pass; retries lower this automatically) |
| **Adaptive threshold** | Measures the recording's noise floor and sets the threshold automatically |
| **Detection Method** | `RMS`, `Spectral`, or `HMM` — see [Detection algorithm](#detection-algorithm) |
| **Flatness threshold** | Spectral mode only: flatness above this is treated as noise (0.5–0.99, default 0.85) |
| **Track Number Format** | Alpha (A1, B2 …) for vinyl positions or Numeric (1, 2, 3 …) |
| **Genre Map File** | Path to a custom `genre.dat` file — see [Genre normalisation](#genre-normalisation) below |

---

## Genre normalisation

VRipr normalises genre tags on export using a mapping table (`genre.dat`). The built-in table ships with ~640 entries drawn from the [sanitizegenre](https://github.com/shunte88/sanitizegenre) project and covers:

- **Abbreviations** — `HH` → `Hip-Hop;Hip Hop`, `DT` → `Dub Techno`, `E` → `Electronic` …
- **Typo corrections** — `Trchno` → `Techno`, `Popo` → `Pop`, `Shoegzae` → `Shoegaze` …
- **Case normalisation** — `TECHNO` → `Techno`, `HOUSE` → `House` …
- **Multi-genre expansion** — `Folk Pop` → `Folk Pop;Folk;Pop`, `Prog Rock` → `Prog Rock;Prog-Rock;Progressive Rock;Progressive` …
- **Discogs styles** — when a release is fetched, both the `genres[]` and `styles[]` arrays are combined into a semicolon-delimited string (e.g. `Electronic;Deep House;House`) so the full genre hierarchy is preserved through to the exported tags

On export each semicolon-delimited component is looked up, expanded, deduplicated, and written as one `GENRE` tag per value. Players that support multi-value tags (foobar2000, beets, Picard, Kodi, Plex…) will display and filter on all genres. Single-value players receive the first genre.

### Bespoke genre maps

You can supply your own `genre.dat` to match your personal library conventions, language preferences, or custom taxonomy. The format is one mapping per line:

```
# comment lines start with #
SourceString|Target1;Target2;Target3
```

- **Source** is the raw string that appears in the genre field (exact match first, case-insensitive fallback).
- **Target** is a semicolon-separated list of canonical output values.
- Unknown genres pass through unchanged — you only need to list the exceptions.

Example entries:

```
# French genre names
Électronique|Electronic
Jazz Fusion|Jazz Fusion;Jazz
Hip Hop|Hip-Hop;Hip Hop;Rap

# Studio-internal codes
EXP|Experimental;Electronic
CJAZZ|Contemporary Jazz;Jazz
```

To activate a custom file: open **Settings → Genre Map File**, click **…**, and select your `.dat` file. The map is loaded immediately on Save and persisted to `vripr.toml`. Click **✕** in the field to revert to the built-in mappings.

---

## Multi-value artist tags

A release may credit multiple artists on a single track, or an artist may be known by more than one name. VRipr supports this by treating the **Artist** field as a semicolon-delimited list:

```
Daniel Mana;Mana
```

On export, each entry is written as a separate `ARTIST` tag — players that support multi-value tags (foobar2000, beets, Picard, Kodi, Plex…) will index and display all contributors. The **Album Artist** field is treated identically.

Enter multiple artists in the edit panel using `;` as the delimiter. No sanitisation map is applied — the values are written exactly as entered after splitting and trimming whitespace.

---

## Workflow

### 1. Record and edit in Audacity

Rip your vinyl into Audacity as normal. Remove the needle drop and any leading/trailing silence if you wish — VRipr exports the project at connect time, so your edits are included.

### 2. Connect

Click **Connect** in the toolbar. VRipr opens the Audacity scripting pipe, exports the full project to a temporary WAV, and builds the waveform display. This takes 30–90 seconds for a full LP side.

### 3. Fetch the release

Fill in **Artist** and **Album** in the Apply-to-all strip and click **📀 Fetch Release**. Choose the correct release from the Discogs picker — use the 🌐 button to verify on the Discogs website if unsure.

VRipr then runs track detection (see [Detection algorithm](#detection-algorithm) below). The log panel shows what happened at each step.

If the release has no track durations (titles only), VRipr still populates the track table with placeholder entries — all timings are set to zero and can be adjusted manually in the edit panel.

### 4. Review the waveform

The track table's **Time** column shows `start–end (duration)` at a glance.

The waveform panel shows coloured regions for each detected track. The amplitude colour gradient — teal for quiet passages, through green and yellow to red at loud peaks — gives an immediate sense of the dynamic shape of each track. If any boundary is wrong:

- **Drag** the boundary line left or right to adjust it. Adjacent track boundaries move together.
- **Right-click** anywhere in the waveform to open the context menu:
  - **▶ Play Track N** — sends a `SelectTime:` + `Play:` command to Audacity to audition that region
  - **↦ Pin start here** / **↤ Pin end here** — moves the boundary to the click position
  - **📌 Pin/Unpin** — locks a track so it is preserved on re-scan
  - **⏹ Stop playback** — stops Audacity playback
- **Drag empty space** to draw a selection, then click **➕ Add Track** to insert a new track at exactly that position.
- Click **🔄 Re-scan** to run detection again while keeping pinned tracks.

#### Editing track timing

Double-click any row in the track table to open the **edit panel**. Three time fields are available:

| Field | Behaviour |
|---|---|
| **Start** | Edit in `MM:SS.ss` or raw seconds. Clamped to 0 ↔ End−0.1 s. |
| **End** | Edit in `MM:SS.ss` or raw seconds. Automatically pushes the next track's Start forward if they would overlap. |
| **Duration** | Edit in `MM:SS.ss` or raw seconds. Sets End = Start + Duration; cascades to the next track with the same overlap rule. |

All three fields accept `MM:SS.ss` notation and display it consistently after editing.

### Processing sides separately (the prescribed workflow)

> **One side per Audacity session is strongly recommended.** The silence detector works on whatever audio is loaded — if you record multiple sides as a single continuous file, it must find twice (or four times) as many tracks with no knowledge of where the physical side-break is. Detection reliability drops significantly above ~6 tracks. For double albums, triple albums, and boxed sets, processing one side per session is not just a suggestion — it is effectively required for consistent results.

The **Side:** selector in the toolbar handles the metadata side of this workflow. It appears automatically when the loaded Discogs release has more than one vinyl side. For double albums (4 sides) and above it groups sides into discs — *Disc 1 / Side A*, *Disc 1 / Side B*, *Disc 2 / Side C* — so you always know which disc a side belongs to.

**Workflow for a double album (4 sides, 2 discs):**

1. Record Disc 1 Side A into Audacity. Connect → detect → select **Side A (Disc 1)** → export. Done for Side A.
2. New Audacity session. Record Disc 1 Side B. Connect → detect → select **Side B (Disc 1)** → export.
3. Repeat for Disc 2 Sides C and D.

**Ripping out of order** (e.g., Side B before Side A):

1. Load your Side B recording and connect.
2. Fetch the release. Select **Side B** from the combo.
3. VRipr filters the Discogs tracklist to Side B only and calibrates the retry loop against that side's track count.
4. If you change the side selection after detection has already run, VRipr immediately re-assigns Discogs metadata to the existing detected boundaries without re-running audio analysis. Click **🔄 Re-scan** to redo detection calibrated to the new side's expected count.

Track numbers are assigned from the Discogs vinyl position (`B1`, `B2`, …) regardless of the order you rip, so exported files sort correctly in your library.

> **What the Side selector does not do**: it does not make multi-side audio easier to analyse. If you've recorded Sides A and B as one 90-minute file, selecting "Side A" only filters the *metadata* — the detector still has to find all the tracks in 90 minutes of audio. The best approach is to rip all sides in sequence as a single continuous recording, then use the **All** option in the Side selector — VRipr will detect tracks across the full recording and match them to the complete tracklist in order.

### 5. Set labels in Audacity

Click **🏷 Set Labels**. VRipr clears any existing label track in Audacity, writes new labels from the track table, then reads them back to confirm they took. Switch to Audacity for a final visual check — drag labels if needed.

#### Fine-tuning boundaries in Audacity

If any track boundary needs adjusting, Audacity's label editor gives you a visual waveform to work against:

1. Click **🏷 Set Labels** to push the current boundaries into Audacity.
2. Switch to Audacity. Drag the label edges left or right until each boundary sits exactly at the silence gap.
3. Switch back to VRipr and click **⬇ Get Labels**. VRipr reads the adjusted positions back and updates the track table in place — all metadata (title, artist, album, etc.) is preserved, only the start/end times change.
4. Repeat as needed. **Set Labels → tweak in Audacity → Get Labels** is a fast loop with no re-detection required.

This is particularly useful for albums with ambiguous silence gaps, mid-track fades, or live recordings where applause bleeds into the next track.

### 6. Export

Click **💾 Export All**. VRipr:

- Validates that label count and titles match (non-blocking warning if not)
- Selects each time region in Audacity and exports it via `Export2:`
- Writes full ID3/Vorbis/FLAC tags including `DISCOGS_RELEASEID`, multi-value `GENRE` and `ARTIST`
- Deposits `folder.jpg` in the album directory after the first track completes

Output structure follows your Path Template. The default template produces:

```
{Export Directory}/
  {Album Artist}/
    {Album}/
      folder.jpg
      01 - Track Title.flac
      02 - Track Title.flac
      …
```

---

## Path Template

The **Path Template** setting (under Export & Detection in Settings) controls where each exported file is placed relative to the Export Directory.

### Tokens

| Token | Value |
|---|---|
| `{title}` | Track title |
| `{artist}` | Track artist (defaults to Album Artist) |
| `{album}` | Album title |
| `{album_artist}` | Album-level artist |
| `{genre}` | Genre |
| `{year}` | Release year |
| `{tracknum}` | Zero-padded track number (e.g. `01`, `A1`, `B2`) |
| `{composer}` | Composer |
| `{country}` | Release country as Discogs names it (e.g. `UK`, `Germany`) |
| `{country_iso}` | Release country as ISO 3166-1 alpha-2 (e.g. `GB`, `DE`) |
| `{catalog}` | Label catalogue number (from Discogs) |
| `{label}` | Record label (from Discogs) |
| `{discogs_id}` | Discogs release ID |

### Bracket collapsing

Wrap tokens in `[...]` to make them conditional: if the token resolves to an empty string, the entire `[...]` group (including brackets) is removed. This avoids orphaned `[]` in directory names when metadata is missing.

### Examples

Default:
```
{album_artist}/{album}/{tracknum} - {title}
```

With ISO country code and catalogue number:
```
{album_artist}/{album} [{country_iso}][{catalog}]/{tracknum} - {title}
```
→ `Miles Davis/Kind of Blue [US][CL 1355]/01 - So What.flac`
→ (if country is empty) `Miles Davis/Kind of Blue [CL 1355]/01 - So What.flac`
→ UK release: `Miles Davis/Kind of Blue [GB][CL 1355]/01 - So What.flac`

### Album Name Format

The **Album Name Format** setting (also under Export & Detection) lets you customise the Album metadata tag written to every exported file using the same `{token}` syntax as the Path Template. Leave it blank and the album name from Discogs (or whatever you've typed in) is used verbatim. Set a format and it is expanded per-track at export time.

Useful for collectors who want release context baked directly into the tag rather than only into the folder structure:

```
{album} [{country_iso}][{catalog}]
```
→ `Kind of Blue [GB][CBS 62066]`

```
{album} ({year})
```
→ `Kind of Blue (1959)`

Bracket collapsing applies here too — `[{country_iso}]` disappears cleanly if the country is unknown.

---

## Detection algorithm

VRipr ships three track-boundary detectors plus a duration-guided pass. All share the same multi-pass retry loop.

### Duration-guided pre-pass

When every expected track has a duration in the Discogs data, VRipr runs a **guided** detection first: it uses the Discogs durations as timing anchors, scanning only the narrow windows around where each track boundary is expected to be. This is faster and more reliable than a blind full-file scan when the timing data is good. If the guided pass produces the exact expected track count it is used directly; otherwise VRipr falls back to the blind retry loop below.

### RMS (default)

The classic energy-based scanner:

1. Decodes the analysis WAV to per-window RMS values (50 ms windows)
2. Applies hysteresis to avoid rapid toggling on borderline signals
3. Bridges small gaps (vinyl crackle, pops) shorter than the gap-fill threshold
4. Requires a minimum silence duration for a gap to count as a track boundary
5. Discards regions shorter than the minimum track duration

Works well for clean pressings where the inter-track groove is meaningfully quieter than the music.

### Spectral (noise-aware)

A combined energy + spectral-flatness scanner, better suited to noisy pressings where the inter-track groove is *loud* but acoustically different from music.

The key insight: **surface noise is spectrally flat** (energy spread roughly evenly across all frequencies, like white noise) while **music is spectrally peaked** (energy concentrated in harmonically related bands). Spectral flatness — the ratio of the geometric mean to the arithmetic mean of the power spectrum — measures this distinction on a 0–1 scale: 0 = perfectly tonal, 1 = white noise. Inter-track groove noise typically scores 0.80–0.95; music 0.10–0.50.

For each 50 ms window the detector computes:
1. **RMS** — as in the energy-only scanner
2. **Spectral flatness** via FFT (Hann-windowed, zero-padded to the next power of two, positive-frequency half-spectrum). A ±2-window (≈250 ms) rolling average smooths the flatness signal before thresholding.

A frame is classified as *between tracks* if its RMS falls below the energy threshold (ordinary silence), **or** its flatness exceeds the flatness threshold while energy is still present (energetic surface noise).

**When to use it:** if your pressing is noisy, the RMS detector reports more tracks than expected, and lowering the threshold starts eating into quiet musical passages. The flatness threshold defaults to 0.85.

### HMM (adaptive)

A two-state Hidden Markov Model over `(RMS dB, spectral flatness)` features. Unlike the threshold-based detectors, the HMM learns emission parameters from the recording itself:

- The **bottom 15 %** of frames by RMS are treated as silence examples → Gaussian model for the SILENCE state
- The **top 40 %** by RMS are treated as music examples → Gaussian model for the SOUND state
- Transition probabilities are derived from the configured minimum silence / minimum track duration

Viterbi decoding finds the globally most-likely sequence of SILENCE/SOUND states. Because the HMM assigns a cost to switching states, momentary level dips mid-track — a quiet passage, a sudden dynamic contrast — no longer produce spurious track splits. The same post-processing (gap-fill, minimum duration, de-overlap) runs on the extracted sound regions.

**When to use it:** when RMS and Spectral both split tracks incorrectly at quiet passages and manual threshold tuning isn't converging. The HMM needs no threshold configuration — it adapts to each recording.

**Implementation:** pure Rust, no additional dependencies. Uses the same FFT-based feature extraction as the Spectral detector.

### Shared retry loop and fallbacks

If the detected count doesn't match Discogs, all detectors retry up to four more times, progressively shortening the minimum silence and minimum track duration each pass.

After all retries:

| Outcome | Action |
|---|---|
| Count matches | Use detected boundaries |
| Too many detected | Truncate to expected count (discard smallest extra regions) |
| Too few detected | Fall back to `split_by_discogs_durations` — chain tracks by Discogs durations |
| No audio file, durations available | Chain tracks by Discogs durations |
| No audio file, no durations | Create title-only placeholder entries (times = 0); edit manually |

---

## Audacity compatibility notes

- VRipr uses `Export2:` via the scripting pipe — this requires Audacity 3.x with `mod-script-pipe` enabled.
- The pipe is synchronous: VRipr waits for each command to complete before proceeding.
- Waveform playback (`▶ Play Track`) sends `SelectTime:` + `Play:` commands to Audacity — Audacity must be open and connected for this to work.
- Labels are written with `AddLabel:` + `SetLabel:` and verified by reading them back with `GetInfo: Type=Labels`.
- The analysis WAV is written to `/tmp/vripr_analysis_{pid}.wav` and is safe to delete after a session.

---

## Contributing

Bug reports and pull requests are welcome. Please open an issue first for significant changes.

See [CONTRIBUTING.md](CONTRIBUTING.md) for build instructions, code structure, and conventions.

---

## Like The App - Git The Shirt

Team Badger shirts and other goodies are available at [shunte88](https://www.zazzle.com/team_badger_t_shirt-235604841593837420)

---

## License

MIT — see [LICENSE](LICENSE).
