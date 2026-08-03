CREATE TABLE user_tierlist_gatekeepers (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tier TEXT NOT NULL CHECK (tier IN ('S', 'A', 'B', 'C', 'D', 'F')),
    side TEXT NOT NULL CHECK (side IN ('above', 'below')),
    series_id TEXT NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (user_id, tier, side)
);

CREATE INDEX idx_user_tierlist_gatekeepers_user
    ON user_tierlist_gatekeepers(user_id);
