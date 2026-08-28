ALTER TABLE oci_manifests ADD COLUMN digest TEXT NOT NULL DEFAULT '';
CREATE INDEX IF NOT EXISTS idx_oci_manifests_digest ON oci_manifests (repo, digest);
