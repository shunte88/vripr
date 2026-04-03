# Contributing to vripr

Thank you for considering a contribution! The sections below cover everything
you need to get up and running.

---

## Development setup

### Prerequisites

| Tool | Purpose | Install |
|---|---|---|
| Python ≥ 3.10 | Runtime | [python.org](https://www.python.org/downloads/) |
| uv | Dependency manager | `curl -LsSf https://astral.sh/uv/install.sh \| sh` |
| fpcalc | Chromaprint CLI | see README |
| Audacity ≥ 3.x | Target application | [audacityteam.org](https://www.audacityteam.org/) |

### Clone and install

```bash
git clone https://github.com/YOUR_GITHUB_USERNAME/vripr.git
cd vripr

# create virtual env + install all deps (runtime + dev)
uv sync --all-extras

# verify
uv run python -m vripr --version
```

---

## Running the app

```bash
uv run vripr
# or
uv run python -m vripr
```

---

## Code quality

All of the following must pass before opening a PR:

```bash
# Lint
uv run ruff check src/ tests/

# Auto-fix safe issues
uv run ruff check --fix src/ tests/

# Format
uv run ruff format src/ tests/

# Type-check
uv run mypy src/vripr/

# Tests (headless Qt via Xvfb on Linux)
uv run pytest
```

You can run them all at once with:

```bash
uv run ruff check src/ tests/ && uv run mypy src/vripr/ && uv run pytest
```

---

## Project layout

```
vripr/
├── src/
│   └── vripr/
│       ├── __init__.py       # version, metadata
│       ├── __main__.py       # python -m vripr entry point
│       └── app.py            # entire application (single-module for now)
├── tests/
│   ├── test_track_meta.py    # TrackMeta + TrackTableModel unit tests
│   ├── test_pipe.py          # AudacityPipe unit tests (offline)
│   └── test_tagging.py       # apply_tags unit tests (mocked mutagen)
├── docs/                     # MkDocs source
├── scripts/                  # helper shell scripts
├── .github/
│   ├── workflows/
│   │   ├── ci.yml            # lint + type + test + build
│   │   └── release.yml       # tag-triggered PyPI publish
│   ├── ISSUE_TEMPLATE/
│   └── PULL_REQUEST_TEMPLATE.md
├── pyproject.toml            # uv / hatchling project manifest
├── requirements.txt          # pip fallback
├── requirements-dev.txt      # dev extras pip fallback
├── .python-version           # uv Python pin
├── .gitignore
├── LICENSE
├── CHANGELOG.md
└── README.md
```

---

## Branching model

| Branch | Purpose |
|---|---|
| `main` | Stable, tagged releases only |
| `develop` | Integration branch — PRs target here |
| `feat/<name>` | New feature branches |
| `fix/<name>` | Bug fix branches |
| `chore/<name>` | Tooling, CI, docs |

---

## Commit messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add Discogs release artwork download
fix: silence detection threshold off-by-one
chore: bump PyQt6 to 6.7.0
docs: add fpcalc Windows install notes
test: cover AudacityPipe.send sentinel handling
```

---

## Opening a pull request

1. Fork the repo and create a branch off `develop`.
2. Make your changes with tests.
3. Run the full quality suite (see above).
4. Update `CHANGELOG.md` under `[Unreleased]`.
5. Open a PR against `develop` and fill in the PR template.

---

## Releasing (maintainers only)

```bash
# 1. Bump version in src/vripr/__init__.py and pyproject.toml
# 2. Update CHANGELOG.md — move [Unreleased] items under the new version
# 3. Commit
git add .
git commit -m "chore: release v0.2.0"

# 4. Tag
git tag -a v0.2.0 -m "Release v0.2.0"
git push origin main --tags
# The release workflow fires automatically and publishes to PyPI
```
