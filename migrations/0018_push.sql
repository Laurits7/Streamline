-- Notifications (Phase 6.2): Web Push subscriptions per device, and per-user choices.
CREATE TABLE push_subscriptions (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id),
    endpoint    TEXT NOT NULL UNIQUE,
    p256dh      TEXT NOT NULL,
    auth        TEXT NOT NULL,
    user_agent  TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL,
    last_ok_at  TEXT,
    failures    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX push_subscriptions_user ON push_subscriptions(user_id);

-- Kinds of notification the user turned off (planning, ready, focus, conflict, metric, occasion).
ALTER TABLE users ADD COLUMN notify_off TEXT NOT NULL DEFAULT '[]';
-- Optional ntfy topic URL (e.g. https://ntfy.sh/my-secret-topic) as another channel.
ALTER TABLE users ADD COLUMN ntfy_url TEXT;
