-- Imported addons start without an owner and without a GitHub App installation. Their author
-- claims them later by proving admin rights on the repo.
ALTER TABLE addons ALTER COLUMN owner_id DROP NOT NULL;
ALTER TABLE addons ALTER COLUMN installation_id DROP NOT NULL;
-- The author's name as the registry knew it, shown while nobody owns the addon.
ALTER TABLE addons ADD COLUMN author_name TEXT;
