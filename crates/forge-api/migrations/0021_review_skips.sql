-- Admins take a version off the review queue without verifying it. The skip holds while the version
-- keeps the trust it had then; a change (new findings, a changed file) puts it back on the queue.
CREATE TABLE review_skips (
    version_id  BIGINT      PRIMARY KEY REFERENCES versions (id) ON DELETE CASCADE,
    trust       TEXT        NOT NULL,
    admin_id    BIGINT      NOT NULL REFERENCES users (id),
    note        TEXT        NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL
);
