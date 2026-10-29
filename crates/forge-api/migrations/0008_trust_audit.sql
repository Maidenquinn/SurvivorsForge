ALTER TABLE versions ADD COLUMN blocked_at TIMESTAMPTZ;
-- Set when the file on GitHub no longer matches the scanned hash (see the source check).
ALTER TABLE versions ADD COLUMN source_changed_at TIMESTAMPTZ;

-- An admin's approval of exactly one file: it only counts while versions.sha256 still matches.
CREATE TABLE verifications (
    version_id  BIGINT      PRIMARY KEY REFERENCES versions (id) ON DELETE CASCADE,
    sha256      TEXT        NOT NULL,
    admin_id    BIGINT      NOT NULL REFERENCES users (id),
    note        TEXT        NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL
);

-- The one place the trust level of a version is decided. Priority, highest first:
-- blocked > source_changed > verified > flagged > scanned > unscanned.
CREATE VIEW version_trust AS
SELECT v.id AS version_id,
       v.addon_id,
       CASE
           WHEN v.blocked_at IS NOT NULL OR a.blocked_at IS NOT NULL THEN 'blocked'
           WHEN v.source_changed_at IS NOT NULL THEN 'source_changed'
           WHEN w.version_id IS NOT NULL THEN 'verified'
           WHEN v.scan_status = 'done' AND s.flagged THEN 'flagged'
           WHEN v.scan_status = 'done' AND s.id IS NOT NULL THEN 'scanned'
           ELSE 'unscanned'
       END AS trust,
       coalesce(s.critical, 0)::int AS critical,
       coalesce(s.warn, 0)::int AS warn,
       coalesce(s.info, 0)::int AS info
FROM versions v
JOIN addons a ON a.id = v.addon_id
LEFT JOIN verifications w ON w.version_id = v.id AND w.sha256 = v.sha256
LEFT JOIN LATERAL (
    SELECT s.id,
           count(f.id) FILTER (WHERE f.severity = 'critical') AS critical,
           count(f.id) FILTER (WHERE f.severity = 'warn') AS warn,
           count(f.id) FILTER (WHERE f.severity = 'info') AS info,
           coalesce(bool_or(f.severity IN ('warn', 'critical')), false) AS flagged
    FROM scans s LEFT JOIN findings f ON f.scan_id = s.id
    WHERE s.version_id = v.id
    GROUP BY s.id
    ORDER BY s.id DESC
    LIMIT 1
) s ON true;

-- The version an addon's trust is shown for: the newest release still on GitHub, stable before
-- pre-releases; the newest of all when every version is gone.
CREATE FUNCTION relevant_version(p_addon BIGINT) RETURNS BIGINT LANGUAGE sql STABLE AS $$
    SELECT id FROM versions WHERE addon_id = p_addon
    ORDER BY deleted_on_github_at IS NOT NULL, prerelease, published_at DESC, id DESC
    LIMIT 1
$$;

CREATE TABLE audit_log (
    id           BIGSERIAL PRIMARY KEY,
    admin_id     BIGINT      NOT NULL REFERENCES users (id),
    action       TEXT        NOT NULL,
    target_type  TEXT        NOT NULL,
    target_id    TEXT        NOT NULL,
    note         TEXT        NOT NULL DEFAULT '',
    details      JSONB       NOT NULL DEFAULT '{}',
    created_at   TIMESTAMPTZ NOT NULL
);

CREATE INDEX audit_log_created ON audit_log (created_at DESC);

CREATE FUNCTION audit_log_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'audit_log is append-only';
END
$$;

CREATE TRIGGER audit_log_append_only BEFORE UPDATE OR DELETE OR TRUNCATE ON audit_log
    FOR EACH STATEMENT EXECUTE FUNCTION audit_log_append_only();
