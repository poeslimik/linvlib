PRAGMA journal_mode = WAL;

CREATE TABLE users (
    id TEXT PRIMARY KEY NOT NULL,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE series (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    author TEXT,
    publisher TEXT,
    aladin_series_id TEXT NOT NULL UNIQUE,
    first_published_at TEXT,
    latest_published_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE volumes (
    id TEXT PRIMARY KEY NOT NULL,
    series_id TEXT NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    volume_number INTEGER NOT NULL,
    title TEXT NOT NULL,
    cover_url TEXT,
    published_at TEXT,
    aladin_item_id TEXT NOT NULL UNIQUE,
    isbn13 TEXT,
    UNIQUE (series_id, volume_number)
);

CREATE TABLE user_volume_reads (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    volume_id TEXT NOT NULL REFERENCES volumes(id) ON DELETE CASCADE,
    is_read INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (user_id, volume_id)
);

CREATE TABLE user_series_ratings (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    series_id TEXT NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    rating TEXT NOT NULL DEFAULT 'None' CHECK (rating IN ('S', 'A', 'B', 'C', 'D', 'F', 'None')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (user_id, series_id)
);

CREATE TABLE user_tierlist_entries (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    series_id TEXT NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    tier TEXT NOT NULL CHECK (tier IN ('S', 'A', 'B', 'C', 'D', 'F')),
    position INTEGER NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (user_id, series_id)
);

CREATE INDEX idx_volumes_series_id ON volumes(series_id);
CREATE INDEX idx_user_volume_reads_user_id ON user_volume_reads(user_id);
CREATE INDEX idx_user_series_ratings_user_id ON user_series_ratings(user_id);
CREATE INDEX idx_user_tierlist_entries_user_tier ON user_tierlist_entries(user_id, tier, position);
CREATE INDEX idx_series_latest_published_at ON series(latest_published_at);
CREATE INDEX idx_series_title ON series(title);
