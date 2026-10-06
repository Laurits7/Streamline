-- Performance pass (Phase 6.4): the day summary's "completed that day" counts and its
-- "carried over from this day" lookup (task history grows without bound).
CREATE INDEX tasks_completed ON tasks(completed_by, completed_at);
CREATE INDEX task_events_carried_from ON task_events(kind, json_extract(data, '$.from'));
