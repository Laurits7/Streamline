-- Phase 2b: views and focus (SPEC §6.7, §6.9).

-- "In progress" = an open task with started_at set (kanban). Kept as a timestamp rather
-- than a new status value, so the tasks table (and its CHECK) needn't be rebuilt.
ALTER TABLE tasks ADD COLUMN started_at TEXT;

-- Per-user UI preferences as a JSON object (e.g. each view's filters), so they follow
-- the user across devices.
ALTER TABLE users ADD COLUMN prefs TEXT NOT NULL DEFAULT '{}';

-- Pomodoro settings (D-20: 25/5/15, long break after 4).
ALTER TABLE users ADD COLUMN focus_work_min INTEGER NOT NULL DEFAULT 25;
ALTER TABLE users ADD COLUMN focus_short_break_min INTEGER NOT NULL DEFAULT 5;
ALTER TABLE users ADD COLUMN focus_long_break_min INTEGER NOT NULL DEFAULT 15;
ALTER TABLE users ADD COLUMN focus_long_every INTEGER NOT NULL DEFAULT 4;

-- The current focus timer of each user. Lives on the server so every device shows the
-- same countdown: remaining = length - (elapsed_ms + (now - running_since)).
CREATE TABLE focus_timers (
    user_id       TEXT PRIMARY KEY REFERENCES users(id),
    task_id       TEXT REFERENCES tasks(id),
    phase         TEXT NOT NULL CHECK (phase IN ('idle', 'work', 'short_break', 'long_break')),
    running_since TEXT,
    elapsed_ms    INTEGER NOT NULL DEFAULT 0,
    length_min    INTEGER NOT NULL DEFAULT 0,
    cycle_done    INTEGER NOT NULL DEFAULT 0,
    updated_at    TEXT NOT NULL,
    rev           INTEGER NOT NULL
);

-- Finished focus/break intervals, for actual time spent and the daily summary.
CREATE TABLE focus_sessions (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id),
    task_id    TEXT REFERENCES tasks(id),
    kind       TEXT NOT NULL CHECK (kind IN ('work', 'break')),
    started_at TEXT NOT NULL,
    ended_at   TEXT NOT NULL,
    minutes    INTEGER NOT NULL,
    completed  INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    rev        INTEGER NOT NULL
);
CREATE INDEX focus_sessions_user ON focus_sessions(user_id, started_at);
CREATE INDEX focus_sessions_rev ON focus_sessions(rev);
