-- Calendar write-back (7.7, D-98): timed tasks written to one of the user's calendars as
-- busy events. Which calendar (NULL = off), whether to show titles, and what was written
-- (so changes are pushed once and removals are cleaned up).
ALTER TABLE calendar_accounts ADD COLUMN write_calendar_id TEXT;
ALTER TABLE calendar_accounts ADD COLUMN write_titles INTEGER NOT NULL DEFAULT 1;
CREATE TABLE written_events (
    account_id   TEXT NOT NULL REFERENCES calendar_accounts(id),
    entry_id     TEXT NOT NULL,
    calendar_id  TEXT NOT NULL,
    href         TEXT NOT NULL,
    hash         TEXT NOT NULL,
    PRIMARY KEY (account_id, entry_id)
);
