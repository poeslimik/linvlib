-- Group series so searching one title also returns spin-offs / related works.
CREATE TABLE series_search_bundles (
    id TEXT PRIMARY KEY NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE series_search_bundle_members (
    bundle_id TEXT NOT NULL REFERENCES series_search_bundles(id) ON DELETE CASCADE,
    series_id TEXT NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    PRIMARY KEY (series_id)
);

CREATE INDEX idx_series_search_bundle_members_bundle
    ON series_search_bundle_members(bundle_id);
