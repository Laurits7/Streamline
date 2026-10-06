-- Streamline initial schema.
-- Conventions: ULID text ids; UTC RFC 3339 timestamps; local dates 'YYYY-MM-DD';
-- local times 'HH:MM'; every synced entity has created_at/updated_at/deleted_at/rev,
-- where rev comes from the global counter in meta_rev (see docs/WORKPLAN.md A.2).

CREATE TABLE meta_rev (
    id  INTEGER PRIMARY KEY CHECK (id = 1),
    rev INTEGER NOT NULL
);
INSERT INTO meta_rev (id, rev) VALUES (1, 0);

CREATE TABLE users (
    id                 TEXT PRIMARY KEY,
    username           TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name       TEXT NOT NULL,
    password_hash      TEXT NOT NULL,
    is_admin           INTEGER NOT NULL DEFAULT 0,
    timezone           TEXT NOT NULL DEFAULT 'UTC',
    day_end            TEXT NOT NULL DEFAULT '04:00',
    last_rollover_date TEXT,
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL,
    deleted_at         TEXT,
    rev                INTEGER NOT NULL
);

CREATE TABLE sessions (
    token_hash   TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id),
    created_at   TEXT NOT NULL,
    expires_at   TEXT NOT NULL,
    user_agent   TEXT
);
CREATE INDEX sessions_user ON sessions(user_id);

CREATE TABLE api_tokens (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id),
    name         TEXT NOT NULL,
    token_hash   TEXT NOT NULL UNIQUE,
    created_at   TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at   TEXT
);
CREATE INDEX api_tokens_user ON api_tokens(user_id);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE task_types (
    id                TEXT PRIMARY KEY,
    key               TEXT NOT NULL UNIQUE,
    name              TEXT NOT NULL,
    day_end_behavior  TEXT NOT NULL CHECK (day_end_behavior IN ('carry','expire','window','deadline')),
    counts_for_streak INTEGER NOT NULL DEFAULT 0,
    shows_overdue     INTEGER NOT NULL DEFAULT 0,
    shows_carry_count INTEGER NOT NULL DEFAULT 1,
    window_overflow   TEXT NOT NULL DEFAULT 'miss' CHECK (window_overflow IN ('miss','roll')),
    builtin           INTEGER NOT NULL DEFAULT 0,
    owner_user_id     TEXT REFERENCES users(id),
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    deleted_at        TEXT,
    rev               INTEGER NOT NULL
);
INSERT INTO task_types (id, key, name, day_end_behavior, counts_for_streak, shows_overdue, shows_carry_count, builtin, created_at, updated_at, rev) VALUES
  ('tt_carry_on', 'carry_on', 'Carry on', 'carry',  0, 0, 1, 1, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z', 0),
  ('tt_expires', 'expires',  'Expires',  'expire', 1, 0, 0, 1, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z', 0);

CREATE TABLE projects (
    id             TEXT PRIMARY KEY,
    owner_user_id  TEXT REFERENCES users(id),
    owner_group_id TEXT,
    name           TEXT NOT NULL,
    color          TEXT,
    position       TEXT NOT NULL,
    archived_at    TEXT,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    deleted_at     TEXT,
    rev            INTEGER NOT NULL,
    CHECK ((owner_user_id IS NULL) <> (owner_group_id IS NULL))
);
CREATE INDEX projects_owner ON projects(owner_user_id);
CREATE INDEX projects_rev ON projects(rev);

CREATE TABLE tasks (
    id               TEXT PRIMARY KEY,
    owner_user_id    TEXT REFERENCES users(id),
    owner_group_id   TEXT,
    assignee_user_id TEXT REFERENCES users(id),
    project_id       TEXT REFERENCES projects(id),
    title            TEXT NOT NULL,
    notes            TEXT NOT NULL DEFAULT '',
    status           TEXT NOT NULL DEFAULT 'open'
                     CHECK (status IN ('open','done','missed','skipped','wont_do')),
    position         TEXT NOT NULL,
    due_date         TEXT,
    estimate_min     INTEGER,
    difficulty       INTEGER CHECK (difficulty BETWEEN 1 AND 3),
    importance       INTEGER CHECK (importance BETWEEN 0 AND 3),
    urgency          INTEGER CHECK (urgency BETWEEN 0 AND 3),
    actual_min       INTEGER NOT NULL DEFAULT 0,
    task_type_id     TEXT NOT NULL REFERENCES task_types(id),
    carry_count      INTEGER NOT NULL DEFAULT 0,
    completed_at     TEXT,
    completed_by     TEXT REFERENCES users(id),
    ext_source       TEXT,
    ext_id           TEXT,
    ext_url          TEXT,
    ext_payload      TEXT,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    deleted_at       TEXT,
    rev              INTEGER NOT NULL,
    CHECK ((owner_user_id IS NULL) <> (owner_group_id IS NULL))
);
CREATE INDEX tasks_owner ON tasks(owner_user_id, status);
CREATE INDEX tasks_project ON tasks(project_id);
CREATE INDEX tasks_rev ON tasks(rev);
CREATE UNIQUE INDEX tasks_ext ON tasks(ext_source, ext_id) WHERE ext_source IS NOT NULL AND deleted_at IS NULL;

-- Append-only history: completed, reopened, carried, missed, snoozed, skipped, wont_do...
CREATE TABLE task_events (
    id      TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    user_id TEXT REFERENCES users(id),
    kind    TEXT NOT NULL,
    source  TEXT NOT NULL DEFAULT 'app',
    data    TEXT,
    at      TEXT NOT NULL
);
CREATE INDEX task_events_task ON task_events(task_id);

-- The day plan is a separate layer over tasks. A task has at most one active entry.
CREATE TABLE day_entries (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id),
    date         TEXT NOT NULL,
    task_id      TEXT NOT NULL REFERENCES tasks(id),
    position     TEXT NOT NULL,
    start_time   TEXT,
    duration_min INTEGER,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    deleted_at   TEXT,
    rev          INTEGER NOT NULL
);
CREATE UNIQUE INDEX day_entries_task ON day_entries(task_id) WHERE deleted_at IS NULL;
CREATE INDEX day_entries_user_date ON day_entries(user_id, date);
CREATE INDEX day_entries_rev ON day_entries(rev);
