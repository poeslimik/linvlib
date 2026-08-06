-- Volumes filled from JP editions without a Korean release.
ALTER TABLE volumes ADD COLUMN is_unreleased INTEGER NOT NULL DEFAULT 0;
