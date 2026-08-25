#![allow(clippy::unwrap_used)]
// CAS refcount claim wiring test (T1 slice 4-5): extraction imports blobs
// into the store and claims each one under the project key.
// (Test wiring claim refcount CAS (T1 slice 4-5): extract import blob vào
//  store và claim từng blob dưới khóa project.)
use flate2::write::GzEncoder;
use flate2::Compression;
use mgc_fetcher::extract::extract_tarball_to_cas_and_link;
use mgc_store::{ContentStore, Database};
use tar::{Builder, Header};
use tempfile::TempDir;

fn write_test_tarball(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).unwrap();
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(encoder);
    for (name, data) in entries {
        let mut header = Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, name, &data[..]).unwrap();
    }
    builder.finish().unwrap();
    let encoder = builder.into_inner().unwrap();
    encoder.finish().unwrap();
}

#[test]
fn cas_claim_registers_imported_blobs() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("store");
    let store = ContentStore::new(root.join("cas")).unwrap();
    let db = Database::open(&root.join("store.db")).unwrap();
    let project_key = root.to_string_lossy().into_owned();

    let tarball = temp.path().join("pkg.tgz");
    write_test_tarball(
        &tarball,
        &[
            ("package/index.js", b"console.log('a');"),
            ("package/lib/util.js", b"module.exports = {};"),
        ],
    );

    let dest = temp.path().join("out");
    let file = std::fs::File::open(&tarball).unwrap();
    extract_tarball_to_cas_and_link(file, &dest, &store, Some((&db, &project_key))).unwrap();

    let claims = db.list_cas_live_refs().unwrap();
    assert_eq!(claims.len(), 2, "every blob imported must be claimed");
    for hash in &claims {
        let path = store.root().join("files").join("blake3");
        // Blob exists in CAS (sharded by first two hex chars)
        let present = std::fs::read_dir(&path)
            .unwrap()
            .flat_map(|dir| {
                let dir = dir.unwrap().path();
                std::fs::read_dir(dir).unwrap().collect::<Vec<_>>()
            })
            .any(|entry| {
                entry
                    .unwrap()
                    .path()
                    .file_name()
                    .map(|n| n.to_string_lossy() == *hash)
                    .unwrap_or(false)
            });
        assert!(present, "claimed blob {} must exist in CAS", hash);
    }
}

#[test]
fn cas_claim_is_idempotent_per_project() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("store");
    let store = ContentStore::new(root.join("cas")).unwrap();
    let db = Database::open(&root.join("store.db")).unwrap();
    let project_key = root.to_string_lossy().into_owned();

    let tarball = temp.path().join("pkg.tgz");
    write_test_tarball(&tarball, &[("package/index.js", b"console.log('b');")]);

    // Same project extracts the same tarball twice (e.g. re-install).
    // The claim table must stay one row per (project, hash).
    for out in ["out1", "out2"] {
        let dest = temp.path().join(out);
        let file = std::fs::File::open(&tarball).unwrap();
        extract_tarball_to_cas_and_link(file, &dest, &store, Some((&db, &project_key))).unwrap();
    }

    let claims = db.list_cas_live_refs().unwrap();
    assert_eq!(claims.len(), 1, "1 unique blob, 1 claim despite 2 extracts");
}
