-- Nested projects: a project may live inside another project (any depth).
ALTER TABLE projects ADD COLUMN parent_id TEXT REFERENCES projects(id);
CREATE INDEX projects_parent ON projects(parent_id);
