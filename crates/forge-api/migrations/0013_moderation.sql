ALTER TABLE addons ADD COLUMN block_reason TEXT;
-- Blocked because the owner was banned; lifting the ban lifts only these blocks.
ALTER TABLE addons ADD COLUMN blocked_by_ban BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE versions ADD COLUMN block_reason TEXT;

CREATE TABLE reports (
    id           BIGSERIAL PRIMARY KEY,
    reporter_id  BIGINT      NOT NULL REFERENCES users (id),
    target_type  TEXT        NOT NULL CHECK (target_type IN ('addon', 'version', 'image')),
    target_id    BIGINT      NOT NULL,
    reason       TEXT        NOT NULL CHECK (reason IN ('malware', 'stolen', 'broken', 'inappropriate', 'spam', 'other')),
    text         TEXT        NOT NULL DEFAULT '',
    status       TEXT        NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'dismissed', 'actioned')),
    handled_by   BIGINT      REFERENCES users (id),
    handled_at   TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL
);

CREATE UNIQUE INDEX reports_one_open ON reports (reporter_id, target_type, target_id) WHERE status = 'open';
CREATE INDEX reports_queue ON reports (created_at) WHERE status = 'open';
