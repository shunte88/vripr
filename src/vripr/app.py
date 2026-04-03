#!/usr/bin/env python3
"""
vripr — The vinyl viper for perfect rips  (Qt edition)
=============================================================
Connects to Audacity via mod-script-pipe, detects silence-separated tracks,
fingerprints each one with Chromaprint/AcoustID, queries MusicBrainz and
Discogs for metadata, and exports fully-tagged FLAC/MP3 files.

Requirements:
    pip install PyQt6 pyacoustid musicbrainzngs discogs-client mutagen

Plus Chromaprint CLI (fpcalc):
    macOS:  brew install chromaprint
    Linux:  sudo apt install libchromaprint-tools
    Windows: download from acoustid.org/chromaprint and add to PATH

Audacity must be running with mod-script-pipe enabled:
    Edit → Preferences → Modules → mod-script-pipe → Enabled  (then restart)
"""

from __future__ import annotations

import configparser
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

# ── PyQt6 ─────────────────────────────────────────────────────────────────
from PyQt6.QtCore import (
    Qt, QAbstractTableModel, QModelIndex, QThread, pyqtSignal, QObject,
    QSortFilterProxyModel, QItemSelectionModel,
)
from PyQt6.QtGui import QColor, QFont, QIcon, QPalette, QAction
from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QWidget, QDialog, QDialogButtonBox,
    QVBoxLayout, QHBoxLayout, QGridLayout, QFormLayout,
    QSplitter, QTabWidget, QGroupBox,
    QLabel, QLineEdit, QPushButton, QComboBox, QCheckBox,
    QTableView, QHeaderView, QTextEdit,
    QToolBar, QStatusBar, QProgressBar,
    QMessageBox, QFileDialog,
    QSizePolicy, QAbstractItemView,
)

# ── optional heavy imports (graceful degradation) ─────────────────────────
try:
    import acoustid
    HAS_ACOUSTID = True
except ImportError:
    HAS_ACOUSTID = False

try:
    import musicbrainzngs as mb
    HAS_MB = True
except ImportError:
    HAS_MB = False

try:
    import discogs_client
    HAS_DISCOGS = True
except ImportError:
    HAS_DISCOGS = False

try:
    from mutagen.flac import FLAC
    from mutagen.id3 import ID3, TIT2, TPE1, TALB, TPE2, TCON, TRCK
    from mutagen import File as MutagenFile
    HAS_MUTAGEN = True
except ImportError:
    HAS_MUTAGEN = False


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Dark stylesheet
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

DARK_QSS = """
QWidget {
    background-color: #1e1e2e;
    color: #cdd6f4;
    font-family: "Segoe UI", "Inter", "Helvetica Neue", sans-serif;
    font-size: 13px;
}
QMainWindow, QDialog {
    background-color: #1e1e2e;
}
QToolBar {
    background-color: #181825;
    border-bottom: 1px solid #313244;
    spacing: 4px;
    padding: 4px 6px;
}
QToolBar QToolButton {
    background-color: #313244;
    color: #cdd6f4;
    border: 1px solid #45475a;
    border-radius: 5px;
    padding: 5px 10px;
    min-width: 80px;
}
QToolBar QToolButton:hover  { background-color: #45475a; }
QToolBar QToolButton:pressed { background-color: #585b70; }
QToolBar QToolButton:disabled { color: #585b70; }
QPushButton {
    background-color: #313244;
    color: #cdd6f4;
    border: 1px solid #45475a;
    border-radius: 5px;
    padding: 5px 14px;
    min-width: 72px;
}
QPushButton:hover   { background-color: #45475a; border-color: #89b4fa; }
QPushButton:pressed { background-color: #585b70; }
QPushButton#accent  { background-color: #89b4fa; color: #1e1e2e; border-color: #89b4fa; font-weight: bold; }
QPushButton#accent:hover   { background-color: #b4befe; }
QPushButton#accent:pressed { background-color: #74c7ec; }
QPushButton#danger  { background-color: #f38ba8; color: #1e1e2e; border-color: #f38ba8; }
QPushButton#danger:hover   { background-color: #eba0ac; }
QLineEdit, QComboBox, QTextEdit {
    background-color: #313244;
    color: #cdd6f4;
    border: 1px solid #45475a;
    border-radius: 4px;
    padding: 4px 8px;
    selection-background-color: #89b4fa;
    selection-color: #1e1e2e;
}
QLineEdit:focus, QComboBox:focus { border-color: #89b4fa; }
QComboBox::drop-down { border: none; width: 20px; }
QComboBox QAbstractItemView {
    background-color: #313244;
    color: #cdd6f4;
    selection-background-color: #89b4fa;
    selection-color: #1e1e2e;
    border: 1px solid #45475a;
}
QTableView {
    background-color: #181825;
    alternate-background-color: #1e1e2e;
    color: #cdd6f4;
    gridline-color: #313244;
    border: 1px solid #313244;
    border-radius: 4px;
    selection-background-color: #45475a;
    selection-color: #cdd6f4;
}
QTableView::item:selected { background-color: #313244; color: #89b4fa; }
QHeaderView::section {
    background-color: #181825;
    color: #a6adc8;
    border: none;
    border-right: 1px solid #313244;
    border-bottom: 1px solid #45475a;
    padding: 5px 8px;
    font-weight: bold;
    font-size: 12px;
}
QScrollBar:vertical {
    background: #181825;
    width: 10px;
    border-radius: 5px;
}
QScrollBar::handle:vertical {
    background: #45475a;
    border-radius: 5px;
    min-height: 30px;
}
QScrollBar::handle:vertical:hover { background: #585b70; }
QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical { height: 0; }
QScrollBar:horizontal {
    background: #181825;
    height: 10px;
    border-radius: 5px;
}
QScrollBar::handle:horizontal {
    background: #45475a;
    border-radius: 5px;
    min-width: 30px;
}
QScrollBar::handle:horizontal:hover { background: #585b70; }
QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal { width: 0; }
QSplitter::handle { background: #313244; width: 3px; height: 3px; }
QTabWidget::pane { border: 1px solid #313244; border-radius: 4px; }
QTabBar::tab {
    background-color: #181825;
    color: #a6adc8;
    border: 1px solid #313244;
    border-bottom: none;
    border-top-left-radius: 4px;
    border-top-right-radius: 4px;
    padding: 6px 14px;
    margin-right: 2px;
}
QTabBar::tab:selected { background-color: #313244; color: #cdd6f4; border-bottom: 1px solid #313244; }
QTabBar::tab:hover    { background-color: #45475a; color: #cdd6f4; }
QGroupBox {
    border: 1px solid #313244;
    border-radius: 6px;
    margin-top: 14px;
    padding-top: 6px;
    font-weight: bold;
    color: #a6adc8;
}
QGroupBox::title {
    subcontrol-origin: margin;
    subcontrol-position: top left;
    padding: 0 6px;
    left: 10px;
}
QCheckBox::indicator {
    width: 16px; height: 16px;
    border: 1px solid #45475a;
    border-radius: 3px;
    background-color: #313244;
}
QCheckBox::indicator:checked {
    background-color: #89b4fa;
    border-color: #89b4fa;
    image: none;
}
QProgressBar {
    background-color: #313244;
    border: 1px solid #45475a;
    border-radius: 4px;
    text-align: center;
    color: #cdd6f4;
    height: 14px;
}
QProgressBar::chunk { background-color: #89b4fa; border-radius: 4px; }
QStatusBar { background-color: #181825; color: #a6adc8; border-top: 1px solid #313244; }
QStatusBar QLabel { color: #a6adc8; padding: 0 8px; }
QTextEdit#log {
    background-color: #11111b;
    color: #a6e3a1;
    font-family: "Cascadia Code", "Fira Code", "Courier New", monospace;
    font-size: 12px;
    border: 1px solid #313244;
    border-radius: 4px;
}
QLabel#hint { color: #6c7086; font-size: 11px; }
QLabel#section { color: #89b4fa; font-weight: bold; font-size: 12px; }
QLabel#conn_on  { color: #a6e3a1; font-weight: bold; }
QLabel#conn_off { color: #f38ba8; font-weight: bold; }
"""


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Configuration
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

