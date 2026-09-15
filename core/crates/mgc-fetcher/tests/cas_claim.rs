#![allow(clippy::unwrap_used)]
// CAS refcount claim wiring test (T1 slice 4-5): extraction imports blobs
// into the store and claims each one under the project key.
// (Test wiring claim refcount CAS (T1 slice 4-5): extract import blob vào
//  store và claim từng blob dưới khóa project.)
#![allow(clippy::too_many_arguments)]
use flate2::Compression;
use flate2::write::GzEncoder;
use mgc_fetcher::extract::extract_tarball_to_cas_and_link;
use mgc_store::{ContentStore, Database};
use tar::{Builder, Header};

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

// Canonicalize the temp dir (P0-A contract, same convention as the
// mgc-store adversarial tests): macOS temp lives under /var — a SYSTEM
// symlink — and the CAS now refuses any symlinked ancestor. Canonicalize
// ONCE at test setup so the store root is a real path; the production
// sweep stays absolute.
// (Canonicalize temp dir (hợp đồng P0-A, cùng quy ước test adversarial
// mgc-store): temp macOS nằm dưới /var — symlink HỆ THỐNG — và CAS giờ
// từ chối mọi ancestor là symlink. Canonicalize MỘT lần lúc setup để
// store root là đường dẫn thật; sweep của production giữ tuyệt đối.)
struct CanonicalTemp(std::path::PathBuf);

impl CanonicalTemp {
    fn new() -> Self {
        // Thread-unique component (flaky-test fix, vòng-11): the old name
        // was `pid + nanos` only — two tests running as PARALLEL THREADS of
        // one process can draw the SAME coarse nanos tick, collide on one
        // directory, and silently SHARE store.db + cas/ (claims from test A
        // pollute test B's count: "3 claims instead of 1"; the hardlink
        // path then hits EEXIST). Adding the thread id makes every
        // concurrent test's store distinct; the nanos still separates
        // sequential runs within one thread.
        // (Thành phần duy nhất theo thread (sửa test flaky): tên cũ chỉ
        // `pid + nanos` — 2 test chạy THREAD SONG SONG cùng process có thể
        // rút trùng tick nanos thô, đụng một thư mục, và âm thầm DÙNG
        // CHUNG store.db + cas/ (claim của test A nhiễm vào đếm của test
        // B: "3 claim thay vì 1"; hardlink rồi dính EEXIST). Thêm thread id
        // khiến store của mọi test song song khác biệt; nanos vẫn tách các
        // lần chạy tuần tự trong một thread.)
        let tid = {
            let tid = std::thread::current().id();
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(&tid, &mut hasher);
            std::hash::Hasher::finish(&hasher)
        };
        let canonical_base = std::env::temp_dir().canonicalize().unwrap();
        let real = canonical_base.join(format!(
            "mgc-fetcher-cas-claim-{}-{tid:x}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&real).unwrap();
        Self(real)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for CanonicalTemp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn cas_claim_registers_imported_blobs() {
    let temp = CanonicalTemp::new();
    let root = temp.path().join("store");
    let store = ContentStore::new(root.join("cas")).unwrap();
    let db = Database::open(&root.join("store.db")).unwrap();
    let project_key = root.to_string_lossy().into_owned();
    // Install token (P0-A): claims must land in the install's OWN
    // generation — begin one, thread it through the extraction.
    // (Token install (P0-A): claim phải rơi vào generation CỦA install —
    // begin một cái rồi xuyên nó qua extraction.)
    let token = db.begin_cas_generation(&project_key).unwrap();

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
    extract_tarball_to_cas_and_link(file, &dest, &store, Some((&db, &project_key, token))).unwrap();

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
    let temp = CanonicalTemp::new();
    let root = temp.path().join("store");
    let store = ContentStore::new(root.join("cas")).unwrap();
    let db = Database::open(&root.join("store.db")).unwrap();
    let project_key = root.to_string_lossy().into_owned();
    let token = db.begin_cas_generation(&project_key).unwrap();

    let tarball = temp.path().join("pkg.tgz");
    write_test_tarball(&tarball, &[("package/index.js", b"console.log('b');")]);

    // Same project extracts the same tarball twice (e.g. re-install).
    // The claim table must stay one row per (project, hash).
    for out in ["out1", "out2"] {
        let dest = temp.path().join(out);
        let file = std::fs::File::open(&tarball).unwrap();
        extract_tarball_to_cas_and_link(file, &dest, &store, Some((&db, &project_key, token)))
            .unwrap();
    }

    let claims = db.list_cas_live_refs().unwrap();
    assert_eq!(claims.len(), 1, "1 unique blob, 1 claim despite 2 extracts");
}
