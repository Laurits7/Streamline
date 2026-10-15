-- Printed day sheets (7.3, D-14): the tasks on a printed sheet and a secret for its QR
-- link (only its hash is kept). The link expires and can only complete these tasks.
CREATE TABLE print_sheets (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id),
    date        TEXT NOT NULL,
    -- JSON [{code, task_id}], in printed order.
    items       TEXT NOT NULL,
    token_hash  TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL
);
CREATE INDEX print_sheets_user ON print_sheets(user_id);
