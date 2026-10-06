-- Per-user display preferences. locale '' = use the browser's language.
-- week_start is ISO weekday: 1 = Monday ... 7 = Sunday.
ALTER TABLE users ADD COLUMN locale TEXT NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN week_start INTEGER NOT NULL DEFAULT 1 CHECK (week_start BETWEEN 1 AND 7);
