ALTER TABLE versions ADD COLUMN source_checked_at TIMESTAMPTZ;

CREATE INDEX versions_source_check ON versions (source_checked_at NULLS FIRST) WHERE sha256 IS NOT NULL;
