-- Daily planning ritual (SPEC §6.2d).
-- plan_mode: when the user plans (evening = plan tomorrow, morning = plan today, or both).
-- day_window_*: the part of the day counted as available time ("free time").
ALTER TABLE users ADD COLUMN plan_mode TEXT NOT NULL DEFAULT 'evening' CHECK (plan_mode IN ('evening', 'morning', 'both'));
ALTER TABLE users ADD COLUMN plan_time_evening TEXT NOT NULL DEFAULT '21:00';
ALTER TABLE users ADD COLUMN plan_time_morning TEXT NOT NULL DEFAULT '07:30';
ALTER TABLE users ADD COLUMN day_window_start TEXT NOT NULL DEFAULT '08:00';
ALTER TABLE users ADD COLUMN day_window_end TEXT NOT NULL DEFAULT '22:00';

-- One row per user and day once planning has started. 'draft' = wizard in progress
-- (step remembers where), 'planned' = confirmed. No row (or deleted) = unplanned.
CREATE TABLE day_plans (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id),
    date       TEXT NOT NULL,
    status     TEXT NOT NULL CHECK (status IN ('draft', 'planned')),
    step       INTEGER NOT NULL DEFAULT 0,
    planned_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    rev        INTEGER NOT NULL
);
CREATE UNIQUE INDEX day_plans_user_date ON day_plans(user_id, date) WHERE deleted_at IS NULL;
CREATE INDEX day_plans_rev ON day_plans(rev);

-- Reminders already sent, so each fires at most once per kind and target day.
CREATE TABLE reminder_log (
    user_id TEXT NOT NULL REFERENCES users(id),
    kind    TEXT NOT NULL,
    date    TEXT NOT NULL,
    sent_at TEXT NOT NULL,
    PRIMARY KEY (user_id, kind, date)
);
