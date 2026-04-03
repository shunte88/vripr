# vripr — The vinyl viper for perfect rips

**vripr** is a Python/PyQt6 desktop application that sits alongside Audacity and
automates the most tedious parts of ripping vinyl records:

- **Silence detection** — automatically splits a recorded side into individual tracks
- **Fingerprinting** — identifies each track via [Chromaprint](https://acoustid.org/chromaprint) and [AcoustID](https://acoustid.org)
- **Metadata enrichment** — queries [MusicBrainz](https://musicbrainz.org) and [Discogs](https://www.discogs.com) to fill in title, artist, album, genre, year and more
- **Tagged export** — writes fully-tagged FLAC or MP3 files, organised into `Artist/Album/` folders

---

## Quick start

```bash
pip install vripr          # or: uv add vripr
bash scripts/install_fpcalc.sh   # one-time: install Chromaprint
vripr                      # launch the app
```

See the [Installation guide](installation.md) for the full setup, including
how to enable `mod-script-pipe` in Audacity.

---

## Navigation

- [Installation](installation.md)
- [Usage guide](usage.md)
- [Configuration reference](configuration.md)
- [Contributing](../CONTRIBUTING.md)
- [Changelog](../CHANGELOG.md)
