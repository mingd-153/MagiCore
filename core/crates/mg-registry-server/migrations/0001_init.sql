-- Initial schema for mg-registry-server
-- (SQLite; schema matches RegistryStore::init_schema)

CREATE TABLE IF NOT EXISTS packages (
    name TEXT PRIMARY KEY,
    description TEXT,
    dist_tags TEXT NOT NULL DEFAULT '{}',
    maintainers TEXT NOT NULL DEFAULT '[]',
    time TEXT NOT NULL,
    private BOOLEAN NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS package_versions (
    id TEXT PRIMARY KEY,
    package_name TEXT NOT NULL,
    version TEXT NOT NULL,
    data TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (package_name) REFERENCES packages(name) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS blobs (
    digest TEXT PRIMARY KEY,
    size INTEGER NOT NULL,
    path TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS oci_manifests (
    repo TEXT NOT NULL,
    reference TEXT NOT NULL,
    manifest TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo, reference)
);

CREATE TABLE IF NOT EXISTS oci_blobs (
    repo TEXT NOT NULL,
    digest TEXT NOT NULL,
    size INTEGER NOT NULL,
    path TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo, digest)
);
