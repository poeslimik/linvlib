-- Admin / email verification / catalog requests / Aladin quota

ALTER TABLE users ADD COLUMN is_admin INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN email_verified INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN verify_token TEXT;
ALTER TABLE users ADD COLUMN verify_token_expires_at TEXT;

-- Existing accounts stay usable; new signups must verify.
UPDATE users SET email_verified = 1;
-- Bootstrap admin via ADMIN_EMAIL env (see ensure_admin_by_email), not a hard-coded address.

ALTER TABLE series ADD COLUMN last_refreshed_at TEXT;
UPDATE series SET last_refreshed_at = COALESCE(created_at, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));

CREATE TABLE catalog_requests (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    request_type TEXT NOT NULL CHECK (request_type IN ('add', 'edit', 'delete')),
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

CREATE INDEX idx_catalog_requests_user ON catalog_requests(user_id, created_at DESC);
CREATE INDEX idx_catalog_requests_status ON catalog_requests(status, created_at ASC);

CREATE TABLE aladin_api_usage (
    usage_date TEXT PRIMARY KEY NOT NULL,
    query_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE app_meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
