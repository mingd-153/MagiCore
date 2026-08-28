-- audit_log: ghi event publish/delete/upload (task #3)
CREATE TABLE IF NOT EXISTS audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    name TEXT NOT NULL,
    version TEXT,
    user TEXT,
    result INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_audit_log_created ON audit_log(created_at);