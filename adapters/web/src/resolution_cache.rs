// Resolution cache keys for core-web — stable hashing of dependency inputs.
// Khóa cache resolution của core-web — tránh phụ thuộc thứ tự trong manifest.
use mgc_types::Manifest;
use sha2::{Digest, Sha256};

pub fn manifest_resolution_cache_key(manifest: &Manifest, registry_url: &str) -> String {
    let mut entries = Vec::new();
    for (group, deps) in manifest.dep_groups() {
        for dep in deps {
            entries.push(format!(
                "{}\0{}\0{}\0{}\0{}\0{}",
                group,
                dep.name.as_str(),
                dep.range.as_str(),
                dep.dev,
                dep.optional,
                dep.peer
            ));
        }
    }
    entries.sort_unstable();

    let mut hasher = Sha256::new();
    // v3 invalidates graphs built from metadata cache records that discarded
    // peerDependenciesMeta before the metadata cache schema was versioned.
    // v3 loại graph sinh từ metadata cache cũ đã làm rơi peerDependenciesMeta.
    hasher.update(b"magicore-web-resolution-v3\0");
    hasher.update(registry_url.trim_end_matches('/').as_bytes());
    hasher.update(b"\0");
    for entry in entries {
        hasher.update(entry.as_bytes());
        hasher.update(b"\0");
    }
    format!("{:x}", hasher.finalize())
}
