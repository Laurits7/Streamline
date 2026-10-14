-- Focus options (D-82), kept with the timer so every computer running the desktop app
-- applies them: mute the speakers while working, and show the timer on every monitor.
ALTER TABLE focus_timers ADD COLUMN mute INTEGER NOT NULL DEFAULT 0;
ALTER TABLE focus_timers ADD COLUMN screens TEXT NOT NULL DEFAULT 'off'
    CHECK (screens IN ('off', 'overlay', 'fullscreen', 'fullscreen_breaks'));
