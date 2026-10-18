-- Tracking page (D-108): each metric gets an emoji and quick amounts (`presets`: a JSON
-- array of at most 6 numbers, each logged as one entry with a tap, e.g. water +200 ml).
-- The built-in water (key 'water') is created like mood and weight on the next sync.
ALTER TABLE metric_definitions ADD COLUMN emoji TEXT NOT NULL DEFAULT '';
ALTER TABLE metric_definitions ADD COLUMN presets TEXT NOT NULL DEFAULT '[]';

-- The existing built-ins get their emoji. Every metric gets a new rev, so clients that
-- sync incrementally (and the desktop app's local copy) receive the new fields.
UPDATE meta_rev SET rev = rev + 1 WHERE id = 1;
UPDATE metric_definitions SET
    emoji = CASE key WHEN 'mood' THEN '🙂' WHEN 'weight' THEN '⚖️' ELSE emoji END,
    rev = (SELECT rev FROM meta_rev WHERE id = 1);
