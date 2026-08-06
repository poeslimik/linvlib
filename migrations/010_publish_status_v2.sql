-- Expand publish_status vocabulary and remap legacy stalled_* values.
UPDATE series SET publish_status = 'ongoing_stalled' WHERE publish_status = 'stalled_local';
UPDATE series SET publish_status = 'complete_partial' WHERE publish_status = 'stalled_done';

UPDATE series SET is_complete = CASE
  WHEN publish_status IN ('complete', 'complete_partial', 'complete_stalled') THEN 1
  ELSE 0
END;
