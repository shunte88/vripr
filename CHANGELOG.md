# Changelog

All notable changes to **vripr** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

## [0.1.0] — 2025-04-02

### Added
- Initial release — *The vinyl viper for perfect rips*
- PyQt6 GUI with dark Catppuccin-inspired theme
- Audacity integration via `mod-script-pipe`
- Automatic silence detection using Audacity's `SilenceFind`
- Label import — derives track regions from silence gaps or named content labels
- Chromaprint / AcoustID audio fingerprinting (via `fpcalc` CLI + `pyacoustid`)
- MusicBrainz metadata enrichment (title, artist, album, track number, year, genre)
- Discogs metadata enrichment (album artist, genre, year, release ID)
- Manual MusicBrainz and Discogs lookup buttons per track
- Inline-editable `QTableView` track list with colour-coded status rows
- Detail panel with Save / MB Lookup / Discogs buttons
- Export-all pre-flight review dialog with bulk inline editing
- `mutagen`-powered tag writing for FLAC, MP3, and other formats
- `QThread`-based fingerprint and export workers — UI never blocks
- Progress bar in status bar during background jobs
- Settings dialog — API Keys / Export / Defaults tabs; API keys masked with show/hide toggle
- INI config at `~/.vripr/vripr.ini` with defaults pre-populated from config
- `uv`-managed project with `pyproject.toml`, `hatchling` build backend
- `ruff`, `mypy`, `pytest` + `pytest-qt` dev toolchain
- GitHub Actions CI — lint, type-check, test (3 OS × 3 Python versions), build
- GitHub Actions release workflow — tag-triggered PyPI publish via trusted publishing

[Unreleased]: https://github.com/YOUR_GITHUB_USERNAME/vripr/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/YOUR_GITHUB_USERNAME/vripr/releases/tag/v0.1.0
