-- Daily health check-in (D-75): one answer per user and day. Personal, like metrics.
CREATE TABLE health_days (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id),
    date       TEXT NOT NULL,
    status     TEXT NOT NULL CHECK (status IN ('great', 'ok', 'unwell', 'sick', 'injured')),
    kind       TEXT,
    note       TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    rev        INTEGER NOT NULL
);
CREATE UNIQUE INDEX health_days_user_date ON health_days(user_id, date);
CREATE INDEX health_days_rev ON health_days(rev);
