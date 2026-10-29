-- Packs are reached by their link; listed ones also show on the public pack list.
ALTER TABLE packs ADD COLUMN listed BOOLEAN NOT NULL DEFAULT false;
CREATE INDEX packs_listed ON packs (created_at DESC) WHERE listed;
