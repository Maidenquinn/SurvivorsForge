CREATE TABLE categories (
    name        CITEXT PRIMARY KEY,
    sort_key    INT    NOT NULL DEFAULT 0
);

INSERT INTO categories (name, sort_key) VALUES ('UI', 1), ('Info', 2), ('Build', 3), ('Loot', 4), ('Meter', 5);

CREATE TABLE addons (
    id               BIGSERIAL PRIMARY KEY,
    slug             CITEXT      NOT NULL,
    name             TEXT        NOT NULL,
    owner_id         BIGINT      NOT NULL REFERENCES users (id),
    repo_id          BIGINT      NOT NULL,
    -- Display only; GitHub identifies the repo by repo_id, which survives renames and transfers.
    repo_full_name   TEXT        NOT NULL,
    installation_id  BIGINT      NOT NULL,
    source_type      TEXT        NOT NULL CHECK (source_type IN ('release', 'tag')),
    asset_pattern    TEXT,
    category         CITEXT      NOT NULL REFERENCES categories (name) ON UPDATE CASCADE,
    summary          TEXT        NOT NULL DEFAULT '',
    description_md   TEXT        NOT NULL DEFAULT '',
    created_at       TIMESTAMPTZ NOT NULL,
    updated_at       TIMESTAMPTZ NOT NULL,
    deleted_at       TIMESTAMPTZ,
    blocked_at       TIMESTAMPTZ,
    CHECK (source_type = 'tag' OR asset_pattern IS NOT NULL)
);

-- Deleting an addon frees its slug and its repo.
CREATE UNIQUE INDEX addons_slug ON addons (slug) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX addons_repo ON addons (repo_id) WHERE deleted_at IS NULL;

CREATE TABLE addon_folders (
    addon_id  BIGINT  NOT NULL REFERENCES addons (id) ON DELETE CASCADE,
    folder    CITEXT  NOT NULL,
    -- false once the addon is deleted, which frees the folder name.
    active    BOOLEAN NOT NULL DEFAULT true,
    PRIMARY KEY (addon_id, folder)
);

-- A folder name belongs to one addon across the whole platform.
CREATE UNIQUE INDEX addon_folders_owner ON addon_folders (folder) WHERE active;