CONFIG_PATH = Path.home() / ".vinyl_ripper" / "vinyl_ripper.ini"
CONFIG_PATH.parent.mkdir(parents=True, exist_ok=True)

DEFAULTS: dict[str, str] = {
    "acoustid_api_key":    "",
    "discogs_token":       "",
    "export_format":       "flac",
    "export_dir":          str(Path.home() / "Music" / "Vinyl"),
    "silence_threshold_db": "-40",
    "silence_min_duration": "1.5",
    "default_artist":      "",
    "default_album":       "",
    "default_album_artist": "",
    "default_genre":       "",
    "default_year":        "",
    "mb_user_agent":       "VinylRipper/1.0 ( vinyl-ripper@example.com )",
}


def load_config() -> configparser.ConfigParser:
    cfg = configparser.ConfigParser()
    cfg["DEFAULT"] = DEFAULTS.copy()
    if CONFIG_PATH.exists():
        cfg.read(CONFIG_PATH)
    if "vinyl_ripper" not in cfg:
        cfg["vinyl_ripper"] = {}
    return cfg


def save_config(cfg: configparser.ConfigParser) -> None:
    with open(CONFIG_PATH, "w") as fh:
        cfg.write(fh)


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Audacity pipe
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

if sys.platform == "win32":
    _TOFILE   = "\\\\.\\pipe\\ToSrvPipe"
    _FROMFILE = "\\\\.\\pipe\\FromSrvPipe"
    _EOL      = "\r\n\0"
else:
    _uid      = os.getuid()
    _TOFILE   = f"/tmp/audacity_script_pipe.to.{_uid}"
    _FROMFILE = f"/tmp/audacity_script_pipe.from.{_uid}"
    _EOL      = "\n"


class AudacityPipe:
    def __init__(self) -> None:
        self._to = self._from_ = None
        self.connected = False

    def connect(self) -> bool:
        try:
            self._to    = open(_TOFILE,   "w")
            self._from_ = open(_FROMFILE, "r")
            self.connected = True
            return True
        except (FileNotFoundError, PermissionError) as exc:
            print(f"[Pipe] {exc}")
            return False

    def send(self, cmd: str) -> str:
        if not self.connected:
            return ""
        self._to.write(cmd + _EOL)
        self._to.flush()
        lines: list[str] = []
        while True:
            line = self._from_.readline().rstrip("\n\r")
            if line in ("BatchCommand finished: OK", "BatchCommand finished: Failed"):
                break
            lines.append(line)
        return "\n".join(lines)

    def close(self) -> None:
        for f in (self._to, self._from_):
            if f:
                f.close()
        self.connected = False

    # helpers
    def add_silence_labels(self, threshold_db: float, min_dur: float) -> None:
        self.send(f"SilenceFind: Threshold={threshold_db:.1f} Minimum={min_dur:.2f}")

    def get_labels(self) -> list:
        raw = self.send("GetInfo: Type=Labels Format=JSON")
        try:
            return json.loads(raw)
        except Exception:
            return []

    def select_audio(self, start: float, end: float, track: int = 0) -> None:
        self.send(f"SelectTime: Start={start:.3f} End={end:.3f} RelativeTo=ProjectStart")
        self.send(f"SelectTracks: Track={track} TrackCount=1 Mode=Set")

    def export_selection(self, filepath: str, fmt: str = "FLAC") -> None:
        escaped = filepath.replace("\\", "/")
        self.send(f"Export2: Filename='{escaped}' NumChannels=2")


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Track data model
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

@dataclass
class TrackMeta:
    index: int
    start: float
    end: float
    title: str = ""
    artist: str = ""
    album: str = ""
    album_artist: str = ""
    genre: str = ""
    track_number: str = ""
    year: str = ""
    acoustid: str = ""
    mb_recording_id: str = ""
    discogs_release_id: str = ""
    fingerprint_done: bool = False
    export_path: str = ""

    @property
    def duration(self) -> float:
        return self.end - self.start

    @property
    def display_time(self) -> str:
        def _f(s: float) -> str:
            m, sec = divmod(int(s), 60)
            return f"{m}:{sec:02d}"
        return f"{_f(self.start)}–{_f(self.end)}"

    @property
    def status_icon(self) -> str:
        if self.export_path and Path(self.export_path).exists():
            return "✓"
        if self.fingerprint_done:
            return "🔍"
        return ""


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Qt table model
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

_COLS = ["", "#", "Time", "Title", "Artist", "Album", "Genre", "Year"]
_COL_W = [24, 40, 100, 200, 160, 160, 100, 60]

_STATUS_COL      = 0
_NUM_COL         = 1
_TIME_COL        = 2
_TITLE_COL       = 3
_ARTIST_COL      = 4
_ALBUM_COL       = 5
_GENRE_COL       = 6
_YEAR_COL        = 7


