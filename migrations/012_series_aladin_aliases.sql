CREATE TABLE series_aladin_aliases (
    aladin_series_id TEXT PRIMARY KEY NOT NULL,
    series_id TEXT NOT NULL REFERENCES series(id) ON DELETE CASCADE
);

CREATE INDEX idx_series_aladin_aliases_series_id
    ON series_aladin_aliases(series_id);
