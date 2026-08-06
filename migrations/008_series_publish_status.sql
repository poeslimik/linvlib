-- Series publish/translation status for badges + bulk refresh skip.
-- ongoing: include in bulk/scheduled Aladin refresh
-- complete / stalled_*: exclude from bulk refresh
ALTER TABLE series ADD COLUMN publish_status TEXT NOT NULL DEFAULT 'ongoing';

UPDATE series
SET publish_status = 'complete'
WHERE COALESCE(is_complete, 0) = 1;

CREATE INDEX idx_series_publish_status ON series(publish_status);
