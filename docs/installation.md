# Installation

## Requirements

| Requirement | Minimum version | Notes |
|---|---|---|
| Python | 3.10 | 3.12 recommended |
| Audacity | 3.0 | mod-script-pipe must be enabled |
| fpcalc (Chromaprint) | any | for fingerprinting |

---

## 1 — Install vripr

### With uv (recommended)
```bash
uv tool install vripr
```

### With pip
```bash
pip install vripr
```

### From source
```bash
git clone https://github.com/YOUR_GITHUB_USERNAME/vripr.git
cd vripr
uv sync --all-extras   # installs runtime + dev deps
```

---

## 2 — Install fpcalc (Chromaprint)

Run the bundled helper script:
```bash
bash scripts/install_fpcalc.sh
```

Or manually:

| OS | Command |
|---|---|
| macOS | `brew install chromaprint` |
| Ubuntu/Debian | `sudo apt install libchromaprint-tools` |
| Fedora | `sudo dnf install chromaprint-tools` |
| Arch | `sudo pacman -S chromaprint` |
| Windows | Download from [acoustid.org/chromaprint](https://acoustid.org/chromaprint) and add `fpcalc.exe` to your `PATH` |

---

## 3 — Enable mod-script-pipe in Audacity

See the [step-by-step guide](../scripts/setup_audacity_pipe.md).

---

## 4 — Get API keys

| Service | URL | Cost |
|---|---|---|
| AcoustID | [acoustid.org/login](https://acoustid.org/login) | Free |
| Discogs | [discogs.com/settings/developers](https://www.discogs.com/settings/developers) | Free personal token |

Enter them in vripr via **⚙ Settings → API Keys**.

---

## 5 — Launch

```bash
vripr
# or
python -m vripr
```
