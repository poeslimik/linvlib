-- New Aladin releases not yet in the catalog (admin can import later).
CREATE TABLE new_release_suggestions (
    id TEXT PRIMARY KEY NOT NULL,
    -- Stable key: numeric Aladin series id, or "item:{itemId}" when no seriesInfo.
    suggestion_key TEXT NOT NULL UNIQUE,
    aladin_series_id TEXT,
    sample_item_id TEXT NOT NULL,
    title TEXT NOT NULL,
    author TEXT,
    publisher TEXT,
    cover_url TEXT,
    pub_date TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'dismissed', 'imported')),
    first_seen_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    last_seen_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX idx_new_release_suggestions_status
    ON new_release_suggestions(status, last_seen_at DESC);
