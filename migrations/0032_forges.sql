-- Tasks from GitHub, GitLab and Gitea/Forgejo (7.6, D-97). An account per forge login,
-- its access token encrypted like the calendar password (secrets.rs).
CREATE TABLE forge_accounts (
    id            TEXT PRIMARY KEY,
    user_id       TEXT NOT NULL REFERENCES users(id),
    kind          TEXT NOT NULL,                -- github | gitlab | gitea
    url           TEXT NOT NULL,                -- the server's web address
    login         TEXT NOT NULL DEFAULT '',     -- whose token it is
    secret        BLOB,
    -- What to bring in.
    issues        INTEGER NOT NULL DEFAULT 1,
    prs           INTEGER NOT NULL DEFAULT 1,
    reviews       INTEGER NOT NULL DEFAULT 1,
    mentions      INTEGER NOT NULL DEFAULT 1,
    rules         TEXT NOT NULL DEFAULT '[]',   -- [{repo, project_id}], first match wins
    status        TEXT NOT NULL DEFAULT 'new',  -- new | ok | error
    last_sync_at  TEXT,
    last_error    TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);
CREATE INDEX forge_accounts_user ON forge_accounts(user_id);
-- Every item that got a task (kept after the task is done or deleted, so it isn't made
-- again), with the reasons it was last seen for (domain::forge bit flags).
CREATE TABLE forge_items (
    account_id    TEXT NOT NULL REFERENCES forge_accounts(id),
    item_key      TEXT NOT NULL,
    task_id       TEXT NOT NULL,
    kinds         INTEGER NOT NULL,
    PRIMARY KEY (account_id, item_key)
);
