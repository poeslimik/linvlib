-- Tutorial is shown once after email verification; no DB flag needed.
ALTER TABLE users DROP COLUMN tutorial_completed_at;
