-- The morning digest (10.6): when to send it (local HH:MM; NULL = off, the default) and
-- the logical day it was last sent for, so it goes out once a day.
ALTER TABLE users ADD COLUMN digest_time TEXT;
ALTER TABLE users ADD COLUMN last_digest_date TEXT;
