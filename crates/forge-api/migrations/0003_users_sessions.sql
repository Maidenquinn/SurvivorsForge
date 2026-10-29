CREATE TABLE users (
    id             BIGSERIAL PRIMARY KEY,
    github_id      BIGINT      NOT NULL UNIQUE,
    login          CITEXT      NOT NULL,
    avatar_url     TEXT        NOT NULL DEFAULT '',
    role           TEXT        NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'admin')),
    banned_at      TIMESTAMPTZ,
    ban_reason     TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- NULL for users that were added as contributors but never logged in.
    last_login_at  TIMESTAMPTZ
);

-- Only hashes of tokens and codes are stored.
CREATE TABLE sessions (
    id          BIGSERIAL PRIMARY KEY,
    user_id     BIGINT      NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash  BYTEA       NOT NULL UNIQUE,
    created_at  TIMESTAMPTZ NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL
);

CREATE TABLE login_codes (
    code_hash   BYTEA       PRIMARY KEY,
    user_id     BIGINT      NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    return_to   TEXT        NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL
);

CREATE TABLE oauth_states (
    state_hash  BYTEA       PRIMARY KEY,
    verifier    TEXT        NOT NULL,
    return_to   TEXT        NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL
);
