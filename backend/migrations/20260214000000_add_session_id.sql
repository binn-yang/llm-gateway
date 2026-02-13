ALTER TABLE requests ADD COLUMN session_id TEXT;
CREATE INDEX IF NOT EXISTS idx_requests_session_id ON requests(session_id);
