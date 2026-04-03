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
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

# ── PyQt6 ─────────────────────────────────────────────────────────────────
from PyQt6.QtCore import (
    Qt, QAbstractTableModel, QModelIndex, QThread, pyqtSignal, QObject,
    QSortFilterProxyModel, QItemSelectionModel, QTimer,
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
    "audio_file":          "",    # explicit path to WAV/FLAC for Python scanner
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



# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Audacity window focus helper  (module-level so all workers can share it)
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

def _raise_audacity_window() -> None:
    """Best-effort: bring the Audacity project window to the foreground.

    Audacity's Analyze-menu effects (SilenceFind) and project-modifying
    commands (SelectTime, SelectTracks, Export2) require the project window
    to have OS focus when the scripting command is processed.

    Tries three OS-level approaches in order; all failures are silent.
    The Audacity native ``Raise:`` pipe command is handled separately by
    AudacityPipe.raise_focus() before this function is called.
    """
    if sys.platform == "darwin":
        try:
            subprocess.run(
                ["osascript", "-e", 'tell application "Audacity" to activate'],
                timeout=3, capture_output=True
            )
        except Exception:
            pass

    elif sys.platform.startswith("linux"):
        # wmctrl is the most reliable on X11/Wayland-XWayland
        try:
            subprocess.run(["wmctrl", "-a", "Audacity"],
                           timeout=3, capture_output=True)
            return
        except Exception:
            pass
        # Fallback: xdotool
        try:
            r = subprocess.run(
                ["xdotool", "search", "--name", "Audacity"],
                timeout=3, capture_output=True, text=True
            )
            wids = r.stdout.strip().splitlines()
            if wids:
                subprocess.run(
                    ["xdotool", "windowactivate", "--sync", wids[0]],
                    timeout=3, capture_output=True
                )
        except Exception:
            pass

    elif sys.platform == "win32":
        try:
            import ctypes
            user32 = ctypes.windll.user32
            found: list[int] = []
            EnumWindowsProc = ctypes.WINFUNCTYPE(
                ctypes.c_bool,
                ctypes.POINTER(ctypes.c_int),
                ctypes.POINTER(ctypes.c_int),
            )
            def _cb(hwnd, _):
                n = user32.GetWindowTextLengthW(hwnd)
                if n:
                    buf = ctypes.create_unicode_buffer(n + 1)
                    user32.GetWindowTextW(hwnd, buf, n + 1)
                    if "Audacity" in buf.value:
                        found.append(hwnd)
                return True
            user32.EnumWindows(EnumWindowsProc(_cb), 0)
            if found:
                user32.SetForegroundWindow(found[0])
        except Exception:
            pass


class AudacityPipe:
    """Manages the mod-script-pipe connection to Audacity.

    Pipe lifecycle rules
    --------------------
    * Audacity **creates** the pipe files when it starts (if mod-script-pipe
      is enabled).  We must never create or delete them — only open/close them.
    * On POSIX the pipes are FIFOs.  Opening a FIFO for *writing* blocks until
      a reader is on the other end, and opening for *reading* blocks until a
      writer is present.  Audacity opens both ends when it starts; once it is
      running both opens complete instantly.
    * If vripr crashes or is killed while the pipes are open, the FIFO inodes
      remain (they are owned by Audacity).  Audacity will recreate / reuse them
      on its next startup.  We must NOT delete them — that would cause exactly
      the startup crash you observed.
    * On Windows the pipes are named pipes created by Audacity; we just open
      them as clients.
    """

    def __init__(self) -> None:
        self._to      = None
        self._from_   = None
        self.connected = False

    # ── pre-connect checks ────────────────────────────────────────────────
    @staticmethod
    def check_pipes() -> tuple[bool, str]:
        """Return (pipes_exist, message).

        Call this *before* connect() to give the user an early, clear error
        rather than a confusing FileNotFoundError.
        """
        if sys.platform == "win32":
            # Windows named pipes only exist while Audacity is running.
            import ctypes
            GENERIC_READ  = 0x80000000
            OPEN_EXISTING = 3
            h = ctypes.windll.kernel32.CreateFileW(
                _FROMFILE, GENERIC_READ, 0, None, OPEN_EXISTING, 0, None
            )
            INVALID = ctypes.c_void_p(-1).value
            if h == INVALID:
                return False, (
                    "Audacity named pipes not found.\n"
                    "Make sure Audacity is running and mod-script-pipe is Enabled."
                )
            ctypes.windll.kernel32.CloseHandle(h)
            return True, "OK"
        else:
            to_ok   = Path(_TOFILE).exists()
            from_ok = Path(_FROMFILE).exists()
            if not to_ok and not from_ok:
                return False, (
                    f"Pipe files not found:\n  {_TOFILE}\n  {_FROMFILE}\n\n"
                    "Audacity is not running, or mod-script-pipe is not Enabled.\n"
                    "Enable it at: Edit → Preferences → Modules → mod-script-pipe"
                    " → Enabled, then restart Audacity."
                )
            if not to_ok or not from_ok:
                missing = _TOFILE if not to_ok else _FROMFILE
                return False, (
                    f"Only one pipe file found (missing: {missing}).\n"
                    "Restart Audacity to recreate both pipes."
                )
            # Check they are actually FIFOs, not regular stale files from a
            # different process — regular files would make open() hang forever.
            import stat as _stat
            for path in (_TOFILE, _FROMFILE):
                mode = Path(path).stat().st_mode
                if not _stat.S_ISFIFO(mode):
                    return False, (
                        f"{path} exists but is not a FIFO (it is a regular file).\n"
                        "This is a leftover from a crashed process.  Delete it and "
                        "restart Audacity:\n  rm {_TOFILE} {_FROMFILE}"
                    )
            return True, "OK"

    def connect(self) -> tuple[bool, str]:
        """Open the pipe.  Returns (success, error_message).

        We mirror pipe_test.py exactly: plain blocking open() in text mode,
        explicit utf-8 encoding on the read end.  Both opens are run in daemon
        threads with a timeout so that if Audacity hasn't opened its end yet
        (mod-script-pipe disabled, Audacity still starting) we get a clean
        error rather than hanging forever.

        All send() calls already run inside QThread workers, so plain blocking
        readline() in send() is fine — it only blocks the worker thread, not
        the Qt main / event-loop thread.
        """
        ok, msg = self.check_pipes()
        if not ok:
            return False, msg

        import queue as _q
        to_q:   _q.Queue = _q.Queue()
        from_q: _q.Queue = _q.Queue()
        timeout_s = 5.0

        def _open_to():
            try:
                # Text mode, line-buffered (buffering=1) so flush() is instant
                to_q.put(open(_TOFILE, "w", buffering=1))
            except Exception as e:
                to_q.put(e)

        def _open_from():
            try:
                # Explicit utf-8, line-buffered — matches pipe_test.py
                from_q.put(open(_FROMFILE, "r", encoding="utf-8", buffering=1))
            except Exception as e:
                from_q.put(e)

        t1 = threading.Thread(target=_open_to,   daemon=True)
        t2 = threading.Thread(target=_open_from, daemon=True)
        t1.start(); t2.start()
        t1.join(timeout_s); t2.join(timeout_s)

        if t1.is_alive() or t2.is_alive():
            return False, (
                f"Opening the pipe timed out after {timeout_s:.0f} s.\n\n"
                "Audacity has not opened its end of the pipe.\n"
                "Check: Edit → Preferences → Modules → mod-script-pipe = Enabled,\n"
                "then restart Audacity and try again."
            )

        to_val   = to_q.get()
        from_val = from_q.get()
        if isinstance(to_val,   Exception): return False, f"Write pipe error: {to_val}"
        if isinstance(from_val, Exception): return False, f"Read pipe error:  {from_val}"

        self._to    = to_val
        self._from_ = from_val
        self.connected = True
        # Drain any stale data left in the pipe from previous sessions
        self.drain_stale()
        return True, "OK"

    def drain_stale(self) -> None:
        """Drain any stale data sitting in the read pipe buffer.

        Previous sessions or failed Raise: commands may have left unread
        sentinel lines / error messages in the kernel pipe buffer.  We use
        select() with zero timeout to read and discard whatever is there
        before sending the first real command.
        """
        if not self._from_ or sys.platform == "win32":
            return
        try:
            import select as _sel
            fd = self._from_.fileno()
            while True:
                ready, _, _ = _sel.select([fd], [], [], 0)
                if not ready:
                    break
                chunk = os.read(fd, 4096)
                if not chunk:
                    break
                # discard — these are stale responses we don't want
        except Exception:
            pass

    def disconnect(self) -> None:
        """Close our file handles.  Does NOT delete the pipe files — those
        belong to Audacity and must not be removed by us."""
        for f in (self._to, self._from_):
            try:
                if f:
                    f.close()
            except Exception:
                pass
        self._to = self._from_ = None
        self.connected = False

    def close(self) -> None:
        """Alias for disconnect() — kept for compatibility."""
        self.disconnect()

    def send(self, cmd: str, timeout: float = 30.0,
             needs_focus: bool = True) -> str:
        """Send *cmd* to Audacity and return the full response text.

        Mirrors pipe_test.py: write the command + newline, flush, then read
        lines until the sentinel arrives.  The blocking readline() is safe
        here because send() is ALWAYS called from a QThread worker, never
        from the Qt main/event-loop thread.

        Parameters
        ----------
        cmd          : Audacity scripting command string
        timeout      : seconds to wait for the sentinel before raising TimeoutError
        needs_focus  : if True (default), raise the Audacity window before
                       sending.  Audacity queues commands but only processes
                       them when its project window has OS focus — on this
                       system we observed ~39 s delays without focus.
                       Set False only for fire-and-forget or read-only queries
                       where stealing focus would be disruptive (e.g. heartbeat).
        """
        if not self.connected:
            return ""

        # Raise Audacity window via OS-level tools only.
        # We do NOT send "Raise:" through the pipe — it is not a valid command
        # in all Audacity versions and its failure response corrupts the pipe
        # read buffer, causing the NEXT command's response to be misread.
        if needs_focus:
            _raise_audacity_window()
            time.sleep(0.3)   # let WM complete focus switch

        # Write — identical to pipe_test.py
        self._to.write(cmd + _EOL)
        self._to.flush()

        lines: list[str] = []

        # ── Watchdog timer ─────────────────────────────────────────────────
        # Sets timed_out[0] = True after *timeout* seconds.  The read loop
        # checks this flag and raises TimeoutError so we exit cleanly even
        # though readline() is blocking.
        import queue as _q
        timed_out: list[bool] = [False]
        result_q:  _q.Queue   = _q.Queue()

        def _read_loop():
            try:
                while True:
                    if timed_out[0]:
                        result_q.put(TimeoutError(
                            f"Audacity did not respond to {cmd!r} within {timeout}s.\n"
                            "Ensure a project is open in Audacity and no dialog is blocking it."
                        ))
                        return
                    line = self._from_.readline()
                    if not line:
                        # EOF — Audacity closed the pipe (crashed / quit)
                        result_q.put(ConnectionResetError(
                            "Audacity closed the pipe (EOF). Has it crashed?"
                        ))
                        return
                    line = line.rstrip("\n\r")
                    if line in ("BatchCommand finished: OK",
                                "BatchCommand finished: Failed"):
                        result_q.put("\n".join(lines))
                        return
                    if line:
                        lines.append(line)
            except Exception as exc:
                result_q.put(exc)

        def _watchdog():
            time.sleep(timeout)
            timed_out[0] = True

        reader  = threading.Thread(target=_read_loop, daemon=True)
        watcher = threading.Thread(target=_watchdog,  daemon=True)
        reader.start()
        watcher.start()

        result = result_q.get()   # blocks until _read_loop puts something
        if isinstance(result, Exception):
            raise result
        return result

    def close(self) -> None:
        for f in (self._to, self._from_):
            if f:
                f.close()
        self.connected = False

    # helpers
    def ping(self) -> tuple[bool, str]:
        """Send a lightweight command and return (ok, response_text).

        We use a 60 s timeout because Audacity queues commands and only
        processes them when its window has focus — on some systems this means
        a query can sit for 30-40 s before being answered.  We do NOT raise
        focus here (that would steal focus every 10 s during normal use).
        """
        try:
            raw = self.send("GetInfo: Type=Tracks Format=JSON",
                            timeout=60.0, needs_focus=False)
            return True, raw
        except TimeoutError:
            return False, "timeout"
        except Exception as exc:
            return False, str(exc)

    def raise_focus(self) -> None:
        """Raise the Audacity window before sending any focus-dependent command.

        Commands that NEED focus (they run Audacity menu actions):
            SilenceFind:  SelectTime:  SelectTracks:  Export2:

        Commands that do NOT need focus (read-only queries):
            GetInfo:  Version:  Raise:

        Uses OS-level tools only (wmctrl/xdotool on Linux, osascript on macOS,
        ctypes SetForegroundWindow on Windows).  Does NOT send any pipe command
        — "Raise:" is rejected by this Audacity version and corrupts the pipe.
        """
        # OS-level raise only — no pipe commands.
        # "Raise:" is not valid in all Audacity versions; sending it writes a
        # failure response into the pipe buffer that corrupts subsequent reads.
        _raise_audacity_window()
        time.sleep(0.5)   # let WM complete the focus switch

    def get_labels(self) -> list:
        raw = self.send("GetInfo: Type=Labels Format=JSON")
        try:
            return json.loads(raw)
        except Exception:
            return []

    def select_audio(self, start: float, end: float, track: int = 0) -> None:
        """Select a time region in Audacity (focus handled by send())."""
        self.send(f"SelectTime: Start={start:.3f} End={end:.3f} RelativeTo=ProjectStart")
        self.send(f"SelectTracks: Track={track} TrackCount=1 Mode=Set")

    def export_selection(self, filepath: str, fmt: str = "FLAC") -> None:
        """Export the current selection (focus-dependent)."""
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

_COLS  = ["", "#", "Time", "Title", "Artist", "Album", "Album Artist", "Genre", "Year"]
_COL_W = [24,  40,  110,   200,     150,      150,     130,              90,      55]

_STATUS_COL      = 0
_NUM_COL         = 1
_TIME_COL        = 2
_TITLE_COL       = 3
_ARTIST_COL      = 4
_ALBUM_COL       = 5
_ALBUM_ARTIST_COL = 6
_GENRE_COL       = 7
_YEAR_COL        = 8


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
                _STATUS_COL:       t.status_icon,
                _NUM_COL:          t.track_number,
                _TIME_COL:         t.display_time,
                _TITLE_COL:        t.title,
                _ARTIST_COL:       t.artist,
                _ALBUM_COL:        t.album,
                _ALBUM_ARTIST_COL: t.album_artist,
                _GENRE_COL:        t.genre,
                _YEAR_COL:         t.year,
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

        if role == Qt.ItemDataRole.ToolTipRole:
            # Show full value in tooltip — useful for truncated cells
            return {
                _TITLE_COL:        t.title,
                _ARTIST_COL:       t.artist,
                _ALBUM_COL:        t.album,
                _ALBUM_ARTIST_COL: t.album_artist,
                _GENRE_COL:        t.genre,
            }.get(col, "")

        return None

    # ── write ─────────────────────────────────────────────────────────────
    def flags(self, index: QModelIndex):
        base = Qt.ItemFlag.ItemIsEnabled | Qt.ItemFlag.ItemIsSelectable
        if index.column() in (
            _NUM_COL, _TITLE_COL, _ARTIST_COL, _ALBUM_COL,
            _ALBUM_ARTIST_COL, _GENRE_COL, _YEAR_COL
        ):
            return base | Qt.ItemFlag.ItemIsEditable
        return base

    def setData(self, index: QModelIndex, value, role=Qt.ItemDataRole.EditRole) -> bool:
        if role != Qt.ItemDataRole.EditRole:
            return False
        t = self._tracks[index.row()]
        col = index.column()
        mapping = {
            _NUM_COL:          "track_number",
            _TITLE_COL:        "title",
            _ARTIST_COL:       "artist",
            _ALBUM_COL:        "album",
            _ALBUM_ARTIST_COL: "album_artist",
            _GENRE_COL:        "genre",
            _YEAR_COL:         "year",
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



# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Connection-verification worker  (runs once on Connect)
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

class ConnectVerifyWorker(QThread):
    """Fired once after the pipe opens.  Sends a round-trip command to confirm
    Audacity is actually responding and reports the version + track count."""
    log     = pyqtSignal(str)
    ok      = pyqtSignal(str)    # version / info string
    failed  = pyqtSignal(str)    # error message

    def __init__(self, pipe: "AudacityPipe") -> None:
        super().__init__()
        self._pipe = pipe

    def run(self) -> None:
        self.log.emit("  Verifying Audacity round-trip…")
        self.log.emit(f"  Write pipe fd : {self._pipe._to.fileno() if self._pipe._to else 'None'}")
        self.log.emit(f"  Read  pipe fd : {self._pipe._from_.fileno() if self._pipe._from_ else 'None'}")

        # ── Step 0: raise Audacity window before ANY command ──────────────
        # Audacity queues scripting commands but only processes them when its
        # project window has OS focus.  On this system we observed a ~39 s
        # delay when focus was not given — the command sat in the queue until
        # the window manager eventually switched focus.  Raising first ensures
        # the very first GetInfo is processed immediately.
        self.log.emit("  Sending GetInfo (focus handled automatically)…")

        # ── Step 1: GetInfo:Type=Tracks  (simplest always-valid command) ──
        self.log.emit("  Sending: GetInfo: Type=Tracks Format=JSON")
        try:
            raw = self._pipe.send("GetInfo: Type=Tracks Format=JSON", timeout=45.0)
            self.log.emit(f"  Raw response ({len(raw)} chars): {raw[:200]!r}")
        except TimeoutError as exc:
            self.log.emit(f"  ✗ Timeout: {exc}")
            self.failed.emit(
                "Pipe opened and both FIFOs are connected, but Audacity sent\n"
                "no response to 'GetInfo: Type=Tracks' within 45 seconds.\n\n"
                "Audacity may only process scripting commands when its project\n"
                "window has OS focus.  Try:\n"
                "  1. Click the Audacity project window to give it focus,\n"
                "     then click Connect again in vripr.\n"
                "  2. Install wmctrl for automatic focus on Linux:\n"
                "     sudo apt install wmctrl\n"
                "  3. Confirm mod-script-pipe shows 'Enabled':\n"
                "     Edit → Preferences → Modules\n"
                "  4. Make sure a project is open (File → New or open a file).\n"
                "  5. Try Audacity's own pipe test: pipe_test.py"
            )
            return
        except ConnectionResetError as exc:
            self.failed.emit(
                f"Audacity closed the pipe unexpectedly: {exc}\n"
                "This usually means Audacity crashed or mod-script-pipe\n"
                "encountered an error. Check the Audacity log."
            )
            return
        except Exception as exc:
            self.failed.emit(f"Unexpected error during GetInfo: {exc}")
            return

        # ── Step 2: parse and report ───────────────────────────────────────
        try:
            tracks = json.loads(raw)
            n_tracks = len(tracks)
            self.log.emit(f"  Parsed {n_tracks} track(s) from JSON")
        except Exception as e:
            self.log.emit(f"  JSON parse failed ({e}) — raw: {raw[:100]!r}")
            n_tracks = -1

        # ── Step 3: done — GetInfo:Tracks is sufficient ─────────────────
        # Version: is not a valid command in all Audacity builds and causes
        # a 5s timeout hang every connect.  We skip it — the track info
        # from Step 1 is all we need to confirm the pipe is working.
        track_info = (f"{n_tracks} track(s) in project"
                      if n_tracks >= 0 else "project info unavailable")
        self.log.emit(f"  ✓ Round-trip OK  [{track_info}]")
        self.ok.emit(track_info)

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Silence-detection + label-import worker
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

class SilenceWorker(QThread):
    """Detects silence in the audio file directly in Python, then derives
    track regions — no Audacity DSP or focus required.
    All pipe I/O runs in this worker thread; the Qt event loop stays free.
    """
    log          = pyqtSignal(str)
    tracks_ready = pyqtSignal(list)   # list[TrackMeta]
    error        = pyqtSignal(str)

    def __init__(
        self,
        pipe: "AudacityPipe",
        cfg: configparser.ConfigParser,
        run_detect: bool = True,      # False → skip scan, just import existing labels
    ) -> None:
        super().__init__()
        self._pipe       = pipe
        self._cfg        = cfg
        self._run_detect = run_detect

    # ── Python-native silence detection ──────────────────────────────────
    @staticmethod
    def _detect_silences_python(
        wav_path: str,
        threshold_db: float,
        min_silence_s: float,
    ) -> list[tuple[float, float]]:
        """Detect silence regions in a WAV file entirely in Python.

        Returns a list of (start_s, end_s) tuples for each silence region.
        Does not require Audacity focus.  Uses only stdlib wave + array;
        numpy/scipy are used if available for speed but are not required.
        """
        import wave as _wave
        import array as _array
        import math

        with _wave.open(wav_path, "rb") as wf:
            n_ch    = wf.getnchannels()
            sampw   = wf.getsampwidth()
            rate    = wf.getframerate()
            n_frames = wf.getnframes()

            # Read in chunks of 0.1 s to keep memory reasonable
            chunk_frames = int(rate * 0.1)
            thresh_linear = 10 ** (threshold_db / 20.0)

            silences: list[tuple[float, float]] = []
            silence_start: float | None = None
            pos = 0

            while pos < n_frames:
                to_read = min(chunk_frames, n_frames - pos)
                raw = wf.readframes(to_read)
                pos += to_read

                # Convert raw bytes to samples
                if sampw == 2:
                    samples = _array.array("h", raw)   # signed 16-bit
                    peak = 32768.0
                elif sampw == 3:
                    # 24-bit — unpack manually
                    n = len(raw) // 3
                    samples = []
                    for i in range(n):
                        b = raw[i*3:(i+1)*3]
                        val = int.from_bytes(b, "little", signed=True)
                        samples.append(val)
                    peak = 8388608.0
                elif sampw == 4:
                    samples = _array.array("i", raw)
                    peak = 2147483648.0
                else:
                    samples = _array.array("B", raw)
                    peak = 128.0

                # RMS across all channels in this chunk
                rms = math.sqrt(
                    sum(s * s for s in samples) / max(len(samples), 1)
                ) / peak

                t = (pos - to_read) / rate
                if rms < thresh_linear:
                    if silence_start is None:
                        silence_start = t
                else:
                    if silence_start is not None:
                        dur = t - silence_start
                        if dur >= min_silence_s:
                            silences.append((silence_start, t))
                        silence_start = None

            # Handle trailing silence
            if silence_start is not None:
                dur = n_frames / rate - silence_start
                if dur >= min_silence_s:
                    silences.append((silence_start, n_frames / rate))

        return silences

    def run(self) -> None:
        sec     = self._cfg["vinyl_ripper"]
        thresh  = float(sec.get("silence_threshold_db", "-40"))
        min_dur = float(sec.get("silence_min_duration", "1.5"))

        # ── step 1: get project info from Audacity ───────────────────────────
        self.log.emit("  Querying Audacity for project info…")
        audio_file  = ""
        project_end = 0.0
        try:
            raw    = self._pipe.send("GetInfo: Type=Tracks Format=JSON", timeout=30.0)
            tracks = json.loads(raw)
            for t in tracks:
                project_end = max(project_end, float(t.get("end", 0)))
                src = t.get("src", "") or t.get("file", "")
                if src and Path(src).exists() and not audio_file:
                    audio_file = src
            self.log.emit(f"  Project duration: {project_end:.1f}s")
        except Exception as exc:
            self.error.emit(f"Could not query Audacity project info: {exc}")
            return

        # ── step 2: get audio file path from Recent Files if not in tracks ──
        # Audacity 3.x does not always include the file path in GetInfo:Tracks.
        # We can recover it from the Recent Files submenu which always lists it.
        # Also check the config-stored path (set in Settings → Audio File)
        if not audio_file:
            cfg_path = sec.get("audio_file", "").strip()
            if cfg_path and Path(cfg_path).exists():
                audio_file = cfg_path
                self.log.emit(f"  Using configured audio file: {Path(cfg_path).name}")

        if not audio_file:
            self.log.emit("  Checking Recent Files for audio path…")
            try:
                raw   = self._pipe.send("GetInfo: Type=Menus Format=JSON", timeout=20.0)
                menus = json.loads(raw)
                for item in menus:
                    lbl = item.get("label", "")
                    if lbl and lbl != "----" and not lbl.startswith("/usr/lib"):
                        p = Path(lbl)
                        if p.exists() and p.suffix.lower() in (
                            ".wav", ".flac", ".aif", ".aiff", ".mp3", ".ogg", ".m4a"
                        ):
                            audio_file = str(p)
                            self.log.emit(f"  Found via Recent Files: {p.name}")
                            break
            except Exception as exc:
                self.log.emit(f"  Recent Files lookup failed: {exc}")

        # ── step 3: silence detection ─────────────────────────────────────────
        if self._run_detect:
            self.log.emit(
                f"Running silence detection (threshold={thresh} dB, min={min_dur}s)…"
            )

            # Python-native silence scanner — the only reliable approach.
            # Nyquist plugins (Label Sounds) always show a parameter dialog
            # when invoked via the pipe, so cannot run unattended.
            # The Python scanner reads the WAV file directly — no Audacity
            # focus or interaction required.
            if not audio_file:
                self.error.emit(
                    "The audio file path could not be determined automatically.\n\n"
                    "Set it in ⚙ Settings → Defaults → Audio File, or use\n"
                    "Import Labels after placing labels manually in Audacity."
                )
                return

            self.log.emit(f"  Scanning: {Path(audio_file).name}")
            try:
                silence_regions = self._detect_silences_python(
                    audio_file, thresh, min_dur
                )
            except Exception as exc:
                self.error.emit(f"Python silence detection failed: {exc}")
                return

            self.log.emit(
                f"  Found {len(silence_regions)} silence region(s) — "
                f"deriving track boundaries…"
            )
            self._derive_tracks_from_silences(silence_regions, sec, project_end)
            return

        # ── Import Labels path (run_detect=False) ────────────────────────────
        # User has placed labels manually in Audacity; just read them back.
        self.log.emit("Importing labels from Audacity…")
        try:
            raw = self._pipe.send("GetInfo: Type=Labels Format=JSON", timeout=30.0)
        except Exception as exc:
            self.error.emit(f"GetInfo (labels) failed: {exc}")
            return

        try:
            label_data = json.loads(raw)
        except Exception:
            self.error.emit("Could not parse label data from Audacity.")
            return

        all_labels: list[tuple[float, float, str]] = []
        for entry in label_data:
            if isinstance(entry, (list, tuple)) and len(entry) >= 2:
                for lbl in entry[1]:
                    all_labels.append((float(lbl[0]), float(lbl[1]), str(lbl[2])))
        all_labels.sort(key=lambda x: x[0])

        if not all_labels:
            self.error.emit(
                "No labels found in Audacity.\n"
                "Place labels manually in Audacity then click Import Labels."
            )
            return

        # Derive tracks from existing labels
        def_artist = sec.get("default_artist", "")
        def_album  = sec.get("default_album",  "")
        def_aa     = sec.get("default_album_artist", "")
        def_genre  = sec.get("default_genre",  "")

        gap_labels     = [l for l in all_labels
                          if re.search(r'silen', l[2], re.I) or not l[2].strip()]
        content_labels = [l for l in all_labels if l not in gap_labels]
        new_tracks: list[TrackMeta] = []

        if gap_labels:
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
                    title=txt or f"Track {i + 1}",
                    track_number=str(i + 1),
                    artist=def_artist, album=def_album,
                    album_artist=def_aa, genre=def_genre,
                ))

        if not new_tracks:
            self.error.emit("No track regions could be derived from the labels.")
            return

        self.log.emit(f"Found {len(new_tracks)} track region(s).")
        self.tracks_ready.emit(new_tracks)

    def _derive_tracks_from_silences(
        self,
        silence_regions: list[tuple[float, float]],
        sec,
        project_end: float = 0.0,
    ) -> None:
        """Convert silence regions directly into TrackMeta objects and emit
        tracks_ready.  project_end is the known recording duration in seconds;
        if 0 it is estimated from the last silence region."""
        def_artist = sec.get("default_artist", "")
        def_album  = sec.get("default_album",  "")
        def_aa     = sec.get("default_album_artist", "")
        def_genre  = sec.get("default_genre",  "")

        if project_end == 0.0 and silence_regions:
            project_end = silence_regions[-1][1] + 30.0

        intervals: list[tuple[float, float]] = []
        cursor = 0.0
        for g_start, g_end in sorted(silence_regions):
            if g_start > cursor + 0.5:
                intervals.append((cursor, g_start))
            cursor = g_end
        if cursor < project_end - 0.5:
            intervals.append((cursor, project_end))

        new_tracks = [
            TrackMeta(
                index=i + 1, start=s, end=e,
                track_number=str(i + 1),
                artist=def_artist, album=def_album,
                album_artist=def_aa, genre=def_genre,
            )
            for i, (s, e) in enumerate(intervals)
        ]

        if not new_tracks:
            self.error.emit("No track regions derived from silence scan.")
            return

        self.log.emit(f"  Derived {len(new_tracks)} track(s) from Python silence scan.")
        self.tracks_ready.emit(new_tracks)


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

        # Audio file path — used by Python silence scanner when Audacity
        # does not return the path via GetInfo:Tracks
        af_le  = QLineEdit(sec.get("audio_file", ""))
        af_btn = QPushButton("…")
        af_btn.setFixedWidth(36)
        af_btn.clicked.connect(lambda: af_le.setText(
            QFileDialog.getOpenFileName(
                self, "Select Audio File", af_le.text(),
                "Audio Files (*.wav *.flac *.aif *.aiff *.mp3 *.ogg)"
            )[0] or af_le.text()
        ))
        af_row = QWidget()
        af_l   = QHBoxLayout(af_row)
        af_l.setContentsMargins(0, 0, 0, 0)
        af_l.addWidget(af_le)
        af_l.addWidget(af_btn)
        def_f.addRow("Audio File:", af_row)
        self._fields["audio_file"] = af_le

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
        self._heartbeat: Optional[QTimer] = None
        self._audacity_info: str = ""          # version string after verify

        if HAS_MB:
            mb.set_useragent("VinylRipper", "1.0",
                             "https://github.com/vinyl-ripper")

        self._build_toolbar()
        self._build_central()
        self._build_statusbar()
        self._check_deps()
        self._populate_apply_strip()

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
        _act("🔍\nFingerprint All","Fingerprint all tracks via AcoustID",       self._fingerprint_all)
        _act("💾\nExport All",     "Export and tag all tracks",                 self._export_all)
        tb.addSeparator()
        _act("🩺\nDiagnostics",    "Test pipe round-trip and show Audacity info", self._run_diagnostics)
        tb.addSeparator()
        _act("🌐\nMB Lookup",      "MusicBrainz lookup for selected track",        self._mb_lookup_selected)
        _act("🎵\nDiscogs",        "Discogs lookup for selected track",             self._discogs_lookup_selected)
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

        # ── "Apply to All" strip ──────────────────────────────────────────
        # One-click population of shared fields across all tracks.
        apply_box = QGroupBox("Apply to All Tracks")
        apply_box.setMaximumHeight(100)
        apply_l = QGridLayout(apply_box)
        apply_l.setContentsMargins(8, 6, 8, 6)
        apply_l.setSpacing(4)

        self._apply_fields: dict[str, QLineEdit] = {}
        apply_defs = [
            ("artist",       "Artist",       0, 0),
            ("album",        "Album",        0, 2),
            ("album_artist", "Album Artist", 0, 4),
            ("genre",        "Genre",        1, 0),
            ("year",         "Year",         1, 2),
        ]
        for key, label, row, col in apply_defs:
            apply_l.addWidget(QLabel(label + ":"), row, col,
                              Qt.AlignmentFlag.AlignRight)
            le = QLineEdit()
            le.setPlaceholderText(f"Apply {label} to all…")
            le.setMinimumWidth(140)
            self._apply_fields[key] = le
            apply_l.addWidget(le, row, col + 1)

        apply_btn = QPushButton("⚡  Apply to All")
        apply_btn.setObjectName("accent")
        apply_btn.setToolTip("Fill all tracks with the values entered above")
        apply_btn.clicked.connect(self._apply_to_all)
        apply_l.addWidget(apply_btn, 0, 6, 2, 1,
                          Qt.AlignmentFlag.AlignVCenter)
        apply_l.setColumnStretch(1, 1)
        apply_l.setColumnStretch(3, 1)
        apply_l.setColumnStretch(5, 1)
        root.addWidget(apply_box)

        # ── track table (full width, no side panel) ───────────────────────
        hdr = QLabel("Detected Tracks")
        hdr.setObjectName("section")
        root.addWidget(hdr)

        self._model = TrackTableModel(self.tracks)
        self._table = QTableView()
        self._table.setModel(self._model)
        self._table.setAlternatingRowColors(True)
        self._table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self._table.setSelectionMode(QAbstractItemView.SelectionMode.SingleSelection)
        # Double-click or F2 to edit any metadata cell in-place
        self._table.setEditTriggers(
            QAbstractItemView.EditTrigger.DoubleClicked |
            QAbstractItemView.EditTrigger.EditKeyPressed
        )
        self._table.horizontalHeader().setSectionResizeMode(QHeaderView.ResizeMode.Interactive)
        self._table.horizontalHeader().setStretchLastSection(True)
        for col, w in enumerate(_COL_W):
            self._table.setColumnWidth(col, w)
        self._table.verticalHeader().setVisible(False)
        self._table.verticalHeader().setDefaultSectionSize(28)
        # No longer connect currentRowChanged to detail panel
        root.addWidget(self._table, stretch=1)

        # ── row action buttons ────────────────────────────────────────────
        btn_row = QHBoxLayout()
        for label, tip, slot in [
            ("▲",             "Move track up",          self._move_up),
            ("▼",             "Move track down",         self._move_down),
            ("✕ Remove",      "Remove selected track",   self._remove_track),
            ("✏ Add Track",   "Add a track manually",    self._manual_add_track),
            ("🔍 Fingerprint", "Fingerprint selection",  self._fingerprint_selected),
            ("💾 Export",     "Export selection",        self._export_selected),
        ]:
            b = QPushButton(label)
            b.setToolTip(tip)
            b.clicked.connect(slot)
            if label == "✕ Remove":
                b.setObjectName("danger")
            btn_row.addWidget(b)
        btn_row.addStretch()
        root.addLayout(btn_row)

        # ── log ───────────────────────────────────────────────────────────
        log_box = QGroupBox("Log")
        log_l = QVBoxLayout(log_box)
        log_l.setContentsMargins(4, 4, 4, 4)
        self._log_view = QTextEdit()
        self._log_view.setObjectName("log")
        self._log_view.setReadOnly(True)
        self._log_view.setMaximumHeight(120)
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
        # If already connected, cleanly disconnect first
        if self.pipe.connected:
            self._log("Reconnecting — closing existing pipe handles…")
            if self._heartbeat:
                self._heartbeat.stop()
            self.pipe.disconnect()
            self._conn_lbl.setText("  ⬤  Disconnected  ")
            self._conn_lbl.setObjectName("conn_off")
            self._conn_lbl.setStyle(self._conn_lbl.style())

        self._log("Opening pipe to Audacity…")
        ok, msg = self.pipe.connect()
        if ok:
            self._conn_lbl.setText("  ⬤  Verifying…  ")
            self._conn_lbl.setObjectName("conn_off")
            self._conn_lbl.setStyle(self._conn_lbl.style())
            self._log("Pipe opened — verifying Audacity round-trip…")
            self._start_verify()
        else:
            self._log(f"⚠  Connect failed: {msg.splitlines()[0]}")
            QMessageBox.critical(
                self, "Connection Failed", msg
            )

    def _start_verify(self) -> None:
        worker = ConnectVerifyWorker(self.pipe)
        worker.log.connect(self._log)
        worker.ok.connect(self._on_verify_ok)
        worker.failed.connect(self._on_verify_failed)
        # keep reference so it isn't GC'd
        self._verify_worker = worker
        worker.start()

    def _on_verify_ok(self, info: str) -> None:
        self._audacity_info = info
        self._conn_lbl.setText(f"  ⬤  {info}  ")
        self._conn_lbl.setObjectName("conn_on")
        self._conn_lbl.setStyle(self._conn_lbl.style())
        self._log(f"✓ Connected — {info}")
        self._start_heartbeat()

    def _on_verify_failed(self, msg: str) -> None:
        self._conn_lbl.setText("  ⬤  Pipe error  ")
        self._conn_lbl.setObjectName("conn_off")
        self._conn_lbl.setStyle(self._conn_lbl.style())
        self._log(f"⚠  Verification failed: {msg.splitlines()[0]}")
        QMessageBox.critical(self, "Connection Problem", msg)

    # ── heartbeat ─────────────────────────────────────────────────────────
    def _start_heartbeat(self) -> None:
        """Ping Audacity every 10 s from a QTimer → daemon thread.
        Keeps the connection indicator accurate and surfaces stalls early."""
        if self._heartbeat:
            self._heartbeat.stop()
        self._heartbeat = QTimer(self)
        self._heartbeat.setInterval(30_000)   # 30 seconds
        self._heartbeat.timeout.connect(self._do_heartbeat)
        self._heartbeat.start()

    def _do_heartbeat(self) -> None:
        """Fire-and-forget: run a ping in a daemon thread so the timer
        callback (main thread) returns immediately."""
        if not self.pipe.connected:
            self._heartbeat.stop()
            return
        # skip if a heavy worker is already holding the pipe
        if self._worker and self._worker.isRunning():
            return

        def _ping() -> None:
            ok, resp = self.pipe.ping()
            if ok:
                try:
                    tracks = json.loads(resp)
                    n = len(tracks)
                    info = f"{self._audacity_info}  ·  {n} track(s)"
                except Exception:
                    info = self._audacity_info
                # Qt objects can only be touched from main thread — use a
                # zero-duration singleShot to marshal back safely
                QTimer.singleShot(0, lambda i=info: self._on_heartbeat_ok(i))
            else:
                QTimer.singleShot(0, lambda r=resp: self._on_heartbeat_fail(r))

        t = threading.Thread(target=_ping, daemon=True)
        t.start()

    def _on_heartbeat_ok(self, info: str) -> None:
        self._conn_lbl.setText(f"  ⬤  {info}  ")
        self._conn_lbl.setObjectName("conn_on")
        self._conn_lbl.setStyle(self._conn_lbl.style())

    def _on_heartbeat_fail(self, reason: str) -> None:
        self._conn_lbl.setText("  ⬤  No response  ")
        self._conn_lbl.setObjectName("conn_off")
        self._conn_lbl.setStyle(self._conn_lbl.style())
        self._log(f"⚠  Heartbeat failed ({reason}) — is Audacity still running?")
        # If the pipe files have disappeared Audacity has quit; mark disconnected
        # so the user gets a clear prompt rather than more timeout errors.
        ok, _ = AudacityPipe.check_pipes()
        if not ok:
            self.pipe.disconnect()
            self._heartbeat.stop()
            self._conn_lbl.setText("  ⬤  Disconnected  ")
            self._conn_lbl.setStyle(self._conn_lbl.style())
            self._log("Pipe files gone — Audacity has exited. Click Connect to reconnect.")

    # ── diagnostics (manual) ──────────────────────────────────────────────
    def _run_diagnostics(self) -> None:
        """Manual diagnostics: dump pipe state and re-run round-trip verify."""
        self._log("─" * 60)
        self._log("🩺  Diagnostics")
        self._log(f"   Platform      : {sys.platform}")
        self._log(f"   Pipe → Audacity : {_TOFILE}")
        self._log(f"   Pipe ← Audacity : {_FROMFILE}")
        self._log(f"   pipe.connected  : {self.pipe.connected}")

        if sys.platform != "win32":
            import stat as _stat
            for label, path in [("→", _TOFILE), ("←", _FROMFILE)]:
                p = Path(path)
                if p.exists():
                    mode = p.stat().st_mode
                    kind = "FIFO ✓" if _stat.S_ISFIFO(mode) else "REGULAR FILE ✗"
                    self._log(f"   {label} {path}: {kind}")
                else:
                    self._log(f"   {label} {path}: MISSING ✗")

        pipe_ok, pipe_msg = AudacityPipe.check_pipes()
        self._log(f"   Pipe check: {'OK' if pipe_ok else '✗ ' + pipe_msg.splitlines()[0]}")

        if not self.pipe.connected:
            self._log("   Not connected — click Connect first for a round-trip test.")
            return
        self._start_verify()

    # ── silence detection ─────────────────────────────────────────────────
    def _detect_silence(self) -> None:
        if not self.pipe.connected:
            QMessageBox.warning(self, "Not Connected", "Connect to Audacity first.")
            return
        self._run_silence_worker(run_detect=True)

    # ── label import ──────────────────────────────────────────────────────
    def _import_labels(self) -> None:
        if not self.pipe.connected:
            QMessageBox.warning(self, "Not Connected", "Connect to Audacity first.")
            return
        self._run_silence_worker(run_detect=False)

    def _run_silence_worker(self, run_detect: bool) -> None:
        """Spin up a SilenceWorker so pipe I/O never blocks the main thread."""
        if self._worker and self._worker.isRunning():
            QMessageBox.warning(self, "Busy", "A background task is already running.")
            return
        worker = SilenceWorker(self.pipe, self.cfg, run_detect=run_detect)
        worker.log.connect(self._log)
        worker.error.connect(self._on_silence_error)
        worker.tracks_ready.connect(self._on_tracks_ready)
        worker.finished.connect(self._on_worker_done)
        self._worker = worker
        self._progress.setVisible(True)
        self._progress.setRange(0, 0)   # indeterminate spinner
        self._log("Starting silence worker…")
        worker.start()

    def _on_silence_error(self, msg: str) -> None:
        self._progress.setVisible(False)
        self._log(f"⚠  {msg}")
        QMessageBox.warning(self, "Silence Detection", msg)

    def _on_tracks_ready(self, new_tracks: list) -> None:
        self.tracks.clear()
        self.tracks.extend(new_tracks)
        self._model.refresh_all()
        self._log(f"Imported {len(new_tracks)} track(s) from Audacity labels.")

    # ── track list actions ────────────────────────────────────────────────
    def _populate_apply_strip(self) -> None:
        """Pre-fill the Apply strip from config defaults."""
        sec = self.cfg["vinyl_ripper"]
        for key, le in self._apply_fields.items():
            val = sec.get(f"default_{key}", "")
            if val:
                le.setText(val)

    def _apply_to_all(self) -> None:
        """Copy non-empty Apply strip values to every track."""
        if not self.tracks:
            return
        values = {k: le.text().strip()
                  for k, le in self._apply_fields.items()
                  if le.text().strip()}
        if not values:
            return
        for t in self.tracks:
            for k, v in values.items():
                setattr(t, k, v)
        self._model.refresh_all()
        self._log(f"Applied {', '.join(values)} to all {len(self.tracks)} track(s).")

    def _current_row(self) -> int:
        return self._table.currentIndex().row()

    def _on_row_changed(self, current: QModelIndex, _prev: QModelIndex) -> None:
        pass  # detail panel removed; in-place table editing handles all changes

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
            self._populate_apply_strip()
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
        # Reset progress bar to determinate mode (in case silence worker left
        # it as an indeterminate spinner) then hide it.
        self._progress.setRange(0, 1)
        self._progress.setVisible(False)
        self._model.refresh_all()
        self._log("Done.")

    # ── manual MB / Discogs lookup ────────────────────────────────────────
    def _mb_lookup_selected(self, title: str = "", artist: str = "") -> None:
        """MusicBrainz lookup for the selected track (also callable from toolbar)."""
        row = self._current_row()
        if row < 0:
            QMessageBox.information(self, "No Selection", "Select a track first.")
            return
        t = self.tracks[row]
        title  = title  or t.title
        artist = artist or t.artist
        self._log(f"MusicBrainz search: '{title}' by '{artist}'…")
        meta = mb_search(title, artist)
        if meta:
            FingerprintWorker._merge(meta, meta, t)
            for k, v in meta.items():
                if v:
                    setattr(t, k, v)
            self._model.refresh_row(row)
            self._log(f"  MB: {t.title} / {t.artist} / {t.album}")
        else:
            self._log("  No MusicBrainz result found.")

    def _discogs_lookup_selected(self, artist: str = "", album: str = "") -> None:
        """Discogs lookup for the selected track (also callable from toolbar)."""
        row = self._current_row()
        if row < 0:
            QMessageBox.information(self, "No Selection", "Select a track first.")
            return
        t = self.tracks[row]
        artist = artist or t.artist
        album  = album  or t.album
        token  = self.cfg["vinyl_ripper"].get("discogs_token", "")
        if not token:
            QMessageBox.warning(self, "Discogs",
                                "Add your Discogs personal token in Settings → API Keys.")
            return
        self._log(f"Discogs search: '{artist}' / '{album}'…")
        meta = discogs_search(artist, album, token)
        if meta:
            for k, v in [("album_artist", meta.get("album_artist","")),
                         ("genre",        meta.get("genre","")),
                         ("year",         meta.get("year",""))]:
                if v and not getattr(t, k):
                    setattr(t, k, v)
            if meta.get("release_id"):
                t.discogs_release_id = meta["release_id"]
            self._model.refresh_row(row)
            self._log(f"  Discogs: album_artist={t.album_artist} genre={t.genre}")
        else:
            self._log("  No Discogs result found.")

    # ── window lifecycle ──────────────────────────────────────────────────
    def closeEvent(self, event) -> None:
        """Release pipe handles cleanly so Audacity can reuse them.

        We deliberately do NOT delete the FIFO files — they belong to
        Audacity and it will reuse or recreate them on next startup.
        """
        if self._heartbeat:
            self._heartbeat.stop()
        if self._worker and self._worker.isRunning():
            self._worker.quit()
            self._worker.wait(2000)
        self.pipe.disconnect()
        event.accept()

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
