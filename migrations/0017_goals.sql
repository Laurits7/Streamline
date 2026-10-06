-- Long-term goals (SPEC §6.10, D-21). Personal, or shared with a group like projects.
--   milestones:  [{"id", "title", "due_date"?, "done"}]
--   project_ids / task_ids: linked work; progress is derived from it unless overridden.
CREATE TABLE goals (
    id                 TEXT PRIMARY KEY,
    owner_user_id      TEXT REFERENCES users(id),
    owner_group_id     TEXT,
    title              TEXT NOT NULL,
    description        TEXT NOT NULL DEFAULT '',
    target_date        TEXT,
    status             TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'paused', 'achieved', 'dropped')),
    progress_override  REAL,               -- 0..1; NULL = derived
    milestones         TEXT NOT NULL DEFAULT '[]',
    project_ids        TEXT NOT NULL DEFAULT '[]',
    task_ids           TEXT NOT NULL DEFAULT '[]',
    position           TEXT NOT NULL,
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL,
    deleted_at         TEXT,
    rev                INTEGER NOT NULL,
    CHECK ((owner_user_id IS NULL) <> (owner_group_id IS NULL))
);
CREATE INDEX goals_owner ON goals(owner_user_id, rev);
CREATE INDEX goals_group ON goals(owner_group_id, rev);

-- Notes from periodic reviews (one per goal and review).
CREATE TABLE goal_reviews (
    id          TEXT PRIMARY KEY,
    goal_id     TEXT NOT NULL REFERENCES goals(id),
    user_id     TEXT NOT NULL REFERENCES users(id),
    date        TEXT NOT NULL,
    progress    REAL,
    note        TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL
);
CREATE INDEX goal_reviews_goal ON goal_reviews(goal_id, date);

ALTER TABLE users ADD COLUMN review_cadence TEXT NOT NULL DEFAULT 'weekly' CHECK (review_cadence IN ('off', 'weekly', 'monthly'));
ALTER TABLE users ADD COLUMN last_review_date TEXT;
