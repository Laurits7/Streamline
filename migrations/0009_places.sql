-- Places (SPEC §6.16, D-45): where a task has to be done. Optional coordinates and
-- radius let the app detect the current place by GPS (opt-in, HTTPS only).
CREATE TABLE places (
    id             TEXT PRIMARY KEY,
    owner_user_id  TEXT REFERENCES users(id),
    owner_group_id TEXT,
    name           TEXT NOT NULL,
    lat            REAL,
    lon            REAL,
    radius_m       INTEGER NOT NULL DEFAULT 200,
    position       TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    deleted_at     TEXT,
    rev            INTEGER NOT NULL,
    CHECK ((owner_user_id IS NULL) <> (owner_group_id IS NULL))
);
CREATE INDEX places_owner ON places(owner_user_id);
CREATE INDEX places_rev ON places(rev);

ALTER TABLE tasks ADD COLUMN place_id TEXT REFERENCES places(id);
-- New tasks in a project get its default place (a task's place can still differ:
-- "buy a garden hose" for the cottage is done in town).
ALTER TABLE projects ADD COLUMN default_place_id TEXT REFERENCES places(id);
ALTER TABLE series ADD COLUMN place_id TEXT REFERENCES places(id);