class TrackTableModel(QAbstractTableModel):
    def __init__(self, tracks: list[TrackMeta], parent=None):
        super().__init__(parent)
        self._tracks = tracks

    # ── read ──────────────────────────────────────────────────────────────
    def rowCount(self, parent=QModelIndex()) -> int:
        return len(self._tracks)

    def columnCount(self, parent=QModelIndex()) -> int:
        return len(_COLS)

    def headerData(self, section, orientation, role=Qt.ItemDataRole.DisplayRole):
        if orientation == Qt.Orientation.Horizontal and role == Qt.ItemDataRole.DisplayRole:
            return _COLS[section]
        return None

    def data(self, index: QModelIndex, role=Qt.ItemDataRole.DisplayRole):
        if not index.isValid():
            return None
        t = self._tracks[index.row()]
        col = index.column()

        if role == Qt.ItemDataRole.DisplayRole:
            return {
                _STATUS_COL: t.status_icon,
                _NUM_COL:    t.track_number,
                _TIME_COL:   t.display_time,
                _TITLE_COL:  t.title,
                _ARTIST_COL: t.artist,
                _ALBUM_COL:  t.album,
                _GENRE_COL:  t.genre,
                _YEAR_COL:   t.year,
            }.get(col, "")

        if role == Qt.ItemDataRole.ForegroundRole:
            if t.export_path and Path(t.export_path).exists():
                return QColor("#a6e3a1")   # green — exported
            if t.fingerprint_done:
                return QColor("#89b4fa")   # blue — fingerprinted
            return QColor("#cdd6f4")

        if role == Qt.ItemDataRole.TextAlignmentRole:
            if col in (_STATUS_COL, _NUM_COL, _TIME_COL, _YEAR_COL):
                return Qt.AlignmentFlag.AlignCenter
            return Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft

        return None

    # ── write ─────────────────────────────────────────────────────────────
    def flags(self, index: QModelIndex):
        base = Qt.ItemFlag.ItemIsEnabled | Qt.ItemFlag.ItemIsSelectable
        if index.column() in (_TITLE_COL, _ARTIST_COL, _ALBUM_COL, _GENRE_COL, _YEAR_COL, _NUM_COL):
            return base | Qt.ItemFlag.ItemIsEditable
        return base

    def setData(self, index: QModelIndex, value, role=Qt.ItemDataRole.EditRole) -> bool:
        if role != Qt.ItemDataRole.EditRole:
            return False
        t = self._tracks[index.row()]
        col = index.column()
        mapping = {
            _NUM_COL:    "track_number",
            _TITLE_COL:  "title",
            _ARTIST_COL: "artist",
            _ALBUM_COL:  "album",
            _GENRE_COL:  "genre",
            _YEAR_COL:   "year",
        }
        if col in mapping:
            setattr(t, mapping[col], value)
            self.dataChanged.emit(index, index)
            return True
        return False

    # ── helpers ───────────────────────────────────────────────────────────
    def track_at(self, row: int) -> TrackMeta:
        return self._tracks[row]

    def refresh_row(self, row: int) -> None:
        self.dataChanged.emit(
            self.index(row, 0), self.index(row, len(_COLS) - 1)
        )

    def refresh_all(self) -> None:
        self.layoutChanged.emit()

    def insert_track(self, t: TrackMeta) -> None:
        row = len(self._tracks)
        self.beginInsertRows(QModelIndex(), row, row)
        self._tracks.append(t)
        self.endInsertRows()

    def remove_row(self, row: int) -> None:
        self.beginRemoveRows(QModelIndex(), row, row)
        self._tracks.pop(row)
        self.endRemoveRows()

    def move_row(self, row: int, delta: int) -> int:
        target = row + delta
        if target < 0 or target >= len(self._tracks):
            return row
        self._tracks[row], self._tracks[target] = self._tracks[target], self._tracks[row]
        self.refresh_all()
        return target


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Backend helpers  (fingerprint / MusicBrainz / Discogs / tags)
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

def fingerprint_file(filepath: str, api_key: str) -> Optional[dict]:
    if not HAS_ACOUSTID or not api_key:
        return None
    try:
        for score, rid, title, artist in acoustid.match(
                api_key, filepath, parse=True, meta="recordings releases"):
            if score > 0.5:
                return {"score": score, "recording_id": rid,
                        "title": title, "artist": artist}
    except Exception as exc:
        print(f"[AcoustID] {exc}")
    return None


def mb_lookup_recording(recording_id: str) -> Optional[dict]:
    if not HAS_MB or not recording_id:
        return None
    try:
        result = mb.get_recording_by_id(
            recording_id,
            includes=["artists", "releases", "release-groups", "tags"])
        rec  = result.get("recording", {})
        meta: dict = {"title": rec.get("title", "")}

        credits = rec.get("artist-credit", [])
        meta["artist"] = " & ".join(
            c.get("artist", {}).get("name", "")
            for c in credits if isinstance(c, dict)
        )

        releases = rec.get("release-list", [])
        if releases:
            rel = releases[0]
            meta["album"] = rel.get("title", "")
            meta["year"]  = rel.get("date", "")[:4]
            for medium in rel.get("medium-list", []):
                for track in medium.get("track-list", []):
                    if track.get("recording", {}).get("id") == recording_id:
                        meta["track_number"] = track.get("number", "")

        tags = rec.get("tag-list", [])
        if tags:
            top = sorted(tags, key=lambda t: int(t.get("count", 0)), reverse=True)
            meta["genre"] = top[0].get("name", "").title()
        return meta
    except Exception as exc:
        print(f"[MusicBrainz] {exc}")
    return None


def mb_search(title: str, artist: str = "") -> Optional[dict]:
    if not HAS_MB:
        return None
    try:
        query = f'recording:"{title}"'
        if artist:
            query += f' AND artist:"{artist}"'
        result = mb.search_recordings(query=query, limit=5)
        recs = result.get("recording-list", [])
        if recs:
            return mb_lookup_recording(recs[0]["id"])
    except Exception as exc:
        print(f"[MusicBrainz search] {exc}")
    return None


def discogs_search(artist: str, album: str, token: str) -> Optional[dict]:
    if not HAS_DISCOGS or not token:
        return None
    try:
        d = discogs_client.Client("VinylRipper/1.0", user_token=token)
        results = d.search(f"{artist} {album}", type="release")
        if results:
            rel = results[0]
            return {
                "album":        rel.title.split(" - ")[-1] if " - " in rel.title else rel.title,
                "album_artist": rel.artists[0].name if rel.artists else "",
                "year":         str(rel.year) if rel.year else "",
                "genre":        rel.genres[0] if rel.genres else "",
                "release_id":   str(rel.id),
            }
    except Exception as exc:
        print(f"[Discogs] {exc}")
    return None


def apply_tags(filepath: str, meta: TrackMeta) -> None:
    if not HAS_MUTAGEN:
        return
    ext = Path(filepath).suffix.lower()
    try:
        if ext == ".flac":
            audio = FLAC(filepath)
            audio["title"]       = meta.title
            audio["artist"]      = meta.artist
            audio["album"]       = meta.album
            audio["albumartist"] = meta.album_artist
            audio["genre"]       = meta.genre
            audio["tracknumber"] = meta.track_number
            audio["date"]        = meta.year
            audio.save()
        elif ext == ".mp3":
            try:
                audio = ID3(filepath)
            except Exception:
                audio = ID3()
            audio.add(TIT2(encoding=3, text=meta.title))
            audio.add(TPE1(encoding=3, text=meta.artist))
            audio.add(TALB(encoding=3, text=meta.album))
            audio.add(TPE2(encoding=3, text=meta.album_artist))
            audio.add(TCON(encoding=3, text=meta.genre))
            audio.add(TRCK(encoding=3, text=meta.track_number))
            audio.save(filepath)
        else:
            audio = MutagenFile(filepath, easy=True)
            if audio:
                for k, v in [("title", meta.title), ("artist", meta.artist),
                              ("album", meta.album), ("genre", meta.genre),
                              ("tracknumber", meta.track_number), ("date", meta.year)]:
                    audio[k] = v
                audio.save()
    except Exception as exc:
        print(f"[Tags] {exc}")


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Worker threads  (Qt signals/slots — no shared state hacks)
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

