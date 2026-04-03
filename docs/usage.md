# Usage Guide

## Typical workflow

```
Record vinyl in Audacity
        ↓
  🔌 Connect to Audacity
        ↓
  🔇 Detect Silence  →  track regions derived from gaps
        ↓
  🔍 Fingerprint All  →  AcoustID → MusicBrainz → Discogs
        ↓
  Review metadata in track list / detail panel
        ↓
  💾 Export All  →  pre-flight table → tagged FLAC/MP3 files
```

---

## Step-by-step

### 1. Record your vinyl in Audacity

Record the full side at your preferred sample rate (44.1 kHz / 16-bit is CD
quality; 96 kHz / 24-bit captures more for archival). Leave the project open.

### 2. Launch vripr and connect

```bash
vripr
```

Click **🔌 Connect** in the toolbar. The status indicator turns green when the
pipe handshake succeeds. If it fails, see the
[pipe setup guide](../scripts/setup_audacity_pipe.md).

### 3. Detect silence

Click **🔇 Detect Silence**. vripr calls Audacity's `SilenceFind` command using
the threshold and minimum duration from **Settings → Export**. Silence gaps are
marked with labels; vripr converts them into track regions automatically.

Adjust `silence_threshold_db` (default `-40 dBFS`) and `silence_min_duration`
(default `1.5 s`) if the detection misses gaps or creates false splits.

### 4. Adjust track regions (optional)

- **Double-click** a Title, Artist, Album, Genre, Year, or Track # cell to edit
  in-place.
- Select a row and edit the **Start / End** times in the detail panel on the
  right, then click **💾 Save**.
- Use **▲ / ▼** to reorder tracks.
- Use **✕ Remove** to delete a spurious split.
- Use **✏ Add Track** to add a region manually.

### 5. Fingerprint

Click **🔍 Fingerprint All**. For each track vripr:

1. Exports a temporary copy via Audacity's Export2 command.
2. Runs `fpcalc` to generate a Chromaprint fingerprint.
3. Queries AcoustID for a recording match.
4. Does a deep MusicBrainz lookup for full metadata.
5. Searches Discogs for album artist, genre, and year.

The progress bar in the status bar tracks the job. Rows turn **blue** once
fingerprinted. Use the **🌐 MB Lookup** or **🎵 Discogs** buttons in the
detail panel to re-query a single track manually.

### 6. Export

Click **💾 Export All**. A pre-flight table appears — all tracks are shown in
an editable grid. Make any last-minute corrections, optionally tick
*"Save as defaults"* to persist album/artist/genre to your config, then click
**Export**.

Files are written to:
```
export_dir / Artist / Album / NN - Title.flac
```

Rows turn **green** after successful export and tagging.

---

## Keyboard shortcuts

| Key | Action |
|---|---|
| `Ctrl+,` | Open Settings |
| `Delete` | Remove selected track |
| `Alt+↑ / Alt+↓` | Move selected track up/down |

---

## Output file layout

```
~/Music/Vinyl/
└── The Beatles/
    └── Abbey Road/
        ├── 01 - Come Together.flac
        ├── 02 - Something.flac
        ├── 03 - Maxwell's Silver Hammer.flac
        └── …
```

Tags written to each file:

| Tag | Source priority |
|---|---|
| `title` | AcoustID → MusicBrainz → manual |
| `artist` | AcoustID → MusicBrainz → manual |
| `album` | MusicBrainz → Discogs → manual |
| `albumartist` | Discogs → manual |
| `tracknumber` | MusicBrainz → manual |
| `genre` | MusicBrainz → Discogs → manual |
| `date` | MusicBrainz → Discogs → manual |
