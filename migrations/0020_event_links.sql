-- Calendar events linked to work (owner request, D-68).
-- A task can be "for" one calendar event (an instance: `events.id`).
ALTER TABLE tasks ADD COLUMN event_id TEXT;
CREATE INDEX tasks_event ON tasks(event_id) WHERE event_id IS NOT NULL;

-- A calendar event (all instances of a recurring one) belongs to a project. Personal: the
-- events are the user's own. Keyed by the event's calendar and UID, so it survives syncs.
CREATE TABLE event_projects (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id),
    calendar_id  TEXT NOT NULL REFERENCES calendars(id),
    uid          TEXT NOT NULL,
    project_id   TEXT NOT NULL REFERENCES projects(id),
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    deleted_at   TEXT,
    rev          INTEGER NOT NULL
);
CREATE UNIQUE INDEX event_projects_key ON event_projects(user_id, calendar_id, uid);
CREATE INDEX event_projects_rev ON event_projects(user_id, rev);
