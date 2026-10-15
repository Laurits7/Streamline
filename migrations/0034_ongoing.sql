-- Ongoing tasks (D-102): kept at over a period ("keep reading Dune") rather than done in
-- one go. They're marked "worked on it today" (the days are kept) and finished by hand;
-- the period runs from `start_date` (or creation) to `due_date` (or until finished).
ALTER TABLE tasks ADD COLUMN ongoing INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN start_date TEXT;
ALTER TABLE tasks ADD COLUMN worked_days TEXT NOT NULL DEFAULT '[]';
