-- 021 used CREATE TABLE IF NOT EXISTS, so a table created earlier without a
-- primary key was left in place. ON CONFLICT(usage_date) then fails.
CREATE TABLE yes24_api_usage_pk (
    usage_date TEXT PRIMARY KEY NOT NULL,
    query_count INTEGER NOT NULL DEFAULT 0
);

INSERT INTO yes24_api_usage_pk (usage_date, query_count)
SELECT usage_date, MAX(COALESCE(query_count, 0))
FROM yes24_api_usage
WHERE usage_date IS NOT NULL AND TRIM(usage_date) <> ''
GROUP BY usage_date;

DROP TABLE yes24_api_usage;

ALTER TABLE yes24_api_usage_pk RENAME TO yes24_api_usage;
