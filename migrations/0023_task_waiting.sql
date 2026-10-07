-- Waiting for results (D-70): an open task whose work is done or handed off, with an
-- optional time to check back. While `waiting_since` is set the task needs no action;
-- when the check-back time passes, `waiting_since` is cleared and `check_back_at` stays
-- as the "check back now" marker until the task is completed or changed.
ALTER TABLE tasks ADD COLUMN waiting_since TEXT;
ALTER TABLE tasks ADD COLUMN check_back_at TEXT;
ALTER TABLE tasks ADD COLUMN waiting_note TEXT NOT NULL DEFAULT '';
ALTER TABLE tasks ADD COLUMN waiting_by TEXT;
CREATE INDEX tasks_check_back ON tasks(check_back_at) WHERE waiting_since IS NOT NULL;
