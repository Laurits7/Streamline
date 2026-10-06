-- A task can also be listed in other projects besides its main one (owner request).
-- JSON array of project ids; the main project stays tasks.project_id.
ALTER TABLE tasks ADD COLUMN also_project_ids TEXT NOT NULL DEFAULT '[]';
