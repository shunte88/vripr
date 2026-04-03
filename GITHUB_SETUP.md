# Creating the vripr GitHub repository

Step-by-step commands to initialise the local repo, create the remote on
GitHub, and push the initial codebase. Run these from the root of the
unzipped `vripr/` folder.

---

## Prerequisites

| Tool | Install |
|---|---|
| Git | [git-scm.com](https://git-scm.com/) |
| GitHub CLI (`gh`) | [cli.github.com](https://cli.github.com/) **or** use the web UI (Step 2b) |
| uv | `curl -LsSf https://astral.sh/uv/install.sh \| sh` |

---

## Step 1 — Replace the placeholder username

Every file that contains `YOUR_GITHUB_USERNAME` must be updated to your real
GitHub handle before you push.

```bash
# macOS / Linux (sed in-place)
YOUR_USERNAME="your_actual_github_username"   # ← edit this line

grep -rl "YOUR_GITHUB_USERNAME" . \
  --include="*.md" --include="*.toml" --include="*.yml" \
  | xargs sed -i "s/YOUR_GITHUB_USERNAME/${YOUR_USERNAME}/g"

# Verify
grep -r "YOUR_GITHUB_USERNAME" . --include="*.md" --include="*.toml" --include="*.yml"
# should return nothing
```

```powershell
# Windows PowerShell equivalent
$username = "your_actual_github_username"   # ← edit this line
Get-ChildItem -Recurse -Include *.md,*.toml,*.yml |
  ForEach-Object {
    (Get-Content $_.FullName) -replace "YOUR_GITHUB_USERNAME", $username |
    Set-Content $_.FullName
  }
```

---

## Step 2a — Create the GitHub repo with the CLI (recommended)

```bash
# Authenticate (one-time)
gh auth login

# Create the public repo and push in one command
gh repo create vripr \
  --public \
  --description "The vinyl viper for perfect rips — Audacity vinyl ripping helper" \
  --homepage "https://${YOUR_USERNAME}.github.io/vripr" \
  --clone=false \
  --source=. \
  --remote=origin \
  --push
```

## Step 2b — Create the GitHub repo via the web UI (alternative)

1. Go to <https://github.com/new>
2. Set **Repository name** to `vripr`
3. Set description: *The vinyl viper for perfect rips*
4. Choose **Public**
5. Leave "Add a README" **unchecked** (we already have one)
6. Click **Create repository**

Then add the remote locally:
```bash
git remote add origin https://github.com/${YOUR_USERNAME}/vripr.git
```

---

## Step 3 — Initialise git and make the first commit

```bash
# Inside the vripr/ directory:
git init -b main

git add .
git commit -m "feat: initial commit — vripr 0.1.0

The vinyl viper for perfect rips.

- PyQt6 dark-themed GUI
- Audacity mod-script-pipe integration
- Chromaprint/AcoustID fingerprinting
- MusicBrainz + Discogs metadata enrichment
- mutagen FLAC/MP3 tag writing
- uv-managed project with hatchling build backend
- GitHub Actions CI (lint, type-check, test, build)"
```

---

## Step 4 — Push to GitHub

```bash
git push -u origin main
```

---

## Step 5 — Create the develop branch

```bash
git checkout -b develop
git push -u origin develop
```

---

## Step 6 — Set develop as the default PR target (optional, CLI)

```bash
gh repo edit vripr --default-branch main
# PRs should target 'develop'; keep 'main' for tagged releases
```

---

## Step 7 — Set up branch protection (recommended)

```bash
# Protect main — require PR + passing CI before merge
gh api repos/${YOUR_USERNAME}/vripr/branches/main/protection \
  --method PUT \
  --field required_status_checks='{"strict":true,"contexts":["test (ubuntu-latest, 3.12)","lint"]}' \
  --field enforce_admins=false \
  --field required_pull_request_reviews='{"required_approving_review_count":1}' \
  --field restrictions=null
```

Or do it manually: **Settings → Branches → Add branch protection rule → `main`**.

---

## Step 8 — Add API secrets for PyPI publishing (optional)

If you plan to publish to PyPI:

1. Go to your repo → **Settings → Secrets and variables → Actions**
2. The release workflow uses **OIDC trusted publishing** — no token needed if
   you configure it on PyPI's side:
   - PyPI → Account Settings → Publishing → Add a new pending publisher
   - Set: owner = your GitHub username, repo = `vripr`, workflow = `release.yml`

---

## Step 9 — Install and verify locally

```bash
uv sync --all-extras
uv run pytest          # all tests should pass
uv run vripr           # app should launch
```

---

## Full command sequence (copy-paste summary)

```bash
# 0. Unzip and enter the directory
unzip vripr.zip
cd vripr

# 1. Replace placeholder username
YOUR_USERNAME="your_actual_github_username"
grep -rl "YOUR_GITHUB_USERNAME" . \
  --include="*.md" --include="*.toml" --include="*.yml" \
  | xargs sed -i "s/YOUR_GITHUB_USERNAME/${YOUR_USERNAME}/g"

# 2. Init git
git init -b main
git add .
git commit -m "feat: initial commit — vripr 0.1.0"

# 3. Create GitHub repo and push
gh auth login   # if not already authenticated
gh repo create vripr \
  --public \
  --description "The vinyl viper for perfect rips" \
  --source=. --remote=origin --push

# 4. Create develop branch
git checkout -b develop
git push -u origin develop

# 5. Install and test
uv sync --all-extras
uv run pytest
uv run vripr
```
