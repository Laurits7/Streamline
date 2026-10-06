-- Workflow templates (SPEC §6.3b): reusable multi-step chores. Steps run as a chain;
-- variants skip some steps (delicates: no dryer; towels: no ironing). Steps and
-- variants are small JSON arrays stored with the template.
--   steps:    [{"id","title","estimate_min"?,"wait_min"?,"difficulty"?}]   in order;
--             wait_min = time to wait after this step before the next is ready
--   variants: [{"id","name","skip":[step ids]}]
CREATE TABLE workflow_templates (
    id             TEXT PRIMARY KEY,
    owner_user_id  TEXT REFERENCES users(id),
    owner_group_id TEXT,
    name           TEXT NOT NULL,
    description    TEXT NOT NULL DEFAULT '',
    project_id     TEXT REFERENCES projects(id),
    steps          TEXT NOT NULL DEFAULT '[]',
    variants       TEXT NOT NULL DEFAULT '[]',
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    deleted_at     TEXT,
    rev            INTEGER NOT NULL,
    CHECK ((owner_user_id IS NULL) <> (owner_group_id IS NULL))
);
CREATE INDEX workflow_templates_owner ON workflow_templates(owner_user_id);
CREATE INDEX workflow_templates_rev ON workflow_templates(rev);
-- Routines can start a workflow on each occurrence (3b.6).
ALTER TABLE series ADD COLUMN workflow_template_id TEXT REFERENCES workflow_templates(id);
ALTER TABLE series ADD COLUMN workflow_variant_ids TEXT NOT NULL DEFAULT '[]';
