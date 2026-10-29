CREATE TABLE jobs (
    id            BIGSERIAL PRIMARY KEY,
    kind          TEXT        NOT NULL,
    payload       JSONB       NOT NULL DEFAULT '{}',
    run_at        TIMESTAMPTZ NOT NULL,
    attempts      INT         NOT NULL DEFAULT 0,
    max_attempts  INT         NOT NULL DEFAULT 5,
    locked_until  TIMESTAMPTZ,
    last_error    TEXT,
    done_at       TIMESTAMPTZ,
    failed_at     TIMESTAMPTZ,
    -- At most one open job per key, e.g. one pending scan per version.
    dedupe_key    TEXT,
    -- One run per periodic job and time slot, across all workers and forever.
    periodic_key  TEXT UNIQUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX jobs_open_dedupe ON jobs (dedupe_key) WHERE done_at IS NULL AND failed_at IS NULL;
CREATE INDEX jobs_runnable ON jobs (run_at) WHERE done_at IS NULL AND failed_at IS NULL;
