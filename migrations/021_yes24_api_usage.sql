-- Yes24 Open API daily usage counter.
-- Safe if the table already exists (e.g. DB migrated offline) or if the
-- legacy Aladin table was never created on this database.
CREATE TABLE IF NOT EXISTS yes24_api_usage (
    usage_date TEXT PRIMARY KEY NOT NULL,
    query_count INTEGER NOT NULL DEFAULT 0
);
