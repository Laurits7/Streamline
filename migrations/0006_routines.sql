-- Phase 3: recurring tasks and routines (SPEC §6.3, D-1, D-3).

-- The remaining built-in task types (SPEC §6.2c).
INSERT INTO task_types (id, key, name, day_end_behavior, counts_for_streak, shows_overdue, shows_carry_count, window_overflow, builtin, created_at, updated_at, rev) VALUES
  ('tt_deadline', 'deadline', 'Deadline',             'deadline', 0, 1, 0, 'miss', 1, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z', 0),
  ('tt_window',   'window',   'Within its week/month', 'window',  1, 0, 0, 'miss', 1, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z', 0);
-- Daily-only routines count towards streaks.
UPDATE task_types SET counts_for_streak = 1 WHERE id = 'tt_expires';

-- A routine: a template that spawns one task per occurrence.
--   repeat:   on the dates of `rrule` (e.g. pay rent monthly); due that day
--   anchored: on the dates of `rrule`, at `start_time` (planned on the day's timeline)
--   flexible: `times_per_window` per `window` ('week' or 'month'), any day within it
CREATE TABLE series (
    id               TEXT PRIMARY KEY,
    owner_user_id    TEXT REFERENCES users(id),
    owner_group_id   TEXT,
    project_id       TEXT REFERENCES projects(id),
    title            TEXT NOT NULL,
    notes            TEXT NOT NULL DEFAULT '',
    mode             TEXT NOT NULL CHECK (mode IN ('repeat', 'anchored', 'flexible')),
    rrule            TEXT,
    dtstart          TEXT NOT NULL,
    until            TEXT,
    start_time       TEXT,
    duration_min     INTEGER,
    times_per_window INTEGER,
    window           TEXT CHECK (window IN ('week', 'month')),
    task_type_id     TEXT NOT NULL REFERENCES task_types(id),
    estimate_min     INTEGER,
    difficulty       INTEGER CHECK (difficulty BETWEEN 1 AND 3),
    importance       INTEGER CHECK (importance BETWEEN 0 AND 3),
    urgency          INTEGER CHECK (urgency BETWEEN 0 AND 3),
    -- Occurrences have been created up to and including this date.
    materialized_through TEXT,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    deleted_at       TEXT,
    rev              INTEGER NOT NULL,
    CHECK ((owner_user_id IS NULL) <> (owner_group_id IS NULL))
);
CREATE INDEX series_owner ON series(owner_user_id);
CREATE INDEX series_rev ON series(rev);

-- Occurrence tasks point back to their routine. The key (date, or window start + "#n")
-- is unique per series *including deleted rows*: a skipped/deleted occurrence never
-- comes back.
ALTER TABLE tasks ADD COLUMN series_id TEXT REFERENCES series(id);
ALTER TABLE tasks ADD COLUMN occurrence_key TEXT;
ALTER TABLE tasks ADD COLUMN occurrence_date TEXT;
ALTER TABLE tasks ADD COLUMN window_end TEXT;
CREATE UNIQUE INDEX tasks_occurrence ON tasks(series_id, occurrence_key) WHERE series_id IS NOT NULL;
CREATE INDEX tasks_series ON tasks(series_id, occurrence_date);