class FingerprintWorker(QThread):
    log       = pyqtSignal(str)
    progress  = pyqtSignal(int, int)       # current, total
    track_done = pyqtSignal(int, dict)     # row index, updated meta dict
    finished_all = pyqtSignal()

    def __init__(self, tracks: list[TrackMeta], rows: list[int],
                 pipe: AudacityPipe, cfg: configparser.ConfigParser):
        super().__init__()
        self._tracks = tracks
        self._rows   = rows
        self._pipe   = pipe
        self._cfg    = cfg

    def run(self) -> None:
        sec     = self._cfg["vinyl_ripper"]
        api_key = sec.get("acoustid_api_key", "")
        fmt     = sec.get("export_format", "flac")

        for idx, row in enumerate(self._rows):
            t = self._tracks[row]
            self.progress.emit(idx, len(self._rows))
            self.log.emit(f"  Track {t.track_number}: exporting temp file…")

            tmp_path = os.path.join(
                tempfile.gettempdir(),
                f"vinyl_ripper_tmp_{t.index}.{fmt}"
            )
            if self._pipe.connected:
                self._pipe.select_audio(t.start, t.end)
                self._pipe.export_selection(tmp_path, fmt.upper())
                time.sleep(2.0)

            updates: dict = {}

            if not Path(tmp_path).exists():
                self.log.emit(f"  ✗ Track {t.track_number}: temp export not found.")
            else:
                match = fingerprint_file(tmp_path, api_key)
                if match:
                    rid = match.get("recording_id", "")
                    self.log.emit(
                        f"  ✓ Track {t.track_number}: AcoustID score={match['score']:.2f} "
                        f"→ {match.get('title','?')} / {match.get('artist','?')}"
                    )
                    updates["acoustid"] = rid
                    updates["fingerprint_done"] = True
                    if match.get("title") and not t.title:
                        updates["title"] = match["title"]
                    if match.get("artist") and not t.artist:
                        updates["artist"] = match["artist"]

                    mb_meta = mb_lookup_recording(rid)
                    if mb_meta:
                        self._merge(updates, mb_meta, t)
                        self.log.emit(
                            f"    MB: {updates.get('title','?')} / "
                            f"{updates.get('artist','?')} / {updates.get('album','?')}"
                        )
                else:
                    self.log.emit(f"  ~ Track {t.track_number}: no AcoustID match.")

                try:
                    os.remove(tmp_path)
                except Exception:
                    pass

            # Discogs
            eff_artist = updates.get("artist", t.artist)
            eff_album  = updates.get("album",  t.album)
            if eff_artist and eff_album:
                dm = discogs_search(eff_artist, eff_album, sec.get("discogs_token", ""))
                if dm:
                    if dm.get("album_artist") and not t.album_artist:
                        updates["album_artist"] = dm["album_artist"]
                    if dm.get("genre") and not (updates.get("genre") or t.genre):
                        updates["genre"] = dm["genre"]
                    if dm.get("year") and not (updates.get("year") or t.year):
                        updates["year"] = dm["year"]
                    if dm.get("release_id"):
                        updates["discogs_release_id"] = dm["release_id"]
                    self.log.emit(
                        f"    Discogs: album_artist={updates.get('album_artist','')}  "
                        f"genre={updates.get('genre','')}"
                    )

            if updates:
                self.track_done.emit(row, updates)

        self.progress.emit(len(self._rows), len(self._rows))
        self.finished_all.emit()

    @staticmethod
    def _merge(updates: dict, mb_meta: dict, t: TrackMeta) -> None:
        for k in ("title", "artist", "album", "genre", "year", "track_number"):
            if mb_meta.get(k) and not (updates.get(k) or getattr(t, k)):
                updates[k] = mb_meta[k]


class ExportWorker(QThread):
    log       = pyqtSignal(str)
    progress  = pyqtSignal(int, int)
    finished_all = pyqtSignal()

    def __init__(self, tracks: list[TrackMeta], rows: list[int],
                 pipe: AudacityPipe, cfg: configparser.ConfigParser):
        super().__init__()
        self._tracks = tracks
        self._rows   = rows
        self._pipe   = pipe
        self._cfg    = cfg

    def run(self) -> None:
        sec     = self._cfg["vinyl_ripper"]
        fmt     = sec.get("export_format", "flac")
        out_dir = Path(sec.get("export_dir", str(Path.home() / "Music")))

        for idx, row in enumerate(self._rows):
            t = self._tracks[row]
            self.progress.emit(idx, len(self._rows))

            def _safe(s: str) -> str:
                return re.sub(r'[<>:"/\\|?*]', "_", s) if s else "Unknown"

            num       = (t.track_number or "00").zfill(2)
            album_dir = out_dir / _safe(t.artist) / _safe(t.album)
            album_dir.mkdir(parents=True, exist_ok=True)
            filepath  = str(album_dir / f"{num} - {_safe(t.title)}.{fmt}")

            self.log.emit(f"Exporting Track {t.track_number}: {t.title}")
            if self._pipe.connected:
                self._pipe.select_audio(t.start, t.end)
                self._pipe.export_selection(filepath, fmt.upper())
                time.sleep(2.5)

            if Path(filepath).exists():
                apply_tags(filepath, t)
                t.export_path = filepath
                self.log.emit(f"  ✓ {filepath}")
            else:
                self.log.emit(f"  ✗ File not found after export: {filepath}")

        self.progress.emit(len(self._rows), len(self._rows))
        self.finished_all.emit()


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Detail panel
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

