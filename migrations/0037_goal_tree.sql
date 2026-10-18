-- Goal tree, attachments and rules (D-112): a goal can sit under another goal (any number
-- of top-level goals; a child shares its parent's owner), routines can be attached to it
-- like projects and tasks, and a third kind, `rule` ("eat less junk"), is kept over a
-- period without a daily question or progress.
ALTER TABLE goals ADD COLUMN parent_id TEXT REFERENCES goals(id);
ALTER TABLE goals ADD COLUMN series_ids TEXT NOT NULL DEFAULT '[]';
CREATE INDEX goals_parent ON goals(parent_id);
