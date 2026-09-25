#![allow(clippy::unwrap_used)]
//! Registry storage hardening tests (P0-2).
//! Test hardening storage registry: digest recompute server-side, chặn
//! digest khai sai, path traversal qua repo/uuid, atomic upload, phát hiện
//! blob hỏng khi serve. Đối chiếu finding Tech Lead 2026-09-12.

use mgc_registry_server::storage::{BlobPresence, RegistryStore};

fn tmp_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

// === Malformed digest must fail-closed (no empty decode) ===

#[tokio::test]
async fn malformed_digest_is_rejected_not_stored_as_empty() {
    let tmp = tmp_dir();
    let store = RegistryStore::new(tmp.path()).await.unwrap();

    for bad in [
        "",
        "sha512-",
        "sha256:",
        "!!!not-a-digest!!!",
        "sha256:zzzz",
        "sha512-%%%invalid-b64",
    ] {
        let res = store.put_blob(bad, b"bytes").await;
        assert!(res.is_err(), "malformed digest '{bad}' must be rejected");
    }
    // No stray "00/" empty-decode directories from the old unwrap_or_default.
    // Không còn thư mục "00/" rác từ unwrap_or_default cũ.
    assert!(!tmp.path().join("blobs").join("00").exists());
}

// === Declared digest vs content mismatch must not be servable ===

#[tokio::test]
async fn declared_digest_mismatch_poisons_nothing() {
    let tmp = tmp_dir();
    let store = RegistryStore::new(tmp.path()).await.unwrap();

    // Client declares digest of content-B but uploads content-A.
    // Client khai digest của nội dung B nhưng upload nội dung A.
    use sha2::{Digest, Sha512};
    let mut hasher = Sha512::new();
    hasher.update(b"CONTENT-A");
    let b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        hasher.finalize(),
    );
    let honest_a = format!("sha512-{b64}");

    let mut hasher = Sha512::new();
    hasher.update(b"CONTENT-B");
    let b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        hasher.finalize(),
    );
    let declared_b = format!("sha512-{b64}");

    // Upload content A while declaring digest B — must be REJECTED outright.
    // Upload nội dung A nhưng khai digest B — phải bị TỪ CHỐI hoàn toàn.
    let res = store.put_blob(&declared_b, b"CONTENT-A").await;
    assert!(
        res.is_err(),
        "declared digest B with content A must be rejected at the door"
    );

    // The honest digest of content A was never stored by the forged attempt.
    // Digest thật của A không hề được lưu bởi lần khai báo giả.
    let forged = store.get_blob(&declared_b).await.unwrap();
    assert_eq!(
        forged, None,
        "forged declaration must not resolve to anything"
    );

    // And a clean upload still round-trips.
    // Upload sạch vẫn hoạt động bình thường.
    store.put_blob(&honest_a, b"CONTENT-A").await.unwrap();
    let served = store.get_blob(&honest_a).await.unwrap();
    assert_eq!(served.as_deref(), Some(b"CONTENT-A".as_ref()));
}

// === Serving detects on-disk corruption (trust-but-verify) ===

#[tokio::test]
async fn corrupted_blob_fails_serving_integrity_check() {
    let tmp = tmp_dir();
    let store = RegistryStore::new(tmp.path()).await.unwrap();

    use sha2::{Digest, Sha512};
    let mut hasher = Sha512::new();
    hasher.update(b"tarball-bytes");
    let b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        hasher.finalize(),
    );
    let digest = format!("sha512-{b64}");

    store.put_blob(&digest, b"tarball-bytes").await.unwrap();

    // Corrupt the stored blob behind the DB's back.
    // Sửa blob trên đĩa phía sau lưng DB.
    let key = store
        .blob_path_for_test(&digest)
        .await
        .unwrap()
        .expect("blob row exists");
    let path = tmp.path().join("blobs").join(&key);
    assert!(path.exists(), "blob file must exist at {path:?}");
    std::fs::write(&path, b"CORRUPTED").unwrap();

    // Serving must fail closed — corrupt bytes never reach the client.
    // Serve phải fail cứng — bytes hỏng không bao giờ tới client.
    let res = store.get_blob(&digest).await;
    assert!(res.is_err(), "corrupted blob must not be served silently");
}

// === Path traversal through repo/uuid segments must fail ===

