DROP INDEX IF EXISTS idx_series_is_complete;
ALTER TABLE series DROP COLUMN is_complete;
