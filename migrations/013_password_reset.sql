ALTER TABLE users ADD COLUMN reset_token TEXT;
ALTER TABLE users ADD COLUMN reset_token_expires_at TEXT;

CREATE INDEX idx_users_reset_token ON users(reset_token);
