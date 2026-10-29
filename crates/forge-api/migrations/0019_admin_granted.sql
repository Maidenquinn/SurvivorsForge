-- Admin rights granted on the website. A user is admin when this is set or their GitHub ID is in
-- ADMIN_GITHUB_IDS; the second follows the configuration on every login, the first stays.
ALTER TABLE users ADD COLUMN admin_granted BOOLEAN NOT NULL DEFAULT false;
