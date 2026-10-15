-- Sharing projects between Streamline servers (Phase 8, D-69, D-100). A project's home
-- server stays authoritative; a collaborator's server keeps a replica and forwards edits.

-- People who live on another server: login-less rows (`home` = that server's address),
-- with the same id there and here so who-did-what matches on both sides.
ALTER TABLE users ADD COLUMN home TEXT;
-- A group mirrored from another server (its members and name come from there).
ALTER TABLE groups ADD COLUMN home TEXT;

-- ---- On the home server ----
-- Single-use invite links to a top-level group project (only the hash is kept).
CREATE TABLE fed_invites (
    token_hash  TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id),
    created_by  TEXT NOT NULL REFERENCES users(id),
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL,
    used_at     TEXT
);
-- One per person who joined from another server: their replica's credentials.
CREATE TABLE fed_links (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES projects(id),
    remote_user_id  TEXT NOT NULL REFERENCES users(id),
    secret_hash     TEXT NOT NULL,
    created_by      TEXT REFERENCES users(id),
    created_at      TEXT NOT NULL,
    last_seen_at    TEXT,
    revoked_at      TEXT
);
CREATE INDEX fed_links_project ON fed_links(project_id);
-- What each link has been sent, so items that leave the project reach it as gone.
CREATE TABLE fed_sent (
    link_id  TEXT NOT NULL REFERENCES fed_links(id),
    kind     TEXT NOT NULL,              -- project | task | series
    item_id  TEXT NOT NULL,
    PRIMARY KEY (link_id, kind, item_id)
);

-- ---- On the collaborator's server ----
-- Projects joined from another server, one per local user who joined.
CREATE TABLE fed_joins (
    id               TEXT PRIMARY KEY,    -- the link's id on the home server
    user_id          TEXT NOT NULL REFERENCES users(id),
    home_url         TEXT NOT NULL,
    root_project_id  TEXT NOT NULL,
    group_id         TEXT NOT NULL,
    secret           BLOB NOT NULL,       -- encrypted (secrets.rs)
    since_rev        INTEGER NOT NULL DEFAULT 0,
    status           TEXT NOT NULL DEFAULT 'new',  -- new | ok | error
    last_sync_at     TEXT,
    last_error       TEXT,
    created_at       TEXT NOT NULL
);
CREATE INDEX fed_joins_user ON fed_joins(user_id);
-- Rows here that are replicas (read-only mirrors; edits go to the home server).
CREATE TABLE fed_items (
    join_id  TEXT NOT NULL REFERENCES fed_joins(id),
    kind     TEXT NOT NULL,
    item_id  TEXT NOT NULL,
    PRIMARY KEY (join_id, kind, item_id)
);
CREATE INDEX fed_items_item ON fed_items(item_id);
