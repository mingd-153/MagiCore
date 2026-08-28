CREATE TABLE IF NOT EXISTS users (
    name TEXT PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    password TEXT,
    email TEXT,
    is_admin BOOLEAN NOT NULL DEFAULT 0,
    scopes TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);