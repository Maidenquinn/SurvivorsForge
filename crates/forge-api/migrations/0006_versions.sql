-- Set when GitHub no longer lets the platform follow the repo (private, deleted, App removed).
-- A locked addon gets no new versions until the repo is reachable again.
ALTER TABLE addons ADD COLUMN locked_at TIMESTAMPTZ;
ALTER TABLE addons ADD COLUMN locked_reason TEXT;

CREATE TABLE versions (
    id                    BIGSERIAL PRIMARY KEY,
    addon_id              BIGINT      NOT NULL REFERENCES addons (id) ON DELETE CASCADE,
    -- NULL for tag addons.
    github_release_id     BIGINT,
    tag                   TEXT        NOT NULL,
    name                  TEXT        NOT NULL DEFAULT '',
    prerelease            BOOLEAN     NOT NULL DEFAULT false,
    published_at          TIMESTAMPTZ NOT NULL,
    asset_id              BIGINT,
    asset_name            TEXT        NOT NULL,
    asset_url             TEXT        NOT NULL,
    asset_size            BIGINT,
    -- Hex SHA-256 GitHub reports for the asset, if it does.
    github_digest         TEXT,
    -- Hex SHA-256 the worker computed when it scanned the file.
    sha256                TEXT,
    download_count        BIGINT      NOT NULL DEFAULT 0,
    scan_status           TEXT        NOT NULL DEFAULT 'pending' CHECK (scan_status IN ('pending', 'running', 'failed', 'done')),
    source_status         TEXT        NOT NULL DEFAULT 'available' CHECK (source_status IN ('available', 'missing')),
    deleted_on_github_at  TIMESTAMPTZ,
    created_at            TIMESTAMPTZ NOT NULL,
    UNIQUE (addon_id, tag)
);

CREATE TABLE webhook_deliveries (
    delivery_id  TEXT        PRIMARY KEY,
    event        TEXT        NOT NULL,
    received_at  TIMESTAMPTZ NOT NULL
);
