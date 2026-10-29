CREATE TABLE addon_members (
    addon_id              BIGINT      NOT NULL REFERENCES addons (id) ON DELETE CASCADE,
    user_id               BIGINT      NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    can_edit_description  BOOLEAN     NOT NULL DEFAULT false,
    can_edit_images       BOOLEAN     NOT NULL DEFAULT false,
    added_by              BIGINT      NOT NULL REFERENCES users (id),
    added_at              TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (addon_id, user_id)
);
