CREATE TABLE IF NOT EXISTS oci_uploads (
    repo TEXT NOT NULL,
    uuid TEXT NOT NULL,
    path TEXT NOT NULL,
    offset_bytes INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo, uuid)
);
