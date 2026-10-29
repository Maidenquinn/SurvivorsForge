# Running the SurvivorsForge platform

The platform is three programs and a database, all started by Docker Compose:

| Service | What it does |
|---|---|
| `api` | `forge-api serve`: the public API, GitHub login, webhooks |
| `worker` | `forge-api worker`: syncs releases, scans every new file, checks that files on GitHub stay unchanged |
| `web` | `forge-web`: the website, a client of the API |
| `caddy` | TLS certificates, both domains, the uploaded images |
| `postgres` | all data except images |

Releases and downloads stay on GitHub. The server keeps metadata, scan results and images.

## What you need

- A Linux server with Docker and the Compose plugin (2.24 or newer). Ports 80 and 443 open.
- Two DNS `A` (or `AAAA`) records pointing at the server, for example `forge.example.com` for
  the website and `api.forge.example.com` for the API.
- A GitHub account to own the GitHub App.

## 1. Create the GitHub App

On GitHub: *Settings → Developer settings → GitHub Apps → New GitHub App*.

| Field | Value |
|---|---|
| GitHub App name | e.g. `SurvivorsForge`; its URL name becomes `GITHUB_APP_SLUG` |
| Homepage URL | `https://forge.example.com` |
| Callback URL | `https://api.forge.example.com/v1/auth/github/callback` |
| Request user authorization (OAuth) during installation | off |
| Webhook | active, URL `https://api.forge.example.com/v1/github/webhook` |
| Webhook secret | a long random value (`openssl rand -hex 32`), becomes `GITHUB_WEBHOOK_SECRET` |
| Repository permissions | `Contents: Read-only`, `Metadata: Read-only` |
| Account permissions | none |
| Subscribe to events | `Release`, `Create`, `Repository` (`Installation` and `Installation repositories` are always sent) |
| Where can this GitHub App be installed? | Any account |

After saving, note from the App's settings page:

- **App ID** → `GITHUB_APP_ID`
- **Client ID** → `GITHUB_CLIENT_ID`
- **Client secret** (generate one) → `GITHUB_CLIENT_SECRET`
- **Private key** (generate one, a `.pem` file) → see `GITHUB_APP_PRIVATE_KEY_PATH`
- **URL name** of the App (`github.com/apps/<name>`) → `GITHUB_APP_SLUG`

## 2. Configure

```bash
git clone https://github.com/Rezondes/SurvivorsForge.git
cd SurvivorsForge
cp .env.example .env
mkdir -p secrets
cp ~/Downloads/your-app.private-key.pem secrets/github-app.pem
chmod 644 secrets/github-app.pem
```

The containers run as the unprivileged user `nonroot` (uid 65532), so the key must be readable
for it (`chmod 644`, or `chown 65532` and `chmod 600`). `.env` and `secrets/` are git-ignored;
never commit them.

Your own GitHub user ID for `ADMIN_GITHUB_IDS` is the `id` field of
`https://api.github.com/users/<your-login>`.

### Every variable in `.env`

**Docker Compose**

| Variable | Required | Meaning | Example |
|---|---|---|---|
| `POSTGRES_PASSWORD` | yes | Database password; Compose builds the database URL from it | output of `openssl rand -hex 24` |
| `DOMAIN_WEB` | yes | Domain of the website, Caddy gets its certificate | `forge.example.com` |
| `DOMAIN_API` | yes | Domain of the API | `api.forge.example.com` |
| `ACME_EMAIL` | yes | Contact for Let's Encrypt | `admin@example.com` |
| `FORGE_EDGE_SUBNET` | no | Subnet of the internal edge network; change it if it clashes with another Docker network | `10.213.17.0/24` |
| `FORGE_CADDY_IP` | no | Caddy's fixed address inside that subnet; the API trusts its `X-Forwarded-For` | `10.213.17.10` |
| `FORGE_WEB_IP` | no | The website's fixed address inside that subnet; the API trusts the client IP it forwards | `10.213.17.11` |
| `GITHUB_APP_PRIVATE_KEY_PATH` | no | Path of the App's `.pem` on the host | `./secrets/github-app.pem` |

**API (`forge-api`)**

