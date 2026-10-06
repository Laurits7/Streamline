-- Phase 4: groups (SPEC §6.4). Projects, tasks and routines can be owned by a group;
-- every member sees them and any member can complete group tasks.
CREATE TABLE groups (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    rev        INTEGER NOT NULL
);
CREATE TABLE group_members (
    group_id   TEXT NOT NULL REFERENCES groups(id),
    user_id    TEXT NOT NULL REFERENCES users(id),
    role       TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('owner', 'member')),
    created_at TEXT NOT NULL,
    PRIMARY KEY (group_id, user_id)
);
CREATE INDEX group_members_user ON group_members(user_id);
CREATE INDEX tasks_group ON tasks(owner_group_id);
CREATE INDEX projects_group ON projects(owner_group_id);

-- Day plans stay personal: each member can plan the same group task on their own day.
DROP INDEX day_entries_task;
CREATE UNIQUE INDEX day_entries_task_user ON day_entries(task_id, user_id) WHERE deleted_at IS NULL;
