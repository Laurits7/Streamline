-- Occasions (SPEC §6.17, D-57): an instance-wide nameday calendar (loaded from the
-- official list or uploaded by an admin, never bundled), each user's people of interest,
-- and per-user templates for what each kind of occasion creates.
CREATE TABLE namedays (
    month     INTEGER NOT NULL,
    day       INTEGER NOT NULL,
    name      TEXT NOT NULL,
    name_key  TEXT NOT NULL,          -- folded for search (lowercase, no accents)
    PRIMARY KEY (month, day, name)
);
CREATE INDEX namedays_key ON namedays(name_key);

-- Where the nameday calendar came from (one row).
CREATE TABLE nameday_source (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    label       TEXT NOT NULL,
    loaded_at   TEXT NOT NULL,
    count       INTEGER NOT NULL
);

CREATE TABLE people (
    id             TEXT PRIMARY KEY,
    owner_user_id  TEXT NOT NULL REFERENCES users(id),
    name           TEXT NOT NULL,         -- how tasks address them, e.g. "Mari (sister)"
    nameday_name   TEXT,                  -- name in the nameday calendar
    birthday       TEXT,                  -- YYYY-MM-DD, or --MM-DD without a year
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    deleted_at     TEXT,
    rev            INTEGER NOT NULL
);
CREATE INDEX people_owner ON people(owner_user_id, rev);

-- What an occasion creates, per user and kind ('nameday' | 'birthday'):
--   steps: [{"title": "Buy a present for {name}", "offset_days": -2, "task_type_id": "tt_deadline", "after_previous": false}]
CREATE TABLE occasion_templates (
    id             TEXT PRIMARY KEY,
    owner_user_id  TEXT NOT NULL REFERENCES users(id),
    kind           TEXT NOT NULL CHECK (kind IN ('nameday', 'birthday')),
    enabled        INTEGER NOT NULL DEFAULT 1,
    steps          TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    rev            INTEGER NOT NULL,
    UNIQUE (owner_user_id, kind)
);
