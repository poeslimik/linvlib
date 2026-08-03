-- Allow miscellaneous ('other') catalog/user requests
PRAGMA foreign_keys = OFF;

CREATE TABLE catalog_requests_new (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    request_type TEXT NOT NULL CHECK (request_type IN ('add', 'edit', 'delete', 'other')),
    series_id TEXT REFERENCES series(id) ON DELETE SET NULL,
    title TEXT,
    author TEXT,
    publisher TEXT,
    aladin_series_id TEXT,
    note TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'approved', 'rejected')),
    admin_note TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

INSERT INTO catalog_requests_new (
    id, user_id, request_type, series_id, title, author, publisher,
    aladin_series_id, note, status, admin_note, created_at, updated_at
)
SELECT
    id, user_id, request_type, series_id, title, author, publisher,
    aladin_series_id, note, status, admin_note, created_at, updated_at
FROM catalog_requests;

DROP TABLE catalog_requests;
ALTER TABLE catalog_requests_new RENAME TO catalog_requests;

CREATE INDEX idx_catalog_requests_user ON catalog_requests(user_id, created_at DESC);
CREATE INDEX idx_catalog_requests_status ON catalog_requests(status, created_at ASC);

PRAGMA foreign_keys = ON;
