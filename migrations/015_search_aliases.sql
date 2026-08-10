-- Search aliases (auto choseong/initials + user/admin nicknames)
CREATE TABLE series_search_aliases (
    id TEXT PRIMARY KEY NOT NULL,
    series_id TEXT NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    alias TEXT NOT NULL COLLATE NOCASE,
    source TEXT NOT NULL CHECK (source IN ('auto', 'user', 'admin')),
    UNIQUE (series_id, alias COLLATE NOCASE)
);

CREATE INDEX idx_series_search_aliases_alias
    ON series_search_aliases (alias COLLATE NOCASE);

CREATE INDEX idx_series_search_aliases_series
    ON series_search_aliases (series_id);

-- Expand catalog request types; store multi-series targets as JSON
PRAGMA foreign_keys = OFF;

CREATE TABLE catalog_requests_new (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    request_type TEXT NOT NULL
        CHECK (request_type IN ('add', 'edit', 'delete', 'other', 'search_improve')),
    series_id TEXT REFERENCES series(id) ON DELETE SET NULL,
    title TEXT,
    author TEXT,
    publisher TEXT,
    aladin_series_id TEXT,
    note TEXT,
    related_series_ids TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'approved', 'rejected')),
    admin_note TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

INSERT INTO catalog_requests_new (
    id, user_id, request_type, series_id, title, author, publisher,
    aladin_series_id, note, related_series_ids, status, admin_note, created_at, updated_at
)
SELECT
    id, user_id, request_type, series_id, title, author, publisher,
    aladin_series_id, note, NULL, status, admin_note, created_at, updated_at
FROM catalog_requests;

DROP TABLE catalog_requests;
ALTER TABLE catalog_requests_new RENAME TO catalog_requests;

CREATE INDEX idx_catalog_requests_user ON catalog_requests(user_id, created_at DESC);
CREATE INDEX idx_catalog_requests_status ON catalog_requests(status, created_at ASC);

PRAGMA foreign_keys = ON;
