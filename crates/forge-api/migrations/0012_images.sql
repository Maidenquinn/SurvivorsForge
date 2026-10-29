CREATE TABLE images (
    id           BIGSERIAL PRIMARY KEY,
    addon_id     BIGINT      NOT NULL REFERENCES addons (id) ON DELETE CASCADE,
    kind         TEXT        NOT NULL CHECK (kind IN ('screenshot', 'icon')),
    -- <sha256 of the WebP>.webp in IMAGE_DIR; equal images share one file.
    file         TEXT        NOT NULL,
    width        INT         NOT NULL,
    height       INT         NOT NULL,
    position     INT         NOT NULL DEFAULT 0,
    caption      TEXT        NOT NULL DEFAULT '',
    uploaded_by  BIGINT      NOT NULL REFERENCES users (id),
    uploaded_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX images_addon ON images (addon_id, kind, position);
CREATE UNIQUE INDEX images_one_icon ON images (addon_id) WHERE kind = 'icon';
