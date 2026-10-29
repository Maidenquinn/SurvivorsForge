#!/usr/bin/env bash
# Builds the images, starts Postgres, API and worker without website and proxy, checks that the
# API answers /healthz and the worker reaches the database, then removes everything again.
# Uses throwaway values only: a key generated for this run, no real GitHub App.
set -euo pipefail
cd "$(dirname "$0")/.."

work="$(mktemp -d)"
project="forge-smoke-$$"
cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    docker compose -p "$project" logs --tail 40 api worker || true
  fi
  docker compose -p "$project" down -v --remove-orphans >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT

# Git Bash on Windows: hand Docker real Windows paths.
host_path() { if command -v cygpath >/dev/null; then cygpath -w "$1"; else echo "$1"; fi; }

# The containers run as the distroless user nonroot (uid 65532), so the key must be readable for it.
MSYS_NO_PATHCONV=1 docker run --rm -v "$(host_path "$work"):/out" --entrypoint sh alpine/openssl   -c "openssl genrsa -out /out/app.pem 2048 && chmod 644 /out/app.pem" >/dev/null 2>&1
cat > "$work/smoke.env" <<EOF
PUBLIC_API_URL=http://api.smoke.test
PUBLIC_IMAGE_URL=http://web.smoke.test/img
MIGRATE_ON_START=true
GITHUB_APP_ID=1
GITHUB_CLIENT_ID=smoke
GITHUB_CLIENT_SECRET=smoke
GITHUB_WEBHOOK_SECRET=smoke
EOF

export FORGE_ENV_FILE="$(host_path "$work/smoke.env")"
export GITHUB_APP_PRIVATE_KEY_PATH="$(host_path "$work/app.pem")"
export POSTGRES_PASSWORD="smoke-$$"

docker compose -p "$project" up -d --build --wait postgres api worker
MSYS_NO_PATHCONV=1 docker compose -p "$project" exec -T api /usr/local/bin/forge-api healthcheck http://127.0.0.1:8080/healthz
MSYS_NO_PATHCONV=1 docker compose -p "$project" exec -T worker /usr/local/bin/forge-api healthcheck db
echo "smoke test passed"
