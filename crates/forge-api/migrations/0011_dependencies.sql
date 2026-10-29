CREATE TABLE addon_dependencies (
    addon_id       BIGINT  NOT NULL REFERENCES addons (id) ON DELETE CASCADE,
    depends_on_id  BIGINT  NOT NULL REFERENCES addons (id) ON DELETE CASCADE,
    required       BOOLEAN NOT NULL,
    PRIMARY KEY (addon_id, depends_on_id),
    CHECK (addon_id <> depends_on_id)
);
