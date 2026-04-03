# vripr — The vinyl viper for perfect rips

> A PyQt6 desktop assistant that connects to Audacity, fingerprints your vinyl
> recordings with **Chromaprint / AcoustID**, pulls metadata from **MusicBrainz**
> and **Discogs**, and exports beautifully tagged FLAC / MP3 files — automatically.

[![CI](https://github.com/shunte88/vripr/actions/workflows/ci.yml/badge.svg)](https://github.com/shunte88/vripr/actions/workflows/ci.yml)
[![PyPI](https://img.shields.io/pypi/v/vripr)](https://pypi.org/project/vripr/)
[![Python](https://img.shields.io/pypi/pyversions/vripr)](https://pypi.org/project/vripr/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

---

<img width="800" src="assets/vripr.webp" align="center" />

## ✨ Features

- **Silence detection** — calls Audacity's `SilenceFind` to split a recorded
  vinyl side into individual track regions automatically
- **Chromaprint fingerprinting** — uses `fpcalc` + the AcoustID API to identify
  each track by its audio content
- **MusicBrainz enrichment** — deep lookup for title, artist, album, track
  number, year, and genre
- **Discogs enrichment** — fills album artist, genre, year, and release ID
- **Tagged export** — writes FLAC / MP3 / WAV / OGG with full metadata via
  `mutagen`, organised into `Artist / Album /` folders
- **Dark Qt UI** — editable track table, detail panel, pre-flight export review,
  progress bar, non-blocking `QThread` workers

---

## 🚀 Quick start

```bash
# 1. Install vripr
pip install vripr          # or: uv tool install vripr

# 2. Install fpcalc (Chromaprint CLI)
#    macOS:
brew install chromaprint
#    Ubuntu/Debian:
sudo apt install libchromaprint-tools

# 3. Enable mod-script-pipe in Audacity
#    Edit → Preferences → Modules → mod-script-pipe → Enabled  →  restart Audacity

# 4. Launch
vripr
```

---

## 📸 Workflow

```
Record vinyl side in Audacity
        ↓
  🔌 Connect to Audacity
        ↓
  🔇 Detect Silence → track regions derived from gap labels
        ↓
  🔍 Fingerprint All → AcoustID → MusicBrainz → Discogs
        ↓
  Review / edit metadata inline in the track table
        ↓
  💾 Export All → pre-flight review table → tagged files on disk
```

---

## 🛠 Development setup

```bash
git clone https://github.com/shunte88/vripr.git
cd vripr

# Install uv (if not already)
curl -LsSf https://astral.sh/uv/install.sh | sh

# Create virtualenv + install all deps
uv sync --all-extras

# Run
uv run vripr

# Lint + type-check + test
uv run ruff check src/ tests/
uv run mypy src/vripr/
uv run pytest
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full guide.

---

## ⚙️ Configuration

vripr stores settings at `~/.vripr/vripr.ini`. Key options:

```ini
[vinyl_ripper]
acoustid_api_key     = YOUR_KEY      # acoustid.org/login (free)
discogs_token        = YOUR_TOKEN    # discogs.com/settings/developers (free)
export_format        = flac          # flac | mp3 | wav | ogg
export_dir           = ~/Music/Vinyl
silence_threshold_db = -40           # dBFS
silence_min_duration = 1.5           # seconds
default_artist       =
default_album        =
```

All settings are also editable via **⚙ Settings** in the app.

---

## 📦 Requirements

| Runtime | Version |
|---|---|
| Python | ≥ 3.10 |
| PyQt6 | ≥ 6.6 |
| pyacoustid | ≥ 1.3 |
| musicbrainzngs | ≥ 0.7 |
| discogs-client | ≥ 2.4 |
| mutagen | ≥ 1.47 |
| fpcalc (Chromaprint CLI) | any |
| Audacity | ≥ 3.0 with mod-script-pipe enabled |

---

## 📄 License

[MIT](LICENSE) © vripr contributors