#[tokio::test]
async fn oci_repo_traversal_is_rejected() {
    let tmp = tmp_dir();
    let store = RegistryStore::new(tmp.path()).await.unwrap();

    // A digest that actually matches the payload "x" — so digest validation
    // passes and the traversal check is what rejects each evil repo.
    // Digest khớp thật với payload "x" — digest hợp lệ để traversal check
    // là phần từ chối từng repo độc ác.
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"x");
    let honest = format!("sha256:{}", hex::encode(hasher.finalize()));

    for evil in [
        "..",
        "../escape",
        "a/../b",
        "a/b/../../c",
        "a\\b",
        "",
        ".",
        "repo\x00null",
    ] {
        let res = store.put_oci_blob(evil, &honest, b"x").await;
        assert!(
            res.is_err(),
            "evil repo segment '{evil:?}' must be rejected"
        );
    }

    // Legal OCI path-style repo (multi-component) must still work.
    // Repo kiểu OCI nhiều component vẫn phải hoạt động.
    let ok = store.put_oci_blob("ai/mymodel", &honest, b"x").await;
    assert!(ok.is_ok(), "legal OCI repo path must be accepted");

    let res = store.create_oci_upload("../escape", "uuid-1").await;
    assert!(res.is_err(), "evil repo in upload session must be rejected");

    let res = store.create_oci_upload("my-repo", "../../etc/passwd").await;
    assert!(res.is_err(), "evil uuid in upload session must be rejected");

    let res = store.append_oci_upload("my-repo", "../evil", b"x").await;
    assert!(res.is_err(), "evil uuid in append must be rejected");
}

// === Atomic OCI blob writes (P0-7: no swallowed errors, recursive temp scan) ===

#[tokio::test]
async fn concurrent_oci_puts_same_digest_leave_valid_blob() {
    let tmp = tmp_dir();
    let store = std::sync::Arc::new(RegistryStore::new(tmp.path()).await.unwrap());

    let digest = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"oci-payload");
        format!("sha256:{}", hex::encode(hasher.finalize()))
    };

    // Every writer's result is COLLECTED and asserted (P0-7): a green test
    // must not hide a failed writer behind its siblings.
    // Kết quả MỖI writer được THU và assert (P0-7): test xanh không được
    // giấu writer fail sau lưng các writer khác.
    let handles: Vec<_> = (0..6)
        .map(|_| {
            let store = std::sync::Arc::clone(&store);
            let digest = digest.clone();
            std::thread::spawn(move || {
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(store.put_oci_blob("concurrent", &digest, b"oci-payload"))
            })
        })
        .collect();
    for (i, h) in handles.into_iter().enumerate() {
        let res = h.join().expect("writer thread must not panic");
        assert!(
            res.is_ok(),
            "concurrent OCI writer {i} must succeed (winner publishes, losers verify winner): {res:?}"
        );
    }

    // The served blob must be intact — get_oci_blob rehashes on serve.
    // Blob serve phải nguyên vẹn — get_oci_blob rehash khi phục vụ.
    let served = store.get_oci_blob("concurrent", &digest).await.unwrap();
    assert_eq!(served.as_deref(), Some(b"oci-payload".as_ref()));

    // No leftover temp files ANYWHERE under the store dir — recursive scan
    // (the old test only scanned the repo root and missed temps nested in
    // digest subdirectories).
    // Không sót temp file BẤT KỂ NƠI NÀO dưới thư mục store — quét đệ quy
    // (test cũ chỉ quét repo root nên sót temp lồng trong thư mục con
    // digest).
    let leftovers = scan_temp_files(tmp.path());
    assert!(
        leftovers.is_empty(),
        "atomic writes must not leave temp files (recursive scan): {leftovers:?}"
    );
}

// === P0-6: corrupt OCI blob must NOT be served (mirror of the npm test) ===

#[tokio::test]
async fn corrupted_oci_blob_fails_serving_and_is_quarantined() {
    let tmp = tmp_dir();
    let store = RegistryStore::new(tmp.path()).await.unwrap();

    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"oci-clean-payload");
    let digest = format!("sha256:{}", hex::encode(hasher.finalize()));

    store
        .put_oci_blob("corrupt-test", &digest, b"oci-clean-payload")
        .await
        .unwrap();

    // Corrupt the on-disk blob behind the DB's back (same attack the npm
    // blob test uses).
    // Sửa blob trên đĩa phía sau lưng DB (cùng kiểu tấn công như test blob
    // npm đang dùng).
    let row = store
        .oci_blob_path_for_test("corrupt-test", &digest)
        .await
        .unwrap()
        .expect("oci blob row exists");
    let path = std::path::PathBuf::from(row);
    assert!(path.exists(), "oci blob file must exist at {path:?}");
    std::fs::write(&path, b"CORRUPTED-OCI-BYTES").unwrap();

    // Serving must fail closed — corrupt bytes never reach the client.
    // Serve phải fail cứng — bytes hỏng không bao giờ tới client.
    let res = store.get_oci_blob("corrupt-test", &digest).await;
    assert!(
        res.is_err(),
        "corrupted OCI blob must not be served silently (P0-6)"
    );

    // The corrupt blob was quarantined for forensics.
    // Blob hỏng đã bị cách ly để điều tra.
    let qdir = tmp.path().join("quarantine");
    let quarantined: Vec<_> = std::fs::read_dir(&qdir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with("corrupt-oci-"))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        !quarantined.is_empty(),
        "corrupt OCI blob must land in quarantine/ for forensics"
    );

    // oci_blob_exists is disk-verified: the quarantined (moved) blob no
    // longer reports as existing.
    // oci_blob_exists verify trên đĩa: blob đã bị move đi thì không còn
    // báo tồn tại.
    let exists = store
        .oci_blob_exists("corrupt-test", &digest)
        .await
        .unwrap();
    assert!(!exists, "quarantined blob must not report as existing");
}

