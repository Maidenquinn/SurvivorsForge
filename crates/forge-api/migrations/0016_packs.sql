-- Addon packs: a fixed list of addon versions behind a random link, for players of the browser
-- version who upload one zip with all their addons.
CREATE TABLE packs (
    id           BIGSERIAL   PRIMARY KEY,
    -- The random part of the link.
    public_id    TEXT        NOT NULL UNIQUE,
    name         TEXT        NOT NULL DEFAULT '',
    created_by   BIGINT      REFERENCES users (id) ON DELETE SET NULL,
    -- SHA-256 of the sorted version ids: packs with the same versions share one file.
    content_key  TEXT        NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL
);

CREATE INDEX packs_created_by ON packs (created_by, created_at DESC);

CREATE TABLE pack_versions (
    pack_id     BIGINT NOT NULL REFERENCES packs (id) ON DELETE CASCADE,
    version_id  BIGINT NOT NULL REFERENCES versions (id) ON DELETE CASCADE,
    PRIMARY KEY (pack_id, version_id)
);

-- The built zip of a content key. The row goes when the file is deleted; the next download
-- builds it again.
CREATE TABLE pack_files (
    content_key       TEXT        PRIMARY KEY,
    status            TEXT        NOT NULL CHECK (status IN ('building', 'ready', 'failed')),
    size              BIGINT,
    error             TEXT,
    requested_at      TIMESTAMPTZ NOT NULL,
    built_at          TIMESTAMPTZ,
    last_download_at  TIMESTAMPTZ
);
