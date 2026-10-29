-- Categories get one level of subcategories (Classes -> Warrior, ...). Addons sit in a category
-- without subcategories; filtering by a category also finds the addons of its subcategories.
ALTER TABLE categories ADD COLUMN parent CITEXT REFERENCES categories (name) ON UPDATE CASCADE;

INSERT INTO categories (name, sort_key, parent) VALUES
    ('Classes', 10, NULL),
    ('Combat', 20, NULL),
    ('Libraries', 30, NULL),
    ('Unit Frames', 40, NULL),
    ('Action Bars', 50, NULL),
    ('Miscellaneous', 60, NULL),
    ('Warrior', 1, 'Classes'),
    ('Paladin', 2, 'Classes'),
    ('Hunter', 3, 'Classes'),
    ('Rogue', 4, 'Classes'),
    ('Priest', 5, 'Classes'),
    ('Shaman', 6, 'Classes'),
    ('Mage', 7, 'Classes'),
    ('Warlock', 8, 'Classes'),
    ('Druid', 9, 'Classes')
ON CONFLICT (name) DO NOTHING;

-- The verified addons by what they do, then everything else by its old category.
UPDATE addons SET category = 'Combat' WHERE slug IN ('bordoui', 'blackwellui', 'azsspellmeter', 'azerothfieldkit');
UPDATE addons SET category = 'Action Bars' WHERE slug = 'survivorsdragonui';
UPDATE addons SET category = 'Unit Frames' WHERE category = 'UI';
UPDATE addons SET category = 'Combat' WHERE category = 'Meter';
UPDATE addons SET category = 'Miscellaneous' WHERE category IN ('Info', 'Build', 'Loot');
DELETE FROM categories WHERE name IN ('UI', 'Info', 'Build', 'Loot', 'Meter');
