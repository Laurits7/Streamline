-- Default tasks (D-71): ready-made tasks such as "Do laundry" with its checklist,
-- personal or shared with a group. Creating a task copies the fields.
CREATE TABLE task_templates (
    id             TEXT PRIMARY KEY,
    owner_user_id  TEXT REFERENCES users(id),
    owner_group_id TEXT,
    title          TEXT NOT NULL,
    notes          TEXT NOT NULL DEFAULT '',
    checklist      TEXT NOT NULL DEFAULT '[]',
    estimate_min   INTEGER,
    difficulty     INTEGER,
    importance     INTEGER,
    urgency        INTEGER,
    task_type_id   TEXT,
    project_id     TEXT REFERENCES projects(id),
    place_id       TEXT,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    deleted_at     TEXT,
    rev            INTEGER NOT NULL,
    CHECK ((owner_user_id IS NULL) <> (owner_group_id IS NULL))
);
CREATE INDEX task_templates_owner ON task_templates(owner_user_id);
CREATE INDEX task_templates_group ON task_templates(owner_group_id);
CREATE INDEX task_templates_rev ON task_templates(rev);