class DetailPanel(QWidget):
    meta_changed    = pyqtSignal(dict)    # emitted on Save
    mb_requested    = pyqtSignal(str, str)   # title, artist
    discogs_requested = pyqtSignal(str, str) # artist, album

    def __init__(self, parent=None):
        super().__init__(parent)
        self._track: Optional[TrackMeta] = None
        self._building = False
        self._build()

    def _build(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(8, 8, 8, 8)
        root.setSpacing(6)

        hdr = QLabel("Track Metadata")
        hdr.setObjectName("section")
        root.addWidget(hdr)

        form = QFormLayout()
        form.setLabelAlignment(Qt.AlignmentFlag.AlignRight)
        form.setSpacing(6)

        self._fields: dict[str, QLineEdit] = {}
        for key, label in [
            ("track_number", "Track #"),
            ("title",        "Title"),
            ("artist",       "Artist"),
            ("album",        "Album"),
            ("album_artist", "Album Artist"),
            ("genre",        "Genre"),
            ("year",         "Year"),
            ("start",        "Start (s)"),
            ("end",          "End (s)"),
        ]:
            le = QLineEdit()
            le.setPlaceholderText(label)
            self._fields[key] = le
            form.addRow(label + ":", le)

        root.addLayout(form)

        # action buttons
        btn_row = QHBoxLayout()
        self._btn_save = QPushButton("💾  Save")
        self._btn_save.setObjectName("accent")
        self._btn_mb = QPushButton("🌐  MB Lookup")
        self._btn_discogs = QPushButton("🎵  Discogs")
        for b in (self._btn_save, self._btn_mb, self._btn_discogs):
            btn_row.addWidget(b)

        self._btn_save.clicked.connect(self._emit_save)
        self._btn_mb.clicked.connect(self._emit_mb)
        self._btn_discogs.clicked.connect(self._emit_discogs)

        root.addLayout(btn_row)
        root.addStretch()

        self.set_track(None)

    def set_track(self, t: Optional[TrackMeta]) -> None:
        self._track = t
        self._building = True
        enabled = t is not None
        for le in self._fields.values():
            le.setEnabled(enabled)
        for b in (self._btn_save, self._btn_mb, self._btn_discogs):
            b.setEnabled(enabled)

        if t:
            for key, le in self._fields.items():
                le.setText(str(getattr(t, key, "")))
        else:
            for le in self._fields.values():
                le.clear()
        self._building = False

    def _emit_save(self) -> None:
        if not self._track:
            return
        data: dict = {}
        for key, le in self._fields.items():
            val = le.text().strip()
            if key in ("start", "end"):
                try:
                    data[key] = float(val)
                except ValueError:
                    pass
            else:
                data[key] = val
        self.meta_changed.emit(data)

    def _emit_mb(self) -> None:
        self.mb_requested.emit(
            self._fields["title"].text().strip(),
            self._fields["artist"].text().strip(),
        )

    def _emit_discogs(self) -> None:
        self.discogs_requested.emit(
            self._fields["artist"].text().strip(),
            self._fields["album"].text().strip(),
        )


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Settings dialog
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

class SettingsDialog(QDialog):
    def __init__(self, cfg: configparser.ConfigParser, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Settings")
        self.setMinimumWidth(520)
        self.cfg = cfg
        self.saved = False
        self._fields: dict[str, QWidget] = {}
        self._build()

    def _build(self) -> None:
        layout = QVBoxLayout(self)
        tabs = QTabWidget()
        layout.addWidget(tabs)

        sec = self.cfg["vinyl_ripper"]

        # ── API Keys ──────────────────────────────────────────────────────
        api_w = QWidget()
        api_f = QFormLayout(api_w)
        api_f.setSpacing(10)
        api_f.setContentsMargins(12, 12, 12, 12)

        for key, label, hint in [
            ("acoustid_api_key", "AcoustID API Key",
             "Free key at acoustid.org/login"),
            ("discogs_token", "Discogs Token",
             "Personal token from discogs.com/settings/developers"),
        ]:
            le = QLineEdit(sec.get(key, ""))
            le.setEchoMode(QLineEdit.EchoMode.Password)
            le.setMinimumWidth(320)
            show_btn = QPushButton("👁")
            show_btn.setFixedWidth(32)
            show_btn.setCheckable(True)
            show_btn.toggled.connect(
                lambda checked, w=le:
                w.setEchoMode(QLineEdit.EchoMode.Normal if checked
                              else QLineEdit.EchoMode.Password)
            )
            row_w = QWidget()
            row_l = QHBoxLayout(row_w)
            row_l.setContentsMargins(0, 0, 0, 0)
            row_l.addWidget(le)
            row_l.addWidget(show_btn)
            api_f.addRow(label + ":", row_w)
            hint_lbl = QLabel(hint)
            hint_lbl.setObjectName("hint")
            api_f.addRow("", hint_lbl)
            self._fields[key] = le

        tabs.addTab(api_w, "API Keys")

        # ── Export ────────────────────────────────────────────────────────
        exp_w = QWidget()
        exp_f = QFormLayout(exp_w)
        exp_f.setSpacing(10)
        exp_f.setContentsMargins(12, 12, 12, 12)

        fmt_cb = QComboBox()
        fmt_cb.addItems(["flac", "mp3", "wav", "ogg"])
        fmt_cb.setCurrentText(sec.get("export_format", "flac"))
        exp_f.addRow("Format:", fmt_cb)
        self._fields["export_format"] = fmt_cb

        dir_le  = QLineEdit(sec.get("export_dir", ""))
        dir_btn = QPushButton("…")
        dir_btn.setFixedWidth(36)
        dir_btn.clicked.connect(lambda: dir_le.setText(
            QFileDialog.getExistingDirectory(self, "Export Directory", dir_le.text())
            or dir_le.text()
        ))
        dir_row = QWidget()
        dir_l = QHBoxLayout(dir_row)
        dir_l.setContentsMargins(0, 0, 0, 0)
        dir_l.addWidget(dir_le)
        dir_l.addWidget(dir_btn)
        exp_f.addRow("Export Dir:", dir_row)
        self._fields["export_dir"] = dir_le

        for key, label in [
            ("silence_threshold_db", "Silence dB"),
            ("silence_min_duration", "Min Silence (s)"),
        ]:
            le = QLineEdit(sec.get(key, DEFAULTS.get(key, "")))
            le.setMaximumWidth(100)
            exp_f.addRow(label + ":", le)
            self._fields[key] = le

        tabs.addTab(exp_w, "Export")

        # ── Defaults ──────────────────────────────────────────────────────
        def_w = QWidget()
        def_f = QFormLayout(def_w)
        def_f.setSpacing(10)
        def_f.setContentsMargins(12, 12, 12, 12)

        for key, label in [
            ("default_artist",       "Artist"),
            ("default_album",        "Album"),
            ("default_album_artist", "Album Artist"),
            ("default_genre",        "Genre"),
            ("default_year",         "Year"),
        ]:
            le = QLineEdit(sec.get(key, ""))
            def_f.addRow(label + ":", le)
            self._fields[key] = le

        tabs.addTab(def_w, "Defaults")

        # ── buttons ───────────────────────────────────────────────────────
        btns = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Save |
            QDialogButtonBox.StandardButton.Cancel
        )
        btns.accepted.connect(self._save)
        btns.rejected.connect(self.reject)
        layout.addWidget(btns)

    def _save(self) -> None:
        sec = self.cfg["vinyl_ripper"]
        for key, widget in self._fields.items():
            if isinstance(widget, QComboBox):
                sec[key] = widget.currentText()
            else:
                sec[key] = widget.text().strip()
        self.saved = True
        self.accept()


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Export-all pre-flight dialog
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

class ExportAllDialog(QDialog):
    def __init__(self, tracks: list[TrackMeta], cfg: configparser.ConfigParser, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Export All Tracks — Review & Confirm")
        self.setMinimumSize(960, 520)
        self._tracks = tracks
        self._cfg    = cfg
        self.confirmed   = False
        self.save_defaults = False
        self.updated_fields: list[dict] = []
        self._build()

    def _build(self) -> None:
        layout = QVBoxLayout(self)

        hdr = QLabel("Review and edit all track metadata before export. "
                     "Inline edits are applied immediately to the table.")
        hdr.setObjectName("hint")
        layout.addWidget(hdr)

        # build a model just for this dialog
        sec = self._cfg["vinyl_ripper"]
        for t in self._tracks:
            t.artist       = t.artist       or sec.get("default_artist", "")
            t.album        = t.album        or sec.get("default_album", "")
            t.album_artist = t.album_artist or sec.get("default_album_artist", "")
            t.genre        = t.genre        or sec.get("default_genre", "")
            t.year         = t.year         or sec.get("default_year", "")
            if not t.track_number:
                t.track_number = str(t.index)

        self._model = TrackTableModel(self._tracks)
        view = QTableView()
        view.setModel(self._model)
        view.setAlternatingRowColors(True)
        view.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        view.horizontalHeader().setSectionResizeMode(QHeaderView.ResizeMode.Interactive)
        view.horizontalHeader().setStretchLastSection(True)
        for col, w in enumerate(_COL_W):
            view.setColumnWidth(col, w)
        view.verticalHeader().setDefaultSectionSize(28)
        layout.addWidget(view)

        # save-defaults checkbox
        self._save_chk = QCheckBox("Save album / artist / genre / year as session defaults")
        layout.addWidget(self._save_chk)

        # buttons
        btns = QDialogButtonBox()
        export_btn = btns.addButton("Export", QDialogButtonBox.ButtonRole.AcceptRole)
        export_btn.setObjectName("accent")
        btns.addButton(QDialogButtonBox.StandardButton.Cancel)
        btns.accepted.connect(self._confirm)
        btns.rejected.connect(self.reject)
        layout.addWidget(btns)

    def _confirm(self) -> None:
        self.updated_fields = [
            {
                "track_number": t.track_number,
                "title":        t.title,
                "artist":       t.artist,
                "album":        t.album,
                "album_artist": t.album_artist,
                "genre":        t.genre,
                "year":         t.year,
            }
            for t in self._tracks
        ]
        self.save_defaults = self._save_chk.isChecked()
        self.confirmed = True
        self.accept()


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Manual-add track dialog
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

class ManualTrackDialog(QDialog):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Add Track Manually")
        self.setFixedWidth(400)
        self.result_track: Optional[TrackMeta] = None
        self._fields: dict[str, QLineEdit] = {}
        self._build()

    def _build(self) -> None:
        layout = QVBoxLayout(self)
        form = QFormLayout()
        form.setSpacing(8)
        defaults = {
            "track_number": "1", "start": "0.0", "end": "180.0",
            "title": "", "artist": "", "album": "",
            "album_artist": "", "genre": "", "year": "",
        }
        labels = {
            "track_number": "Track #", "start": "Start (s)", "end": "End (s)",
            "title": "Title", "artist": "Artist", "album": "Album",
            "album_artist": "Album Artist", "genre": "Genre", "year": "Year",
        }
        for key, lbl in labels.items():
            le = QLineEdit(defaults.get(key, ""))
            form.addRow(lbl + ":", le)
            self._fields[key] = le

        layout.addLayout(form)

        btns = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok |
            QDialogButtonBox.StandardButton.Cancel
        )
        btns.accepted.connect(self._ok)
        btns.rejected.connect(self.reject)
        layout.addWidget(btns)

    def _ok(self) -> None:
        try:
            idx = int(self._fields["track_number"].text())
            s   = float(self._fields["start"].text())
            e   = float(self._fields["end"].text())
        except ValueError:
            QMessageBox.warning(self, "Validation Error",
                                "Track #, Start and End must be numbers.")
            return
        self.result_track = TrackMeta(
            index=idx, start=s, end=e,
            track_number=str(idx),
            title=        self._fields["title"].text(),
            artist=       self._fields["artist"].text(),
            album=        self._fields["album"].text(),
            album_artist= self._fields["album_artist"].text(),
            genre=        self._fields["genre"].text(),
            year=         self._fields["year"].text(),
        )
        self.accept()


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Main window
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

class MainWindow(QMainWindow):
    def __init__(self) -> None:
        super().__init__()
        self.setWindowTitle("Vinyl Ripper Helper")
        self.setMinimumSize(1100, 680)

        self.cfg    = load_config()
        self.pipe   = AudacityPipe()
        self.tracks: list[TrackMeta] = []
        self._model: Optional[TrackTableModel] = None
        self._worker: Optional[QThread] = None

        if HAS_MB:
            mb.set_useragent("VinylRipper", "1.0",
                             "https://github.com/vinyl-ripper")

        self._build_toolbar()
        self._build_central()
        self._build_statusbar()
        self._check_deps()

    # ── toolbar ───────────────────────────────────────────────────────────
    def _build_toolbar(self) -> None:
        tb = QToolBar("Main")
        tb.setMovable(False)
        tb.setToolButtonStyle(Qt.ToolButtonStyle.ToolButtonTextUnderIcon)
        self.addToolBar(tb)

        def _act(label: str, tip: str, slot) -> QAction:
            a = QAction(label, self)
            a.setToolTip(tip)
            a.triggered.connect(slot)
            tb.addAction(a)
            return a

        _act("⚙\nSettings",       "Configure API keys, export settings",       self._open_settings)
        tb.addSeparator()
        self._act_connect = _act("🔌\nConnect",   "Connect to Audacity pipe",   self._connect)
        tb.addSeparator()
        _act("🔇\nDetect Silence", "Run silence detection in Audacity",         self._detect_silence)
        _act("📥\nImport Labels",  "Import label markers from Audacity",        self._import_labels)
        _act("✏\nAdd Track",       "Add a track region manually",               self._manual_add_track)
        tb.addSeparator()
        _act("🔍\nFingerprint All","Fingerprint all tracks via AcoustID",       self._fingerprint_all)
        _act("💾\nExport All",     "Export and tag all tracks",                 self._export_all)
        tb.addSeparator()

        spacer = QWidget()
        spacer.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)
        tb.addWidget(spacer)

        self._conn_lbl = QLabel("  ⬤  Disconnected  ")
        self._conn_lbl.setObjectName("conn_off")
        tb.addWidget(self._conn_lbl)

    # ── central widget ────────────────────────────────────────────────────
    def _build_central(self) -> None:
        central = QWidget()
        self.setCentralWidget(central)
        root = QVBoxLayout(central)
        root.setContentsMargins(6, 6, 6, 6)
        root.setSpacing(4)

        splitter = QSplitter(Qt.Orientation.Horizontal)
        root.addWidget(splitter, stretch=1)

        # ── left: track list ──────────────────────────────────────────────
        left = QWidget()
        left_l = QVBoxLayout(left)
        left_l.setContentsMargins(0, 0, 0, 0)
        left_l.setSpacing(4)

        hdr = QLabel("Detected Tracks")
        hdr.setObjectName("section")
        left_l.addWidget(hdr)

        self._model = TrackTableModel(self.tracks)
        self._table = QTableView()
        self._table.setModel(self._model)
        self._table.setAlternatingRowColors(True)
        self._table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self._table.setSelectionMode(QAbstractItemView.SelectionMode.SingleSelection)
        self._table.setEditTriggers(QAbstractItemView.EditTrigger.DoubleClicked |
                                    QAbstractItemView.EditTrigger.SelectedClicked)
        self._table.horizontalHeader().setSectionResizeMode(QHeaderView.ResizeMode.Interactive)
        self._table.horizontalHeader().setStretchLastSection(True)
        for col, w in enumerate(_COL_W):
            self._table.setColumnWidth(col, w)
        self._table.verticalHeader().setVisible(False)
        self._table.verticalHeader().setDefaultSectionSize(26)
        self._table.selectionModel().currentRowChanged.connect(self._on_row_changed)
        left_l.addWidget(self._table)

        # row action buttons
        btn_row = QHBoxLayout()
        for label, tip, slot in [
            ("▲",           "Move track up",        self._move_up),
            ("▼",           "Move track down",       self._move_down),
            ("✕ Remove",    "Remove selected track", self._remove_track),
            ("🔍 Fingerprint","Fingerprint selection",self._fingerprint_selected),
            ("💾 Export",   "Export selection",      self._export_selected),
        ]:
            b = QPushButton(label)
            b.setToolTip(tip)
            b.clicked.connect(slot)
            if label == "✕ Remove":
                b.setObjectName("danger")
            btn_row.addWidget(b)
        btn_row.addStretch()
        left_l.addLayout(btn_row)

        splitter.addWidget(left)

        # ── right: detail panel ───────────────────────────────────────────
        self._detail = DetailPanel()
        self._detail.meta_changed.connect(self._apply_detail_changes)
        self._detail.mb_requested.connect(self._mb_lookup_selected)
        self._detail.discogs_requested.connect(self._discogs_lookup_selected)
        splitter.addWidget(self._detail)
        splitter.setSizes([700, 320])

        # ── log ───────────────────────────────────────────────────────────
        log_box = QGroupBox("Log")
        log_l = QVBoxLayout(log_box)
        log_l.setContentsMargins(4, 4, 4, 4)
        self._log_view = QTextEdit()
        self._log_view.setObjectName("log")
        self._log_view.setReadOnly(True)
        self._log_view.setMaximumHeight(140)
        log_l.addWidget(self._log_view)
        root.addWidget(log_box)

    # ── status bar ────────────────────────────────────────────────────────
    def _build_statusbar(self) -> None:
        sb = QStatusBar()
        self.setStatusBar(sb)
        self._status_lbl  = QLabel("Ready")
        self._progress    = QProgressBar()
        self._progress.setVisible(False)
        self._progress.setMaximumWidth(240)
        sb.addWidget(self._status_lbl, 1)
        sb.addPermanentWidget(self._progress)

    # ── logging ───────────────────────────────────────────────────────────
    def _log(self, msg: str) -> None:
        self._log_view.append(msg)
        self._log_view.ensureCursorVisible()
        self._status_lbl.setText(msg.strip()[:100])

    # ── dependency check ──────────────────────────────────────────────────
    def _check_deps(self) -> None:
        missing = []
        if not HAS_ACOUSTID:  missing.append("pyacoustid")
        if not HAS_MB:        missing.append("musicbrainzngs")
        if not HAS_DISCOGS:   missing.append("discogs-client")
        if not HAS_MUTAGEN:   missing.append("mutagen")
        if missing:
            self._log(f"⚠  Missing optional packages: {', '.join(missing)}")
            self._log(f"   Install: pip install {' '.join(missing)}")

    # ── connection ────────────────────────────────────────────────────────
    def _connect(self) -> None:
        if self.pipe.connect():
            self._conn_lbl.setText("  ⬤  Connected  ")
            self._conn_lbl.setObjectName("conn_on")
            self._conn_lbl.setStyle(self._conn_lbl.style())  # force re-polish
            self._log("✓ Connected to Audacity via mod-script-pipe.")
        else:
            QMessageBox.critical(
                self, "Connection Failed",
                "Cannot connect to Audacity.\n\n"
                "Ensure Audacity is running and mod-script-pipe is enabled:\n"
                "  Edit → Preferences → Modules → mod-script-pipe → Enabled\n"
                "Then restart Audacity."
            )

    # ── silence detection ─────────────────────────────────────────────────
    def _detect_silence(self) -> None:
        if not self.pipe.connected:
            QMessageBox.warning(self, "Not Connected", "Connect to Audacity first.")
            return
        sec = self.cfg["vinyl_ripper"]
        thresh  = float(sec.get("silence_threshold_db", "-40"))
        min_dur = float(sec.get("silence_min_duration", "1.5"))
        self._log(f"Running silence detection (threshold={thresh} dB, min={min_dur}s)…")
        self.pipe.add_silence_labels(thresh, min_dur)
        time.sleep(1.0)
        self._import_labels()

    # ── label import ──────────────────────────────────────────────────────
    def _import_labels(self) -> None:
        if not self.pipe.connected:
            QMessageBox.warning(self, "Not Connected", "Connect to Audacity first.")
            return
        raw = self.pipe.send("GetInfo: Type=Labels Format=JSON")
        try:
            label_data = json.loads(raw)
        except Exception:
            self._log("No label data returned from Audacity.")
            return

        all_labels: list[tuple[float, float, str]] = []
        for entry in label_data:
            if isinstance(entry, (list, tuple)) and len(entry) >= 2:
                for lbl in entry[1]:
                    all_labels.append((float(lbl[0]), float(lbl[1]), str(lbl[2])))
        all_labels.sort(key=lambda x: x[0])

        if not all_labels:
            self._log("No labels found. Run silence detection first.")
            return

        sec = self.cfg["vinyl_ripper"]
        def_artist = sec.get("default_artist", "")
        def_album  = sec.get("default_album", "")
        def_aa     = sec.get("default_album_artist", "")
        def_genre  = sec.get("default_genre", "")

        gap_labels = [l for l in all_labels
                      if re.search(r'silen', l[2], re.I) or not l[2].strip()]
        content_labels = [l for l in all_labels if l not in gap_labels]

        new_tracks: list[TrackMeta] = []

        if gap_labels:
            # derive content windows between silences
            dur_raw = self.pipe.send("GetInfo: Type=Clips Format=JSON")
            project_end = 0.0
            try:
                clips = json.loads(dur_raw)
                for c in clips:
                    project_end = max(project_end, float(c.get("end", 0)))
            except Exception:
                pass
            if project_end == 0.0:
                project_end = gap_labels[-1][1] + 30.0

            intervals: list[tuple[float, float]] = []
            cursor = 0.0
            for g_start, g_end, _ in sorted(gap_labels, key=lambda x: x[0]):
                if g_start > cursor + 0.5:
                    intervals.append((cursor, g_start))
                cursor = g_end
            if cursor < project_end - 0.5:
                intervals.append((cursor, project_end))

            for i, (s, e) in enumerate(intervals):
                new_tracks.append(TrackMeta(
                    index=i + 1, start=s, end=e,
                    track_number=str(i + 1),
                    artist=def_artist, album=def_album,
                    album_artist=def_aa, genre=def_genre,
                ))
        else:
            for i, (s, e, txt) in enumerate(content_labels):
                new_tracks.append(TrackMeta(
                    index=i + 1, start=s, end=e,
                    title=txt or f"Track {i+1}",
                    track_number=str(i + 1),
                    artist=def_artist, album=def_album,
                    album_artist=def_aa, genre=def_genre,
                ))

        if not new_tracks:
            self._log("No tracks derived from labels.")
            return

        self.tracks.clear()
        self.tracks.extend(new_tracks)
        self._model.refresh_all()
        self._log(f"Imported {len(new_tracks)} track(s) from Audacity labels.")

    # ── track list actions ────────────────────────────────────────────────
    def _current_row(self) -> int:
        return self._table.currentIndex().row()

    def _on_row_changed(self, current: QModelIndex, _prev: QModelIndex) -> None:
        row = current.row()
        if 0 <= row < len(self.tracks):
            self._detail.set_track(self.tracks[row])
        else:
            self._detail.set_track(None)

    def _apply_detail_changes(self, data: dict) -> None:
        row = self._current_row()
        if row < 0 or row >= len(self.tracks):
            return
        t = self.tracks[row]
        for k, v in data.items():
            setattr(t, k, v)
        self._model.refresh_row(row)

    def _move_up(self) -> None:
        row = self._current_row()
        if row <= 0:
            return
        new_row = self._model.move_row(row, -1)
        self._table.selectRow(new_row)

    def _move_down(self) -> None:
        row = self._current_row()
        if row < 0 or row >= len(self.tracks) - 1:
            return
        new_row = self._model.move_row(row, 1)
        self._table.selectRow(new_row)

    def _remove_track(self) -> None:
        row = self._current_row()
        if row < 0:
            return
        if QMessageBox.question(
            self, "Remove Track",
            f"Remove Track {self.tracks[row].track_number}?",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
        ) == QMessageBox.StandardButton.Yes:
            self._model.remove_row(row)
            self._detail.set_track(None)

    def _manual_add_track(self) -> None:
        dlg = ManualTrackDialog(self)
        if dlg.exec() == QDialog.DialogCode.Accepted and dlg.result_track:
            self._model.insert_track(dlg.result_track)
            self._log(f"Added track {dlg.result_track.track_number} manually.")

    # ── fingerprinting ────────────────────────────────────────────────────
    def _fingerprint_selected(self) -> None:
        row = self._current_row()
        if row < 0:
            QMessageBox.information(self, "No Selection", "Select a track first.")
            return
        self._run_fingerprint([row])

    def _fingerprint_all(self) -> None:
        if not self.tracks:
            QMessageBox.information(self, "No Tracks", "No tracks to fingerprint.")
            return
        self._run_fingerprint(list(range(len(self.tracks))))

    def _run_fingerprint(self, rows: list[int]) -> None:
        if self._worker and self._worker.isRunning():
            QMessageBox.warning(self, "Busy", "A background task is already running.")
            return
        worker = FingerprintWorker(self.tracks, rows, self.pipe, self.cfg)
        worker.log.connect(self._log)
        worker.progress.connect(self._on_progress)
        worker.track_done.connect(self._on_fp_track_done)
        worker.finished_all.connect(self._on_worker_done)
        self._worker = worker
        self._progress.setVisible(True)
        self._progress.setMaximum(len(rows))
        self._log(f"Fingerprinting {len(rows)} track(s)…")
        worker.start()

    def _on_fp_track_done(self, row: int, updates: dict) -> None:
        t = self.tracks[row]
        for k, v in updates.items():
            setattr(t, k, v)
        self._model.refresh_row(row)
        # refresh detail panel if this row is selected
        if self._current_row() == row:
            self._detail.set_track(t)

    # ── export ────────────────────────────────────────────────────────────
    def _export_selected(self) -> None:
        row = self._current_row()
        if row < 0:
            QMessageBox.information(self, "No Selection", "Select a track first.")
            return
        self._run_export([row])

    def _export_all(self) -> None:
        if not self.tracks:
            QMessageBox.information(self, "No Tracks", "No tracks to export.")
            return
        dlg = ExportAllDialog(self.tracks, self.cfg, self)
        if dlg.exec() != QDialog.DialogCode.Accepted or not dlg.confirmed:
            return
        for t, fields in zip(self.tracks, dlg.updated_fields):
            for k, v in fields.items():
                setattr(t, k, v)
        if dlg.save_defaults and self.tracks:
            ref = self.tracks[0]
            sec = self.cfg["vinyl_ripper"]
            sec["default_artist"]       = ref.artist
            sec["default_album"]        = ref.album
            sec["default_album_artist"] = ref.album_artist
            sec["default_genre"]        = ref.genre
            save_config(self.cfg)
        self._model.refresh_all()
        self._run_export(list(range(len(self.tracks))))

    def _run_export(self, rows: list[int]) -> None:
        if self._worker and self._worker.isRunning():
            QMessageBox.warning(self, "Busy", "A background task is already running.")
            return
        worker = ExportWorker(self.tracks, rows, self.pipe, self.cfg)
        worker.log.connect(self._log)
        worker.progress.connect(self._on_progress)
        worker.finished_all.connect(self._on_worker_done)
        self._worker = worker
        self._progress.setVisible(True)
        self._progress.setMaximum(len(rows))
        self._log(f"Exporting {len(rows)} track(s)…")
        worker.start()

    # ── worker helpers ────────────────────────────────────────────────────
    def _on_progress(self, done: int, total: int) -> None:
        self._progress.setValue(done)

    def _on_worker_done(self) -> None:
        self._progress.setVisible(False)
        self._model.refresh_all()
        self._log("Done.")

    # ── manual MB / Discogs lookup ────────────────────────────────────────
    def _mb_lookup_selected(self, title: str, artist: str) -> None:
        self._log(f"MusicBrainz search: '{title}' by '{artist}'…")
        meta = mb_search(title, artist)
        row = self._current_row()
        if meta and row >= 0:
            t = self.tracks[row]
            FingerprintWorker._merge(meta, meta, t)  # reuse helper
            for k, v in meta.items():
                if v:
                    setattr(t, k, v)
            self._model.refresh_row(row)
            self._detail.set_track(t)
            self._log(f"  MB: {t.title} / {t.artist} / {t.album}")
        else:
            self._log("  No MusicBrainz result found.")

    def _discogs_lookup_selected(self, artist: str, album: str) -> None:
        token = self.cfg["vinyl_ripper"].get("discogs_token", "")
        if not token:
            QMessageBox.warning(self, "Discogs",
                                "Add your Discogs personal token in Settings → API Keys.")
            return
        self._log(f"Discogs search: '{artist}' / '{album}'…")
        meta = discogs_search(artist, album, token)
        row = self._current_row()
        if meta and row >= 0:
            t = self.tracks[row]
            for k, v in [("album_artist", meta.get("album_artist","")),
                         ("genre",        meta.get("genre","")),
                         ("year",         meta.get("year",""))]:
                if v and not getattr(t, k):
                    setattr(t, k, v)
            if meta.get("release_id"):
                t.discogs_release_id = meta["release_id"]
            self._model.refresh_row(row)
            self._detail.set_track(t)
            self._log(f"  Discogs: album_artist={t.album_artist} genre={t.genre}")
        else:
            self._log("  No Discogs result found.")

    # ── settings ──────────────────────────────────────────────────────────
    def _open_settings(self) -> None:
        dlg = SettingsDialog(self.cfg, self)
        dlg.exec()
        if dlg.saved:
            save_config(self.cfg)
            self._log("Settings saved.")
            if HAS_MB:
                agent = self.cfg["vinyl_ripper"].get("mb_user_agent",
                                                     DEFAULTS["mb_user_agent"])
                parts = agent.split("/")
                mb.set_useragent(parts[0],
                                 parts[1].split()[0] if len(parts) > 1 else "1.0")


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Entry point
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

def main() -> None:
    from vripr import __version__
    app = QApplication(sys.argv)
    app.setApplicationName("vripr")
    app.setApplicationVersion(__version__)
    app.setStyleSheet(DARK_QSS)
    win = MainWindow()
    win.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
