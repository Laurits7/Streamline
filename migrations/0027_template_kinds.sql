-- Kinds of a default task (D-86): Laundry → linen, colours, whites; each with its own
-- steps (or the default task's). Picking kinds adds one task per kind.
ALTER TABLE task_templates ADD COLUMN kinds TEXT NOT NULL DEFAULT '[]';
