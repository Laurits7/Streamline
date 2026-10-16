-- Habit goals (D-105): a goal is either an outcome (as before: target date, milestones)
-- or a habit kept over a period ("eat less junk"), from `start_date` until `target_date`
-- (or for good), answered per day in the reflection and judged per week in the review.
ALTER TABLE goals ADD COLUMN kind TEXT NOT NULL DEFAULT 'outcome';
ALTER TABLE goals ADD COLUMN start_date TEXT;
-- The weekly verdict per habit (yes | partly | no), next to the review note.
ALTER TABLE goal_reviews ADD COLUMN verdict TEXT;
-- A day's answer for a habit. Personal: each member of a shared habit answers for themselves.
CREATE TABLE habit_checks (
    id          TEXT PRIMARY KEY,
    goal_id     TEXT NOT NULL REFERENCES goals(id),
    user_id     TEXT NOT NULL REFERENCES users(id),
    date        TEXT NOT NULL,
    result      TEXT NOT NULL CHECK (result IN ('yes', 'partly', 'no')),
    note        TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    deleted_at  TEXT,
    rev         INTEGER NOT NULL
);
CREATE UNIQUE INDEX habit_checks_day ON habit_checks(goal_id, user_id, date);
CREATE INDEX habit_checks_user_rev ON habit_checks(user_id, rev);
