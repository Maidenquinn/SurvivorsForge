-- Versions of the game itself, e.g. 0.69.1. Admins keep the list; sort_key orders it, since names
-- are free text and not always semver.
CREATE TABLE game_versions (
    id           SERIAL PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE,
    released_at  DATE,
    sort_key     INT  NOT NULL
);

-- "works from game_min on", optionally "up to game_max". Both NULL: unknown.
ALTER TABLE versions ADD COLUMN game_min_id INT REFERENCES game_versions (id);
ALTER TABLE versions ADD COLUMN game_max_id INT REFERENCES game_versions (id);

-- NULL when nothing is known, else whether the addon version runs on the game version.
CREATE FUNCTION game_compatible(p_version BIGINT, p_game INT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
    SELECT CASE
               WHEN v.game_min_id IS NULL AND v.game_max_id IS NULL THEN NULL
               ELSE coalesce(lo.sort_key <= g.sort_key, true) AND coalesce(g.sort_key <= hi.sort_key, true)
           END
    FROM versions v
    JOIN game_versions g ON g.id = p_game
    LEFT JOIN game_versions lo ON lo.id = v.game_min_id
    LEFT JOIN game_versions hi ON hi.id = v.game_max_id
    WHERE v.id = p_version
$$;
