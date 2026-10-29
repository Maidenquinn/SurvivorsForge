-- Name before summary before description. 'simple': addon names are proper nouns and texts mix languages.
ALTER TABLE addons ADD COLUMN search TSVECTOR GENERATED ALWAYS AS (
    setweight(to_tsvector('simple', name), 'A') ||
    setweight(to_tsvector('simple', summary), 'B') ||
    setweight(to_tsvector('simple', description_md), 'C')
) STORED;

CREATE INDEX addons_search ON addons USING GIN (search);
CREATE INDEX addons_name_trgm ON addons USING GIN (name gin_trgm_ops);
