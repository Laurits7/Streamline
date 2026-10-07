-- Checklists inside a task, and on routines (each occurrence starts unticked) (D-71).
-- JSON array of {id, text, done}.
ALTER TABLE tasks ADD COLUMN checklist TEXT NOT NULL DEFAULT '[]';
ALTER TABLE series ADD COLUMN checklist TEXT NOT NULL DEFAULT '[]';
