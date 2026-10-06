-- Time blocks and day templates (SPEC §6.8, Phase 5a).
-- A template is a reusable block layout ("Workday", "Weekend"), assigned to weekdays:
--   blocks: [{"title": "Deep work", "start": "08:00", "end": "12:00", "energy": "hard"|"medium"|"easy"|null}]
CREATE TABLE day_templates (
    id             TEXT PRIMARY KEY,
    owner_user_id  TEXT NOT NULL REFERENCES users(id),
    name           TEXT NOT NULL,
    weekdays       TEXT NOT NULL DEFAULT '[]',   -- ISO weekdays 1 (Mon) … 7 (Sun)
    blocks         TEXT NOT NULL DEFAULT '[]',
    position       TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    deleted_at     TEXT,
    rev            INTEGER NOT NULL
);
CREATE INDEX day_templates_owner ON day_templates(owner_user_id, rev);

-- The blocks of one day (personal, like day plans).
CREATE TABLE time_blocks (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id),
    date         TEXT NOT NULL,
    title        TEXT NOT NULL,
    start_time   TEXT NOT NULL,
    end_time     TEXT NOT NULL,
    energy       TEXT CHECK (energy IN ('hard', 'medium', 'easy')),
    template_id  TEXT,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    deleted_at   TEXT,
    rev          INTEGER NOT NULL
);
CREATE INDEX time_blocks_day ON time_blocks(user_id, date);
CREATE INDEX time_blocks_rev ON time_blocks(user_id, rev);

-- How each day's blocks were set up. Days set up automatically follow their weekday
-- template (re-applied when it changes, by `template_rev`); once the user changes a
-- day's blocks by hand (`manual`), it's left alone.
CREATE TABLE block_days (
    user_id       TEXT NOT NULL,
    date          TEXT NOT NULL,
    manual        INTEGER NOT NULL DEFAULT 0,
    template_id   TEXT,
    template_rev  INTEGER,
    PRIMARY KEY (user_id, date)
);

-- All-day events of this calendar block the whole day (conflicts and free time).
ALTER TABLE calendars ADD COLUMN all_day_busy INTEGER NOT NULL DEFAULT 0;
