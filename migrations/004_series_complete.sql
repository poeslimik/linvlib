-- Admin-marked completed series: skip bulk/scheduled Aladin refresh
ALTER TABLE series ADD COLUMN is_complete INTEGER NOT NULL DEFAULT 0;
CREATE INDEX idx_series_is_complete ON series(is_complete);
