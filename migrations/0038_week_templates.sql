-- Week templates, and routines and tasks in template blocks (D-113).
--
-- Template blocks (`day_templates.blocks`) can hold items:
--   {"id": ULID, "kind": "routine"|"task", "series_id", "title", "estimate_min", "notes"}
-- What each set-up day already got from them: {"<item id>": "<task id>"}, so a day set
-- up again (its template changed) doesn't plan an item twice.
ALTER TABLE block_days ADD COLUMN placed TEXT NOT NULL DEFAULT '{}';

-- Week templates: which day template each weekday gets (`days`: 7 day template ids or
-- null, Monday first). One `default` week, at most one `alternate` (every other week from
-- the week of `start_date`) and any number for a date `range` (`start_date`..`end_date`).
-- A range wins, then the alternate week, then the default. The default week and
-- `day_templates.weekdays` are kept in line both ways (older clients set weekdays).
CREATE TABLE week_templates (
    id             TEXT PRIMARY KEY,
    owner_user_id  TEXT NOT NULL REFERENCES users(id),
    name           TEXT NOT NULL,
    days           TEXT NOT NULL DEFAULT '[null,null,null,null,null,null,null]',
    mode           TEXT NOT NULL CHECK (mode IN ('default', 'alternate', 'range')),
    start_date     TEXT,
    end_date       TEXT,
    position       TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    deleted_at     TEXT,
    rev            INTEGER NOT NULL
);
CREATE INDEX week_templates_owner ON week_templates(owner_user_id, rev);

-- Everyone's weekday assignments become their default week (ids: valid ULIDs).
UPDATE meta_rev SET rev = rev + 1 WHERE id = 1;
INSERT INTO week_templates (id, owner_user_id, name, days, mode, position, created_at, updated_at, rev)
SELECT '01' || upper(hex(randomblob(12))), u.owner_user_id, 'Usual week',
       json_array(
           (SELECT d.id FROM day_templates d WHERE d.owner_user_id = u.owner_user_id AND d.deleted_at IS NULL AND EXISTS (SELECT 1 FROM json_each(d.weekdays) WHERE value = 1) ORDER BY d.position LIMIT 1),
           (SELECT d.id FROM day_templates d WHERE d.owner_user_id = u.owner_user_id AND d.deleted_at IS NULL AND EXISTS (SELECT 1 FROM json_each(d.weekdays) WHERE value = 2) ORDER BY d.position LIMIT 1),
           (SELECT d.id FROM day_templates d WHERE d.owner_user_id = u.owner_user_id AND d.deleted_at IS NULL AND EXISTS (SELECT 1 FROM json_each(d.weekdays) WHERE value = 3) ORDER BY d.position LIMIT 1),
           (SELECT d.id FROM day_templates d WHERE d.owner_user_id = u.owner_user_id AND d.deleted_at IS NULL AND EXISTS (SELECT 1 FROM json_each(d.weekdays) WHERE value = 4) ORDER BY d.position LIMIT 1),
           (SELECT d.id FROM day_templates d WHERE d.owner_user_id = u.owner_user_id AND d.deleted_at IS NULL AND EXISTS (SELECT 1 FROM json_each(d.weekdays) WHERE value = 5) ORDER BY d.position LIMIT 1),
           (SELECT d.id FROM day_templates d WHERE d.owner_user_id = u.owner_user_id AND d.deleted_at IS NULL AND EXISTS (SELECT 1 FROM json_each(d.weekdays) WHERE value = 6) ORDER BY d.position LIMIT 1),
           (SELECT d.id FROM day_templates d WHERE d.owner_user_id = u.owner_user_id AND d.deleted_at IS NULL AND EXISTS (SELECT 1 FROM json_each(d.weekdays) WHERE value = 7) ORDER BY d.position LIMIT 1)
       ),
       'default', 'V', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
       (SELECT rev FROM meta_rev WHERE id = 1)
FROM (SELECT DISTINCT owner_user_id FROM day_templates WHERE deleted_at IS NULL) u;
