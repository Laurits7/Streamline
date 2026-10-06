-- Daily summary, reflection and tracking (SPEC §6.2b, Phase 5b). All personal: never
-- visible to group members.
CREATE TABLE day_records (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id),
    date        TEXT NOT NULL,
    journal     TEXT NOT NULL DEFAULT '',
    went_well   TEXT NOT NULL DEFAULT '',
    went_badly  TEXT NOT NULL DEFAULT '',
    tomorrow    TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    deleted_at  TEXT,
    rev         INTEGER NOT NULL,
    UNIQUE (user_id, date)
);
CREATE INDEX day_records_rev ON day_records(user_id, rev);

-- What a user tracks. Built-ins (`key` = 'mood' | 'weight') are created on first use.
-- `aggregate`: how several entries of one day combine (weight: latest, mood: average,
-- steps: sum, yes/no: max).
CREATE TABLE metric_definitions (
    id                 TEXT PRIMARY KEY,
    owner_user_id      TEXT NOT NULL REFERENCES users(id),
    key                TEXT,                       -- built-in kind, or NULL for custom
    name               TEXT NOT NULL,
    kind               TEXT NOT NULL CHECK (kind IN ('number', 'scale', 'yes_no')),
    unit               TEXT NOT NULL DEFAULT '',   -- canonical unit (weight: kg)
    scale_min          INTEGER,
    scale_max          INTEGER,
    aggregate          TEXT NOT NULL DEFAULT 'latest' CHECK (aggregate IN ('latest', 'average', 'sum', 'max')),
    reminder_time      TEXT,                       -- HH:MM: remind if nothing logged by then
    last_reminded      TEXT,                       -- date of the last reminder
    archived           INTEGER NOT NULL DEFAULT 0,
    position           TEXT NOT NULL,
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL,
    deleted_at         TEXT,
    rev                INTEGER NOT NULL,
    UNIQUE (owner_user_id, key)
);
CREATE INDEX metric_definitions_rev ON metric_definitions(owner_user_id, rev);

CREATE TABLE metric_entries (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id),
    metric_id   TEXT NOT NULL REFERENCES metric_definitions(id),
    date        TEXT NOT NULL,           -- the user's logical day
    at          TEXT NOT NULL,           -- when it was logged
    value       REAL NOT NULL,
    note        TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    deleted_at  TEXT,
    rev         INTEGER NOT NULL
);
CREATE INDEX metric_entries_day ON metric_entries(metric_id, date);
CREATE INDEX metric_entries_rev ON metric_entries(user_id, rev);

-- D-23: metric (kg) or imperial (lb) display; stored values are always metric.
ALTER TABLE users ADD COLUMN unit_system TEXT NOT NULL DEFAULT 'metric' CHECK (unit_system IN ('metric', 'imperial'));
