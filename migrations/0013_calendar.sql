-- Calendar read (SPEC §6.5, D-55). Each user connects a CalDAV account (the schema allows
-- more than one); its calendars are listed, the raw event resources cached, and the
-- instances in a rolling window expanded into `events` (what clients see).
CREATE TABLE calendar_accounts (
    id            TEXT PRIMARY KEY,
    user_id       TEXT NOT NULL REFERENCES users(id),
    kind          TEXT NOT NULL DEFAULT 'caldav',
    url           TEXT NOT NULL,
    username      TEXT NOT NULL DEFAULT '',
    secret        BLOB,                         -- encrypted password (secrets.rs)
    status        TEXT NOT NULL DEFAULT 'new',  -- new | ok | error
    last_sync_at  TEXT,
    last_error    TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    deleted_at    TEXT,
    rev           INTEGER NOT NULL
);
CREATE INDEX calendar_accounts_user ON calendar_accounts(user_id);

CREATE TABLE calendars (
    id            TEXT PRIMARY KEY,
    account_id    TEXT NOT NULL REFERENCES calendar_accounts(id),
    user_id       TEXT NOT NULL REFERENCES users(id),
    href          TEXT NOT NULL,
    name          TEXT NOT NULL,
    color         TEXT,                         -- from the server
    user_color    TEXT,                         -- the user's choice, wins over `color`
    enabled       INTEGER NOT NULL DEFAULT 1,
    ctag          TEXT,                         -- last seen; NULL forces a full refresh
    expanded_for  TEXT,                         -- date the instance window was computed for
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    deleted_at    TEXT,
    rev           INTEGER NOT NULL
);
CREATE UNIQUE INDEX calendars_href ON calendars(account_id, href);
CREATE INDEX calendars_user_rev ON calendars(user_id, rev);

CREATE TABLE calendar_objects (
    calendar_id   TEXT NOT NULL REFERENCES calendars(id),
    href          TEXT NOT NULL,
    etag          TEXT NOT NULL,
    ics           TEXT NOT NULL,
    PRIMARY KEY (calendar_id, href)
);

CREATE TABLE events (
    id            TEXT PRIMARY KEY,
    user_id       TEXT NOT NULL REFERENCES users(id),
    calendar_id   TEXT NOT NULL REFERENCES calendars(id),
    uid           TEXT NOT NULL,
    instance_key  TEXT NOT NULL,                -- '' for one-off events
    title         TEXT NOT NULL,
    location      TEXT,
    all_day       INTEGER NOT NULL,
    start_at      TEXT,                         -- timed: RFC 3339 UTC
    end_at        TEXT,
    start_date    TEXT,                         -- all-day: YYYY-MM-DD, end exclusive
    end_date      TEXT,
    busy          INTEGER NOT NULL DEFAULT 1,
    recurring     INTEGER NOT NULL DEFAULT 0,
    updated_at    TEXT NOT NULL,
    deleted_at    TEXT,
    rev           INTEGER NOT NULL
);
CREATE UNIQUE INDEX events_instance ON events(calendar_id, uid, instance_key);
CREATE INDEX events_user_rev ON events(user_id, rev);
CREATE INDEX events_user_start ON events(user_id, start_at);
