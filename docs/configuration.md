# Configuration Reference

vripr stores its configuration at:

| OS | Path |
|---|---|
| Linux / macOS | `~/.vripr/vripr.ini` |
| Windows | `%USERPROFILE%\.vripr\vripr.ini` |

The file is created with defaults on first run. All settings are also
accessible via **⚙ Settings** in the app.

---

## Full reference

```ini
[vinyl_ripper]

# ── API Keys ────────────────────────────────────────────────────────────────
# Free AcoustID application API key — https://acoustid.org/login
acoustid_api_key =

# Discogs personal access token — https://www.discogs.com/settings/developers
discogs_token =

# MusicBrainz user-agent string (app/version contact)
# Must be set to a non-default value for production use per MusicBrainz policy
mb_user_agent = VinylRipper/1.0 ( vinyl-ripper@example.com )

# ── Export ──────────────────────────────────────────────────────────────────
# Output format: flac | mp3 | wav | ogg
export_format = flac

# Root export directory.
# Sub-folders are created automatically: export_dir/Artist/Album/
export_dir = ~/Music/Vinyl

# ── Silence Detection ───────────────────────────────────────────────────────
# Threshold in dBFS — audio quieter than this is treated as silence.
# Typical values: -35 (noisy pressing) to -50 (quiet pressing / digital)
silence_threshold_db = -40

# Minimum gap duration in seconds before vripr considers it a track boundary.
# Increase for records with very short inter-track gaps.
silence_min_duration = 1.5

# ── Session Defaults ────────────────────────────────────────────────────────
# Pre-populate the export dialog for every track in the session.
# Useful for albums where fingerprinting returns no match.
# Leave blank to prompt each time.
default_artist =
default_album =
default_album_artist =
default_genre =
default_year =
```

---

## Tuning silence detection

| Pressing type | Suggested `silence_threshold_db` |
|---|---|
| Pristine / digital remaster | `-50` |
| Average consumer record | `-40` (default) |
| Worn / noisy surface noise | `-32` to `-35` |

If vripr creates too many false track splits from surface noise, *raise* the
threshold (make it less negative, e.g. `-35`). If it misses genuine gaps,
*lower* it (e.g. `-45`).

`silence_min_duration` should be at least as long as the longest breath between
phrases but shorter than the shortest inter-track gap. `1.5` seconds works well
for most records.
