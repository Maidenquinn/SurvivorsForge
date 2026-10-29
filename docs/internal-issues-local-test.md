# Test the internal issues feature locally (Windows)

This branch is experimental and must not be deployed to production before compilation, database tests and review.

## Requirements
- Windows 10/11, Git, Docker Desktop (WSL2 backend)
- A test GitHub App for authenticated issue creation; browsing issues works without login
- Free ports 3080 and 3081

## Download this branch

In PowerShell:

```powershell
git clone --branch feat/internal-issues https://github.com/Maidenquinn/SurvivorsForge.git
cd SurvivorsForge
```

If you already cloned the repository:

```powershell
git fetch origin
git switch feat/internal-issues
git pull --ff-only
```

## Configure and start

Follow [local-setup.md](local-setup.md) to create your own **test** GitHub App. Do not use production secrets.

```powershell
Copy-Item .env.local.example .env.local
New-Item -ItemType Directory -Force secrets
# Copy your test GitHub App private key to secrets/github-app-local.pem
# Edit .env.local and set GitHub App credentials and ADMIN_GITHUB_IDS
function dcl { docker compose --env-file .env.local -f compose.yaml -f compose.local.yaml @args }
dcl up -d --build
dcl ps
dcl logs --tail 100 api web
```

Open http://localhost:3080 . If either service is unhealthy, check the logs before continuing.

## Manual acceptance test

1. Import the verified registry as described in local-setup.md, or publish your own test addon.
2. Visit `http://localhost:3080/addons/<your-addon-slug>/issues`. It should show an empty issue list.
3. Log in with GitHub and submit a **bug** with a title and body. Confirm you are redirected to its detail page.
4. Reply to the issue, refresh, and confirm the reply persists.
5. As the addon owner or a member with description-edit permission, set status to `in_progress`, then `resolved`.
6. As a normal user, confirm you cannot change another addon's issue status (the API should return 403).
7. Verify that posting without login is rejected and the site requires a valid CSRF token.
8. Verify that empty or oversized fields are rejected.
9. Restart the stack and confirm issues and replies remain.

## Rust checks (if Rust is installed)

```powershell
cargo fmt --all -- --check
cargo test -p forge-api -p forge-web
cargo clippy -p forge-api -p forge-web --all-targets -- -D warnings
```

## Clean up

```powershell
dcl down
```

Use `dcl down -v` **only** when you intend to delete all local test data.

## Known limitations in this draft
- Lists are limited to the newest 100 issues; pagination and issue search are not implemented.
- The interface currently shows API error pages rather than inline form validation.
- Issue and comment editing/deletion, notifications and attachments are not implemented.
- This contribution has not yet passed a verified local Rust build or PostgreSQL integration test.
