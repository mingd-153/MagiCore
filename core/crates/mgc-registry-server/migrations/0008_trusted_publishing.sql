CREATE TABLE IF NOT EXISTS trusted_publishers (
    package TEXT PRIMARY KEY,
    repository TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS trusted_attestations (
    package TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    payload TEXT NOT NULL,
    entry_hash TEXT NOT NULL,
    key_id TEXT NOT NULL,
    signature TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (package, sequence),
    UNIQUE (package, entry_hash)
);

CREATE TRIGGER IF NOT EXISTS trusted_attestations_no_update
BEFORE UPDATE ON trusted_attestations
BEGIN
    SELECT RAISE(ABORT, 'trusted attestations are append-only');
END;

CREATE TRIGGER IF NOT EXISTS trusted_attestations_no_delete
BEFORE DELETE ON trusted_attestations
BEGIN
    SELECT RAISE(ABORT, 'trusted attestations are append-only');
END;
