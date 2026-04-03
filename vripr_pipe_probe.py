#!/usr/bin/env python3
"""
vripr_pipe_probe.py
===================
Diagnostic script — run alongside Audacity (mod-script-pipe enabled) to
discover the exact command names and parameters available for silence detection.

Usage:
    python3 vripr_pipe_probe.py 2>&1 | tee probe_results.txt

Do NOT run vripr at the same time — both cannot hold the pipe simultaneously.
"""
import os, sys, json, time, threading, subprocess
import queue as _q
from pathlib import Path

# ── Pipe paths ──────────────────────────────────────────────────────────────
if sys.platform == "win32":
    TOFILE   = r"\\.\pipe\ToSrvPipe"
    FROMFILE = r"\\.\pipe\FromSrvPipe"
    EOL      = "\r\n\0"
else:
    uid      = os.getuid()
    TOFILE   = f"/tmp/audacity_script_pipe.to.{uid}"
    FROMFILE = f"/tmp/audacity_script_pipe.from.{uid}"
    EOL      = "\n"


# ── Focus raise ─────────────────────────────────────────────────────────────
def raise_audacity_window(tofile=None) -> None:
    """Best-effort window raise — mirrors vripr send() logic exactly."""
    if tofile:
        try:
            tofile.write("Raise:" + EOL)
            tofile.flush()
            time.sleep(0.1)
        except Exception:
            pass

    if sys.platform == "darwin":
        try:
            subprocess.run(["osascript", "-e",
                            'tell application "Audacity" to activate'],
                           timeout=3, capture_output=True)
        except Exception:
            pass

    elif sys.platform.startswith("linux"):
        try:
            subprocess.run(["wmctrl", "-a", "Audacity"],
                           timeout=3, capture_output=True)
            return
        except Exception:
            pass
        try:
            r = subprocess.run(["xdotool", "search", "--name", "Audacity"],
                               timeout=3, capture_output=True, text=True)
            wids = r.stdout.strip().splitlines()
            if wids:
                subprocess.run(["xdotool", "windowactivate", "--sync", wids[0]],
                               timeout=3, capture_output=True)
        except Exception:
            pass

    elif sys.platform == "win32":
        try:
            import ctypes
            user32 = ctypes.windll.user32
            found = []
            PROC = ctypes.WINFUNCTYPE(ctypes.c_bool,
                                      ctypes.POINTER(ctypes.c_int),
                                      ctypes.POINTER(ctypes.c_int))
            def _cb(hwnd, _):
                n = user32.GetWindowTextLengthW(hwnd)
                if n:
                    buf = ctypes.create_unicode_buffer(n + 1)
                    user32.GetWindowTextW(hwnd, buf, n + 1)
                    if "Audacity" in buf.value:
                        found.append(hwnd)
                return True
            user32.EnumWindows(PROC(_cb), 0)
            if found:
                user32.SetForegroundWindow(found[0])
        except Exception:
            pass

    time.sleep(0.3)


# ── send() with watchdog timeout — same pattern as vripr ────────────────────
def send(tofile, fromfile, cmd: str, timeout: float = 15.0,
         needs_focus: bool = True, label: str = None) -> str:
    print(f"\n>>> {label or cmd}")

    if needs_focus:
        raise_audacity_window(tofile)

    tofile.write(cmd + EOL)
    tofile.flush()

    lines = []
    timed_out = [False]
    result_q = _q.Queue()

    def _read():
        try:
            while True:
                if timed_out[0]:
                    result_q.put(TimeoutError(
                        f"No response within {timeout}s — "
                        "is Audacity focused / is a project open?"
                    ))
                    return
                line = fromfile.readline()
                if not line:
                    result_q.put(ConnectionResetError("Audacity closed the pipe"))
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

    threading.Thread(target=_read,     daemon=True).start()
    threading.Thread(target=_watchdog, daemon=True).start()

    result = result_q.get()
    if isinstance(result, Exception):
        print(f"!!! {type(result).__name__}: {result}")
        return ""
    preview = result[:400].replace("\n", " ")
    print(f"<<< {preview!r}")
    return result


# ── main ────────────────────────────────────────────────────────────────────
def main():
    print(f"Platform : {sys.platform}")
    print(f"Pipe  →  : {TOFILE}")
    print(f"Pipe  ←  : {FROMFILE}")

    if sys.platform != "win32":
        import stat as _stat
        for arrow, path in [("→", TOFILE), ("←", FROMFILE)]:
            p = Path(path)
            if not p.exists():
                print(f"  {arrow} MISSING — Audacity not running or "
                      "mod-script-pipe not Enabled")
                sys.exit(1)
            kind = "FIFO ✓" if _stat.S_ISFIFO(p.stat().st_mode) else "REGULAR FILE ✗"
            print(f"  {arrow} {path}: {kind}")

    print("\nOpening pipes...")
    tofile   = open(TOFILE,   "w",                buffering=1)
    fromfile = open(FROMFILE, "r", encoding="utf-8", buffering=1)
    print("Pipes open.\n")

    # 1. Version
    print("=" * 60)
    send(tofile, fromfile, "Version:", timeout=15.0)

    # 2. Track info
    raw = send(tofile, fromfile, "GetInfo: Type=Tracks Format=JSON", timeout=15.0)
    try:
        tracks = json.loads(raw)
        print(f"  → {len(tracks)} track(s) in project")
        for t in tracks:
            print(f"    name={t.get('name')!r}  end={t.get('end')}s  "
                  f"channels={t.get('channels')}")
    except Exception:
        pass

    # 3. Full command list — filter for silence/analyze/label
    print("\n" + "=" * 60)
    print("Fetching full command list (may take a few seconds)...")
    raw = send(tofile, fromfile, "GetInfo: Type=Commands Format=JSON",
               timeout=20.0, label="GetInfo: Commands")
    try:
        cmds = json.loads(raw)
        print(f"Total commands available: {len(cmds)}")
        keywords = ["silence", "find", "label", "analyze", "detect", "region"]
        matches = [c for c in cmds
                   if any(k in json.dumps(c).lower() for k in keywords)]
        print(f"\nSilence / analyze / label related ({len(matches)} found):")
        for m in matches:
            print(f"  {json.dumps(m)}")
    except Exception as e:
        print(f"  Parse error: {e}\n  Raw: {raw[:300]}")

    # 4. Probe candidate command names
    print("\n" + "=" * 60)
    print("Probing candidate SilenceFind command names via Help:...")
    for name in ["SilenceFind", "SilenceFinder", "FindSilences",
                 "AnalyzeSilence", "DetectSilence"]:
        resp = send(tofile, fromfile, f"Help: CommandName={name}",
                    timeout=10.0, label=f"Help: {name}")
        if resp.strip():
            print(f"  *** FOUND: {name}")
            print(f"      {resp[:300]}")

    # 5. Menu scan
    print("\n" + "=" * 60)
    print("Scanning menus for silence/analyze items...")
    raw = send(tofile, fromfile, "GetInfo: Type=Menus Format=JSON",
               timeout=20.0, label="GetInfo: Menus")
    try:
        menus = json.loads(raw)
        for item in menus:
            s = json.dumps(item).lower()
            if any(k in s for k in ["silence", "find", "analyze"]):
                print(f"  {json.dumps(item)}")
    except Exception as e:
        print(f"  Parse error: {e}")

    tofile.close()
    fromfile.close()
    print("\n" + "=" * 60)
    print("Probe complete. Paste the full output here for analysis.")


if __name__ == "__main__":
    main()
