CREATE TABLE IF NOT EXISTS pypi_files (
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    filename TEXT NOT NULL,
    digest TEXT NOT NULL,
    size INTEGER NOT NULL,
    requires_python TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (name, filename)
);