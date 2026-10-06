-- When a routine's schedule changes, the new version links to the one it replaced, so
-- progress and streaks continue across the change (and a mid-week change of "N times a
-- week" only tops up what's missing this week).
ALTER TABLE series ADD COLUMN split_from TEXT REFERENCES series(id);
