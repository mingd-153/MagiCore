CREATE TABLE IF NOT EXISTS mgc_runtime_migrations (
    name TEXT PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);
