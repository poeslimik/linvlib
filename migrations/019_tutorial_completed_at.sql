-- First-login tour completion (NULL = not yet shown/finished).
ALTER TABLE users ADD COLUMN tutorial_completed_at TEXT;

-- Existing accounts skip the tour; only users created after this migration see it.
UPDATE users
SET tutorial_completed_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE tutorial_completed_at IS NULL;