| Variable | Required | Meaning | Example |
|---|---|---|---|
| `DATABASE_URL` | yes, set by Compose | Postgres connection | set by `compose.yaml` |
| `PUBLIC_API_URL` | yes | Public base URL of the API, no trailing slash | `https://api.forge.example.com` |
| `PUBLIC_IMAGE_URL` | yes | Public base URL of uploaded images | `https://forge.example.com/img` |
| `IMAGE_DIR` | no, set by Compose | Folder the API stores images in | `/data/images` |
| `BIND_ADDR` | no | Address the API listens on in its container | `0.0.0.0:8080` |
| `MIGRATE_ON_START` | no | Apply database migrations when the API starts | `true` |
| `RATE_LIMIT_PER_MINUTE` | no | Requests per minute and client IP, for the API and the website each | `120` |
| `CACHE_SECONDS` | no | How long public answers stay in the API's memory cache; a write through the API empties it, worker changes show after this time; `0` turns it off | `60` |
| `TRUSTED_PROXIES` | no, set by Compose | Proxies whose `X-Forwarded-For` counts | API: `FORGE_CADDY_IP` and `FORGE_WEB_IP`; website: `FORGE_CADDY_IP` |
| `SOURCE_CHECK_BATCH` | no | Versions compared with GitHub per 6-hourly run | `200` |
| `GITHUB_API_URL` | no | GitHub REST API | `https://api.github.com` |
| `GITHUB_APP_ID` | yes | App ID from step 1 | `123456` |
| `GITHUB_APP_PRIVATE_KEY` | no | The key as text (newlines as `\n`) instead of a file | empty |
| `GITHUB_APP_PRIVATE_KEY_FILE` | no, set by Compose | Path of the key inside the container | `/run/secrets/github-app.pem` |
| `GITHUB_WEBHOOK_SECRET` | yes | Webhook secret from step 1 | random value |
| `GITHUB_OAUTH_URL` | no | GitHub's OAuth pages | `https://github.com` |
| `GITHUB_CLIENT_ID` | yes | Client ID from step 1 | `Iv1.abc...` |
| `GITHUB_CLIENT_SECRET` | yes | Client secret from step 1 | secret |
| `AUTH_RETURN_URLS` | yes | Exact callback URLs a login may return to | `https://forge.example.com/auth/callback` |
| `ADMIN_GITHUB_IDS` | no | Comma-separated numeric GitHub IDs of admins | `1234567` |

**Website (`forge-web`)**

| Variable | Required | Meaning | Example |
|---|---|---|---|
| `API_INTERNAL_URL` | yes, set by Compose | How the website reaches the API | `http://api:8080` |
| `PUBLIC_WEB_URL` | yes | Public base URL of the website | `https://forge.example.com` |
| `COOKIE_KEY` | yes | At least 32 random characters, encrypts the session cookies | output of `openssl rand -hex 32` |
| `GITHUB_APP_SLUG` | yes | URL name of the App, for the "install the App" link | `survivorsforge` |
| `WEB_BIND_ADDR` | no | Address the website listens on in its container | `0.0.0.0:8080` |

## 3. First start

```bash
docker compose up -d --build
docker compose ps
```

All five services should become `healthy`. The API applies the database migrations on start
(`MIGRATE_ON_START=true`); to run them by hand: `docker compose exec api forge-api migrate`.

Carry over the old verified list. The addons are created without an owner; their authors claim
them later on the website by proving admin rights on the repo:

```bash
docker compose cp registry/verified.json api:/tmp/verified.json
docker compose exec api forge-api import-verified /tmp/verified.json
```

Then log in on the website with your GitHub account (it must be in `ADMIN_GITHUB_IDS`), open
*Admin → Review* once the first scans are done and verify the versions you trust.

To publish the verified list for older launchers, export it and commit `registry/verified.json`:

```bash
docker compose exec api forge-api export-verified > registry/verified.json
```

## 4. Updates

```bash
git pull
docker compose up -d --build
```

After a release with new scanner rules, scan all versions again:

```bash
docker compose exec api forge-api rescan --all
```

## 5. Backups

Two things hold data: the database and the image volume.

```bash
# Database, e.g. daily from cron
docker compose exec -T postgres pg_dump -U forge -Fc forge > backup/forge-$(date +%F).dump
# Images
docker run --rm -v survivorsforge_images:/data -v "$PWD/backup":/backup alpine \
  tar czf /backup/images-$(date +%F).tar.gz -C /data .
```

Restore:

```bash
docker compose up -d postgres
docker compose exec -T postgres pg_restore -U forge -d forge --clean --if-exists < backup/forge-2026-01-01.dump
docker run --rm -v survivorsforge_images:/data -v "$PWD/backup":/backup alpine \
  tar xzf /backup/images-2026-01-01.tar.gz -C /data
docker compose up -d
```

The volume name starts with the Compose project name, by default the folder name.

## 6. The launcher

A launcher reads the platform when it is built with the API's address:

```bash
SURVIVORSFORGE_API_URL=https://api.forge.example.com cargo build --release
```

In the release workflow, set `SURVIVORSFORGE_API_URL` as an environment variable of the build
step. Without it the launcher reads only `registry/verified.json`, as before.

## Running only the API

The website and Caddy are optional; the API runs on its own:

```bash
docker compose up -d postgres api worker
```

Its port is not published then; put your own proxy in front or add a `ports` entry for `api`.

## Local development

```bash
POSTGRES_PASSWORD=forge POSTGRES_HOST_PORT=15432 docker compose -f compose.yaml -f compose.dev.yaml up -d postgres
DATABASE_URL=postgres://forge:forge@127.0.0.1:15432/forge cargo test --workspace
bash scripts/compose-smoke.sh
```