// === P0-5: local blob put is atomic — final path never holds partial bytes ===

#[tokio::test]
async fn concurrent_local_blob_puts_same_digest_all_succeed() {
    let tmp = tmp_dir();
    let store = std::sync::Arc::new(RegistryStore::new(tmp.path()).await.unwrap());

    use sha2::{Digest, Sha512};
    let mut hasher = Sha512::new();
    hasher.update(b"local-atomic-payload");
    let b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        hasher.finalize(),
    );
    let digest = format!("sha512-{b64}");

    // 6 concurrent puts of the same digest: every writer must succeed
    // (first publishes, losers reuse/verify) and the final path must hold
    // the complete payload.
    // 6 put song song cùng digest: mọi writer phải thành công (writer đầu
    // publish, thua thì reuse/verify) và path cuối phải giữ payload đầy đủ.
    let handles: Vec<_> = (0..6)
        .map(|_| {
            let store = std::sync::Arc::clone(&store);
            let digest = digest.clone();
            std::thread::spawn(move || {
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(store.put_blob(&digest, b"local-atomic-payload"))
            })
        })
        .collect();
    for (i, h) in handles.into_iter().enumerate() {
        let res = h.join().expect("writer thread must not panic");
        assert!(res.is_ok(), "local blob writer {i} failed: {res:?}");
    }

    let served = store.get_blob(&digest).await.unwrap();
    assert_eq!(
        served.as_deref(),
        Some(b"local-atomic-payload".as_ref()),
        "final blob must be complete after concurrent puts"
    );

    // No .put-* temp leftovers (recursive scan).
    // Không sót temp .put-* (quét đệ quy).
    let leftovers = scan_temp_files(tmp.path());
    assert!(leftovers.is_empty(), "temp leftovers: {leftovers:?}");
}

// === P1-6 audit vòng-3: blob_exists is typed — torn blobs are not hits ===

#[tokio::test]
async fn blob_exists_reports_torn_blob_as_length_mismatch() {
    let tmp = tmp_dir();
    let store = RegistryStore::new(tmp.path()).await.unwrap();

    use sha2::{Digest, Sha512};
    let mut hasher = Sha512::new();
    hasher.update(b"presence-payload-0123456789");
    let b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        hasher.finalize(),
    );
    let digest = format!("sha512-{b64}");

    store
        .put_blob(&digest, b"presence-payload-0123456789")
        .await
        .unwrap();

    // Healthy state: PresentLengthMatched (length only — NOT a digest
    // verify; the digest is checked at serve time, audit vòng-4 P1).
    // (Trạng thái khỏe: PresentLengthMatched (chỉ độ dài — KHÔNG phải
    // verify digest; digest được check lúc serve, P1 audit vòng-4).)
    assert_eq!(
        store.blob_exists(&digest).await.unwrap(),
        BlobPresence::PresentLengthMatched
    );

    // Missing digest: Missing.
    // (Digest không có: Missing.)
    let mut other = Sha512::new();
    other.update(b"never-stored");
    let ob64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, other.finalize());
    assert_eq!(
        store.blob_exists(&format!("sha512-{ob64}")).await.unwrap(),
        BlobPresence::Missing
    );

    // Truncate the blob behind the DB's back: the boolean API used to say
    // "true"; the typed API must say PresentLengthMismatch (needs repair,
    // never a cache hit).
    // (Cắt ngắn blob phía sau lưng DB: API boolean cũ trả "true"; API typed
    // phải trả PresentLengthMismatch (cần sửa, không bao giờ là cache hit).)
    let key = store
        .blob_path_for_test(&digest)
        .await
        .unwrap()
        .expect("row exists");
    let p = tmp.path().join("blobs").join(&key);
    let data = std::fs::read(&p).unwrap();
    std::fs::write(&p, &data[..data.len() / 2]).unwrap();

    assert_eq!(
        store.blob_exists(&digest).await.unwrap(),
        BlobPresence::PresentLengthMismatch,
        "torn blob must not report as a plain cache hit"
    );
}

// === Helpers ===

/// Recursively scan for atomic-write temp leftovers under a root dir.
/// Look for `.upload-*` (OCI puts) and `.put-*` (local blob puts) at ANY
/// depth — the old test missed temps nested in digest subdirectories and
/// silently passed on Windows rename leftovers.
///
/// Quét đệ quy tìm temp của ghi nguyên tử dưới thư mục gốc. Tìm `.upload-*`
/// (put OCI) và `.put-*` (put local) ở MỌI độ sâu — test cũ sót temp lồng
/// trong thư mục con digest và vẫn pass im lặng với rác rename trên Windows.
fn scan_temp_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(".upload-") || name.starts_with(".put-") {
                    found.push(path);
                }
            }
        }
    }
    found
}
