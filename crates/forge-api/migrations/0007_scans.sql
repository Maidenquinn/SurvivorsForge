CREATE TABLE scans (
    id               BIGSERIAL PRIMARY KEY,
    version_id       BIGINT      NOT NULL REFERENCES versions (id) ON DELETE CASCADE,
    -- Hex SHA-256 of the file that was scanned.
    sha256           TEXT,
    scanner_version  TEXT        NOT NULL,
    started_at       TIMESTAMPTZ NOT NULL,
    finished_at      TIMESTAMPTZ NOT NULL,
    result           TEXT        NOT NULL CHECK (result IN ('clean', 'flagged', 'error')),
    error            TEXT
);

CREATE INDEX scans_version ON scans (version_id, id DESC);

CREATE TABLE findings (
    id        BIGSERIAL PRIMARY KEY,
    scan_id   BIGINT NOT NULL REFERENCES scans (id) ON DELETE CASCADE,
    rule      TEXT   NOT NULL,
    severity  TEXT   NOT NULL CHECK (severity IN ('info', 'warn', 'critical')),
    file      TEXT   NOT NULL,
    line      INT,
    message   TEXT   NOT NULL,
    snippet   TEXT
);

CREATE INDEX findings_scan ON findings (scan_id);
