-- Phase 3b: prerequisites and workflows (SPEC §6.3b).
-- depends_on: JSON array of task ids that must be finished first. `blocked` caches
-- whether any of them is still open (kept up to date on every change, so the ready
-- stack stays a cheap query). After the last prerequisite is done, a task can wait a
-- further `wait_min` minutes ("the machine runs ~1 h"); `ready_at` is when that ends.
ALTER TABLE tasks ADD COLUMN depends_on TEXT NOT NULL DEFAULT '[]';
ALTER TABLE tasks ADD COLUMN blocked INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN wait_min INTEGER;
ALTER TABLE tasks ADD COLUMN ready_at TEXT;
-- Steps of a running workflow: which run, which step (1-based) of how many.
ALTER TABLE tasks ADD COLUMN workflow_instance_id TEXT;
ALTER TABLE tasks ADD COLUMN workflow_step INTEGER;
ALTER TABLE tasks ADD COLUMN workflow_steps INTEGER;
CREATE INDEX tasks_workflow ON tasks(workflow_instance_id);
