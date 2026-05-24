CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    operator_id TEXT NOT NULL REFERENCES operators(id),
    display_name TEXT NOT NULL,
    role TEXT NOT NULL,
    language TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);
CREATE INDEX idx_sessions_operator ON sessions(operator_id);
CREATE INDEX idx_sessions_expires ON sessions(expires_at);
