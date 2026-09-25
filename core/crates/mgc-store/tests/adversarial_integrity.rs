// Adversarial CAS integrity tests (P0-1 hardening).
// Test đối kháng cho CAS: hash sai dạng, blob bị sửa (mutation), quarantine,
// export KHÔNG là hardlink (project không ghi ngược vào store), concurrent
// writers, atomic write. Đối chiếu finding Tech Lead 2026-09-12.
#![allow(clippy::unwrap_used)] // Test code: unwrap acceptable for setup/assertions
// (Test code: unwrap chấp nhận được cho setup/assert)

use mgc_store::cas::Loader;
use mgc_store::cas::integrity::TarballEntry;
use mgc_store::cas::{CompilationKey, ContentStore, IntegrityHash, validate_blake3_hex};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn tmp_store_dir(name: &str) -> PathBuf {
    // Canonicalize: macOS temp dirs live under /var (a symlink); store roots
    // must be symlink-free, so tests resolve the real path first.
    // Canonicalize: temp dir macOS nằm dưới /var (symlink); store root phải
    // không chứa symlink nên test resolve đường dẫn thật trước.
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir().to_path_buf())
        .join("magicore-cas-adversarial")
        .join(format!(
            "{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

// === Hash contract (fail-closed, no panic) ===

#[test]
fn short_hash_string_is_rejected_not_panicking() {
    // "abc" previously sliced hash[..2] fine but Unicode/short input could
    // panic; now every malformed digest must return an Err.
    // Trước đây chuỗi ngắn/Unicode có thể panic qua hash[..2]; giờ mọi digest
    // sai dạng phải trả Err rõ ràng.
    for bad in [
        "",
        "a",
        "ab",
        "abc",
        "zzzz",
        "😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀",
        "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789",
        "sha256:abcdef0123456789",
        "blake3-0000000000000000000000000000000000000000000000000000000000000000",
        "0000000000000000000000000000000000000000000000000000000000000000extra",
    ] {
        let res = IntegrityHash::from_hash_str(bad, false);
        assert!(res.is_err(), "malformed digest '{bad}' must be rejected");
        assert!(validate_blake3_hex(bad).is_err());
    }
}

#[test]
fn valid_hash_string_is_accepted() {
    let good = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    assert!(IntegrityHash::from_hash_str(good, false).is_ok());
    assert!(validate_blake3_hex(good).is_ok());
}

// === Cache-hit rehash (corruption detection) ===

#[test]
fn tampered_existing_blob_fails_import_and_is_quarantined() {
    let root = tmp_store_dir("tampered-hit");
    let store = ContentStore::new(root.clone()).unwrap();
    let data = b"hello-cas".to_vec();

    let hash = store.import_bytes(&data).unwrap();
    let blob_path = hash.cas_path(&root);

    // Tamper with the stored blob directly on disk.
    // Sửa blob trên đĩa trực tiếp — mô phỏng store bị đầu độc.
    fs::write(&blob_path, b"POISONED!!!").unwrap();

    // A second import of the same content must fail-closed (not reuse).
    // Import lần 2 cùng nội dung phải fail cứng (không tái sử dụng blob bẩn).
    let res = store.import_bytes(&data);
    assert!(
        res.is_err(),
        "tampered cache hit must not be silently reused"
    );

    // The poisoned blob must have been moved to quarantine.
    // Blob bị đầu độc phải được chuyển vào quarantine.
    let qdir = root.join("quarantine");
    let entries: Vec<_> = fs::read_dir(&qdir)
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    assert!(
        !entries.is_empty(),
        "corrupt blob must land in quarantine for forensics"
    );

    // After quarantine, re-import succeeds (self-healing repair).
    // Sau khi bị cách ly, import lại thành công (tự sửa).
    let repaired = store.import_bytes(&data).unwrap();
    assert_eq!(repaired.as_hex(), hash.as_hex());
}

#[test]
fn import_with_declared_hash_mismatch_is_rejected() {
    let root = tmp_store_dir("declared-mismatch");
    let store = ContentStore::new(root.clone()).unwrap();

    let real = IntegrityHash::from_bytes(b"payload-A", false);
    let wrong = IntegrityHash::from_bytes(b"payload-B", false);

    let res = store.import_bytes_with_hash(b"payload-A", wrong.as_hex(), false);
    assert!(res.is_err(), "declared digest must match uploaded content");
    let _ = real;
}

// === Vòng-2 review R3: poisoned import must fail EVEN when the declared
// digest's blob already exists in the CAS (the cache-hit path must not
// become a poisoning bypass) ===
// (R3 vòng-2 review: import đầu độc phải fail KỂ CẢ khi blob của digest
// khai đã tồn tại trong CAS — đường cache-hit không được thành lỗ bypass
// đầu độc.)

#[test]
fn poisoned_import_rejected_even_when_declared_blob_exists() {
    let root = tmp_store_dir("poison-existing");
    let store = ContentStore::new(root.clone()).unwrap();

    // Legitimately store content-B first (its blob now exists in the CAS).
    // Lưu hợp lệ content-B trước (blob của nó giờ có trong CAS).
    let b = IntegrityHash::from_bytes(b"payload-B", false);
    store.import_bytes(b"payload-B").unwrap();
    assert!(b.cas_path(&root).exists());

    // Now try to import content-A while DECLARING digest-B. The cache-hit
    // shortcut must NOT verify blob-B and return Ok — the actual bytes (A)
    // never matched the declared digest (B). This is the R3 adversarial
    // gate.
    // Giờ thử import content-A nhưng KHAI digest-B. Đường tắt cache-hit
    // KHÔNG được verify blob-B rồi trả Ok — bytes thực (A) chưa bao giờ
    // khớp digest khai (B). Đây là cổng đối kháng R3.
    let res = store.import_bytes_with_hash(b"payload-A", b.as_hex(), false);
    assert!(
        res.is_err(),
        "poisoned import must be rejected at the adversarial gate even when the declared blob exists"
    );
}

#[test]
fn import_with_malformed_declared_hash_is_rejected() {
    let root = tmp_store_dir("malformed-declared");
    let store = ContentStore::new(root.clone()).unwrap();

    let res = store.import_bytes_with_hash(b"payload", "deadbeef", false);
    assert!(
        res.is_err(),
        "malformed declared digest must fail-closed, not panic"
    );
}

// === Export must NOT be a hardlink into the CAS (mutation isolation) ===

#[test]
fn exported_file_mutation_does_not_modify_store_blob() {
    let root = tmp_store_dir("export-isolation");
    let store = ContentStore::new(root.clone()).unwrap();

    let data = b"immutable-content".to_vec();
    let hash = store.import_bytes(&data).unwrap();

    let export_dir = root.join("export");
    fs::create_dir_all(&export_dir).unwrap();
    let dest = export_dir.join("linked.txt");

    store.export_to(&hash, &dest).unwrap();

    // Hardlink would share the inode — mutating the export must NOT change
    // the store blob. (The old hardlink export failed exactly here.)
    // Hardlink sẽ dùng chung inode — sửa file export KHÔNG được làm thay đổi
    // blob trong store. (Export hardlink cũ fail ngay tại đây.)
    let before = fs::read(hash.cas_path(&root)).unwrap();
    fs::write(&dest, b"MUTATED-BY-PROJECT").unwrap();
    let after = fs::read(hash.cas_path(&root)).unwrap();

    assert_eq!(
        before, after,
        "BUG: project-side mutation poisoned the shared store blob (writable hardlink still present?)"
    );
    assert_eq!(before, b"immutable-content".to_vec());

    // nlink check (unix): exported file must have link count 1.
    // Kiểm tra nlink (unix): file export phải có link count 1.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = fs::metadata(&dest).unwrap();
        assert_eq!(
            meta.nlink(),
            1,
            "exported file must be an independent copy, not a hardlink"
        );
    }
}

#[test]
fn export_twice_is_idempotent() {
    let root = tmp_store_dir("export-idempotent");
    let store = ContentStore::new(root.clone()).unwrap();
    let hash = store.import_bytes(b"same-bytes").unwrap();

    let dest = root.join("out").join("file.txt");
    store.export_to(&hash, &dest).unwrap();
    store.export_to(&hash, &dest).unwrap();
    assert_eq!(fs::read(&dest).unwrap(), b"same-bytes".to_vec());
}

#[test]
fn export_missing_blob_fails_not_found() {
    let root = tmp_store_dir("export-missing");
    let store = ContentStore::new(root.clone()).unwrap();
    let ghost = IntegrityHash::from_bytes(b"never-imported", false);
    let res = store.export_to(&ghost, &root.join("nowhere.txt"));
    assert!(res.is_err());
}

// === Tarball entries dedup also re-verifies ===

#[test]
fn tampered_tarball_dedup_entry_fails() {
    let root = tmp_store_dir("tarball-dedup");
    let store = ContentStore::new(root.clone()).unwrap();

    let data = b"tar-entry".to_vec();
    let entries = vec![TarballEntry {
        path: "a.txt".into(),
        data: data.clone(),
        executable: false,
    }];
    store.import_tarball_entries(entries).unwrap();

    let hash = IntegrityHash::from_bytes(&data, false);
    fs::write(hash.cas_path(&root), b"corrupted").unwrap();

    let entries2 = vec![TarballEntry {
        path: "a.txt".into(),
        data,
        executable: false,
    }];
    let res = store.import_tarball_entries(entries2);
    assert!(res.is_err(), "dedup path must re-verify stored content");
}

// === Concurrent writers (unique temp files, no clobber) ===

#[test]
fn concurrent_imports_same_content_all_succeed() {
    let root = tmp_store_dir("concurrent-import");
    let store = Arc::new(ContentStore::new(root).unwrap());
    let data: Vec<u8> = b"concurrent-payload-0123456789".to_vec();

    let handles: Vec<_> = (0..8)
        .map(|i| {
            let store = Arc::clone(&store);
            let data = data.clone();
            std::thread::spawn(move || {
                let h = store.import_bytes(&data).unwrap();
                Some((i, h.as_hex().to_string()))
            })
        })
        .collect();

    let mut hashes = Vec::new();
    for h in handles {
        hashes.push(h.join().unwrap().unwrap());
    }
    let first = hashes[0].1.clone();
    assert!(
        hashes.iter().all(|(_, h)| *h == first),
        "all concurrent imports must agree on the digest"
    );
}

#[test]
fn concurrent_compiled_cache_puts_never_collide() {
    let root = tmp_store_dir("compiled-concurrent");
    let store = ContentStore::new(root.clone()).unwrap();
    let cache = store.compiled_cache();

    // Full compilation key (P0-1 audit vòng-4): context-full, not
    // source-only.
    // (Key biên dịch đầy đủ (P0-1 audit vòng-4): đủ ngữ cảnh, không chỉ
    // source.)
    let key = CompilationKey::new(
        // blake3 hex of b"source" — stable content digest for the key.
        // (hex blake3 của b"source" — digest nội dung ổn định cho key.)
        "6b84be2b6c09bf573b8e8c663182b1e7d0d5e79ba1cd9e2d5a6f4d3e3f2a1b0c",
        Loader::Tsx,
        "esbuild-rs",
        "0.13.8",
        "{}",
    )
    .unwrap();
    let cache = Arc::new(cache);
    // Audit vòng-3 P0-5: every writer's result is COLLECTED and asserted —
    // a green test must not hide a failed writer behind its siblings, and
    // no temp may be left behind at ANY depth.
    // (P0-5 audit vòng-3: kết quả MỖI writer được THU và assert — test xanh
    // không được giấu writer fail sau lưng, và không temp sót ở MỌI độ sâu.)
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let cache = Arc::clone(&cache);
            let key = key.clone();
            std::thread::spawn(move || {
                let module = mgc_store::cas::CompiledModule {
                    js: "console.log(1)".into(),
                    source_map: None,
                };
                cache.put(&key, &module)
            })
        })
        .collect();
    for (i, h) in handles.into_iter().enumerate() {
        let res = h.join().expect("compiled-cache writer must not panic");
        assert!(
            res.is_ok(),
            "compiled-cache writer {i} must succeed (atomic put, loser verifies winner): {res:?}"
        );
    }
    // The surviving entry is valid JSON with the right payload.
    // (Entry sống sót là JSON hợp lệ với payload đúng.)
    let module = cache.get(&key).unwrap().expect("entry must exist");
    assert_eq!(module.js, "console.log(1)");
    // Recursive temp scan under the whole store root.
    // (Quét temp đệ quy toàn bộ gốc store.)
    let leftovers = scan_store_temps(&root);
    assert!(
        leftovers.is_empty(),
        "compiled-cache temp leftovers: {leftovers:?}"
    );
}

#[test]
fn compiled_cache_rejects_corrupt_json_at_get() {
    // A torn/corrupt cache entry must fail at GET with a hard error — never
    // be silently served as a compiled module.
    // (Entry cache đứt/hỏng phải fail tại GET với lỗi cứng — không bao giờ
    // được phục vụ im lặng như một compiled module.)
    let root = tmp_store_dir("compiled-corrupt");
    let store = ContentStore::new(root.clone()).unwrap();
    let cache = store.compiled_cache();
    // Full compilation key (P0-1 audit vòng-4).
    // (Key biên dịch đầy đủ (P0-1 audit vòng-4).)
    let key = CompilationKey::new(
        "a1b2c3d4e5f60718a9b0c1d2e3f40516a7b8c9d0e1f2a3b4c5d6e7f80910a1b2",
        Loader::Ts,
        "esbuild-rs",
        "0.13.8",
        "{}",
    )
    .unwrap();

    let module = mgc_store::cas::CompiledModule {
        js: "ok".into(),
        source_map: None,
    };
    cache.put(&key, &module).unwrap();

    // Corrupt the entry behind the cache's back (path from the key's
    // master digest, mirroring module_path).
    // (Sửa entry sau lưng cache (path từ master digest của key, phản chiếu
    // module_path).)
    let master = key.master_digest();
    let hex = master.as_hex();
    let algo = root.join("compiled").join("blake3");
    let path = algo.join(&hex[..2]).join(format!("{hex}.json"));
    fs::write(&path, b"{TORN-JSON").unwrap();

    let res = cache.get(&key);
    assert!(
        res.is_err(),
        "corrupt compiled-cache JSON must fail at get()"
    );
}

// === Memo adversarial tests (audit vòng-3 P0-1) ===

#[test]
fn memo_survives_nothing_same_length_rewrite_with_mtime_restore() {
    // Bypass A (audit): rewrite the blob with DIFFERENT same-length content
    // then RESTORE the original mtime — the memo must still be invalidated
    // (ctime is kernel-maintained and changes on every content write).
    // (Bypass A (audit): ghi đè blob bằng nội dung KHÁC cùng độ dài rồi PHỤC
    // HỒI mtime cũ — memo vẫn phải vô hiệu (ctime do kernel giữ và đổi
    // theo mỗi lần ghi nội dung).)
    let root = tmp_store_dir("memo-mtime-restore");
    let store = ContentStore::new(root.clone()).unwrap();
    let data = b"memo-atomic-identity-A".to_vec();
    let hash = store.import_bytes(&data).unwrap();

    let out1 = root.join("e1.bin");
    store.export_to(&hash, &out1).unwrap(); // verify + memoize

    // Attack: same-length rewrite + restore mtime (ctime cannot be
    // restored from user space — this is the whole point of the identity).
    // (Tấn công: ghi đè cùng độ dài + phục hồi mtime (ctime không thể
    // phục hồi từ user space — đây chính là ý nghĩa của identity).)
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let path = hash.cas_path(&root);
        let meta = fs::metadata(&path).unwrap();
        let saved_secs = meta.mtime();
        let saved_nanos = meta.mtime_nsec() as i64;
        let poison = b"memo-atomic-identity-X".to_vec(); // same length
        assert_eq!(poison.len(), data.len());
        fs::write(&path, &poison).unwrap();

        // Restore mtime exactly via utimensat — the SAME syscall an
        // attacker with file-write access would use (macOS touch has no
        // @epoch; utimensat is the ground-truth attack primitive).
        // (Phục hồi mtime chính xác qua utimensat — cùng syscall mà attacker
        // có quyền ghi file sẽ dùng (touch macOS không có @epoch; utimensat
        // là nguyên thủy tấn công chuẩn thực tế).)
        #[allow(unsafe_code)]
        fn restore_mtime(path: &Path, secs: i64, nanos: i64) {
            use std::ffi::CString;
            let times = [
                libc::timespec {
                    tv_sec: secs,
                    tv_nsec: nanos,
                },
                libc::timespec {
                    tv_sec: secs,
                    tv_nsec: nanos,
                },
            ];
            let c = CString::new(path.as_os_str().to_string_lossy().as_bytes()).unwrap();
            // SAFETY: utimensat with absolute path, plain timespec array by
            // reference, UTIME_OMIT not used — thin syscall, no memory
            // reinterpretation; kernel validates the path.
            // (An toàn: utimensat với path tuyệt đối, mảng timespec thuần
            // theo tham chiếu, không dùng UTIME_OMIT — syscall mỏng, không
            // diễn giải lại bộ nhớ; kernel tự validate path.)
            let ret = unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), 0) };
            assert_eq!(ret, 0, "attack setup: utimensat must succeed");
        }
        restore_mtime(&path, saved_secs, saved_nanos);

        // mtime really restored (attack is well-formed).
        // (mtime thực sự được phục hồi (tấn công hợp lệ).)
        let after = fs::metadata(&path).unwrap();
        assert_eq!(
            after.mtime(),
            saved_secs,
            "attack precondition: mtime restored"
        );
    }

    let out2 = root.join("e2.bin");
    let res = store.export_to(&hash, &out2);
    assert!(
        res.is_err(),
        "Bypass A: same-length rewrite + mtime restore must NOT pass the memo (ctime changed)"
    );
    assert!(!out2.exists(), "no export may be produced from poison");
}

#[test]
fn memo_exec_and_nonexec_digests_do_not_share_verification() {
    // Bypass B (audit): the exec and non-exec variants of one digest are
    // DISTINCT CAS files; tampering the non-exec blob must not be masked by
    // a memo entry keyed only on the hex digest. The memo key now includes
    // the executable bit, so each variant verifies independently.
    // (Bypass B (audit): biến thể exec và non-exec của cùng digest là 2 file
    // CAS KHÁC NHAU; sửa blob non-exec không được che bởi memo key chỉ theo
    // hex digest. Memo key giờ gồm bit executable nên mỗi biến thể verify
    // độc lập.)
    let root = tmp_store_dir("memo-exec-split");
    let store = ContentStore::new(root.clone()).unwrap();
    let data = b"shared-bytes-for-exec-split".to_vec();

    // Import the same bytes as BOTH variants (different CAS paths).
    // (Import cùng bytes ở CẢ 2 biến thể (path CAS khác nhau).)
    let exec_hash = store.import_bytes_with_exec(&data, true).unwrap();
    let plain_hash = store.import_bytes_with_exec(&data, false).unwrap();
    assert_ne!(exec_hash.cas_path(&root), plain_hash.cas_path(&root));

    // Verify+memoize BOTH variants through export.
    // (Verify + memo hóa CẢ 2 biến thể qua export.)
    let o1 = root.join("x1").join("a");
    let o2 = root.join("x2").join("a");
    store.export_to(&exec_hash, &o1).unwrap();
    store.export_to(&plain_hash, &o2).unwrap();

    // Tamper the NON-EXEC blob on disk.
    // (Sửa blob NON-EXEC trên đĩa.)
    fs::write(plain_hash.cas_path(&root), b"TAMPERED-NON-EXEC-BYTES").unwrap();

    // Export of the non-exec variant must FAIL (fresh verify catches it)…
    // (Export biến thể non-exec phải FAIL (verify mới bắt được)…)
    let o3 = root.join("x3").join("a");
    let res = store.export_to(&plain_hash, &o3);
    assert!(
        res.is_err(),
        "tampered non-exec blob must fail its own verify"
    );

    // …while the exec variant's memo/file is unrelated and still valid.
    // (…trong khi memo/file của biến thể exec không liên quan và còn hợp lệ.)
    let o4 = root.join("x4").join("a");
    store.export_to(&exec_hash, &o4).unwrap();
    assert_eq!(fs::read(&o4).unwrap(), data);
}

#[test]
fn memo_catches_file_replacement_by_rename() {
    // Replace the blob file with a DIFFERENT file (new inode) carrying
    // attacker content — inode is part of the identity, the memo dies.
    // (Thay file blob bằng file KHÁC (inode mới) mang nội dung tấn công —
    // inode nằm trong identity nên memo chết.)
    let root = tmp_store_dir("memo-rename-replace");
    let store = ContentStore::new(root.clone()).unwrap();
    let data = b"identity-rename-original".to_vec();
    let hash = store.import_bytes(&data).unwrap();

    let out1 = root.join("r1.bin");
    store.export_to(&hash, &out1).unwrap(); // verify + memoize

    // Swap the blob by rename (same length is not even needed).
    // (Đổi blob bằng rename (không cần cùng độ dài).)
    let path = hash.cas_path(&root);
    let fake = root.join(".fake-replacement");
    fs::write(&fake, b"identity-rename-attacker!").unwrap();
    fs::rename(&fake, &path).unwrap();

    let out2 = root.join("r2.bin");
    let res = store.export_to(&hash, &out2);
    assert!(
        res.is_err(),
        "file replacement by rename must invalidate the memo (inode changed)"
    );
}

/// Recursively scan for atomic-write temp leftovers under a store root
/// (compiled-cache temps are `compiled-*`).
/// (Quét đệ quy temp của ghi nguyên tử dưới gốc store (temp compiled-cache
/// là `compiled-*`).)
fn scan_store_temps(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("compiled-") || name.starts_with("mgc-export-") {
                    found.push(path);
                }
            }
        }
    }
    found
}

// === Staging branch is LIVE (audit vòng-3 P0-2: prove CowClone runs) ===

#[test]
fn staging_kind_probe_matches_platform_capability() {
    // Assert the ACTUAL branch used on THIS filesystem — a fallback that
    // silently never runs is a false-green (audit vòng-3 P0-2).
    // On APFS (stock macOS) the probe MUST report CowClone; on a
    // non-reflink filesystem it must report PlainCopy. Either way the
    // answer must be non-accidental and stable.
    // (Assert branch THẬT được dùng trên filesystem NÀY — fallback im lặng
    // không chạy là false-green (P0-2 audit vòng-3). Trên APFS (macOS
    // chuẩn) probe PHẢI báo CowClone; trên filesystem không reflink phải
    // báo PlainCopy. Cách nào thì câu trả lời cũng không được ngẫu nhiên và
    // phải ổn định.)
    let root = tmp_store_dir("staging-probe");
    let store = ContentStore::new(root.clone()).unwrap();
    let hash = store.import_bytes(b"probe-payload").unwrap();

    let probe_dir = root.join("probe-dest");
    fs::create_dir_all(&probe_dir).unwrap();
    let kind = store.staging_kind_probe(&hash, &probe_dir);

    #[cfg(target_os = "macos")]
    {
        // /tmp on stock macOS is APFS → clonefile works.
        // (/tmp trên macOS chuẩn là APFS → clonefile chạy.)
        assert_eq!(
            kind,
            mgc_store::cas::StagingKind::CowClone,
            "APFS must use the clonefile COW branch (fallback silently dead = false-green)"
        );
    }
    #[cfg(target_os = "linux")]
    {
        // CI btrfs/XFS → CowClone; tmpfs/ext4 → PlainCopy. Both are valid;
        // assert consistency instead of a fixed value.
        // (CI btrfs/XFS → CowClone; tmpfs/ext4 → PlainCopy. Cả hai hợp lệ;
        // assert tính nhất quán thay vì giá trị cố định.)
        let again = store.staging_kind_probe(&hash, &probe_dir);
        assert_eq!(kind, again, "probe must be deterministic per filesystem");
    }
    // No probe leftovers.
    // (Không sót probe temp.)
    let leftovers: Vec<_> = fs::read_dir(&probe_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("mgc-export-"))
        .collect();
    assert!(leftovers.is_empty(), "probe temp leftovers: {leftovers:?}");
}

// === P1-3 audit vòng-3: existing destination gets its mode repaired ===

#[cfg(unix)]
#[test]
fn export_to_existing_destination_repairs_executable_mode() {
    // Export an EXECUTABLE blob onto an existing 0644 destination with the
    // SAME bytes: export_to must succeed AND repair the mode to 0755 —
    // identical bytes alone are not a complete export (audit P1-3).
    // (Export blob EXECUTABLE lên đích 0644 đã tồn tại với CÙNG bytes:
    // export_to phải thành công VÀ sửa mode thành 0755 — bytes giống nhau
    // một mình chưa phải export hoàn chỉnh (P1-3 audit).)
    use std::os::unix::fs::PermissionsExt;
    let root = tmp_store_dir("exec-mode-repair");
    let store = ContentStore::new(root.clone()).unwrap();
    let hash = store
        .import_bytes_with_exec(b"#!/bin/sh\necho hi\n", true)
        .unwrap();

    let dest = root.join("out").join("script.sh");
    fs::create_dir_all(dest.parent().unwrap()).unwrap();
    fs::write(&dest, b"#!/bin/sh\necho hi\n").unwrap(); // same bytes, 0644
    fs::set_permissions(&dest, fs::Permissions::from_mode(0o644)).unwrap();

    store.export_to(&hash, &dest).unwrap();

    let mode = fs::metadata(&dest).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o755,
        "existing destination mode must be repaired to 0755"
    );
}

#[test]
fn export_destination_under_symlinked_dir_is_rejected() {
    let root = tmp_store_dir("symlink-ancestor");
    let store = ContentStore::new(root.clone()).unwrap();

    let hash = store.import_bytes(b"symlinked-target").unwrap();

    let real_dir = root.join("real");
    let link_dir = root.join("linkdir");
    fs::create_dir_all(&real_dir).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real_dir, &link_dir).unwrap();

    let dest = link_dir.join("escape.txt");
    let res = store.export_to(&hash, &dest);
    assert!(
        res.is_err(),
        "export through a symlinked directory must be refused"
    );
}

// === P0-1: private fields — no public construction bypass ===

#[test]
fn typed_hash_fields_are_not_publicly_accessible() {
    // Compile-time proof lives in tests/compile_fail/ (trybuild-style):
    // `IntegrityHash { hash: "x".into(), executable: false }` must NOT
    // compile outside the crate. At runtime we prove the *reflection-free*
    // invariant: every public constructor rejects malformed digests and the
    // accessors are the only way to read the value back.
    // Bằng chứng compile nằm ở tests/compile_fail/ (kiểu trybuild):
    // struct literal ngoài crate KHÔNG compile được. Runtime chứng minh
    // invariant: mọi constructor public từ chối digest sai, accessor là
    // đường đọc duy nhất.
    let h = IntegrityHash::from_bytes(b"data", true);
    assert_eq!(h.as_hex().len(), 64);
    assert!(h.is_executable());

    let parsed =
        IntegrityHash::try_from("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
            .unwrap();
    assert!(!parsed.is_executable());
    assert_eq!(parsed.as_hex().len(), 64);

    // TryFrom must reject everything from_hash_str rejects (same contract).
    // TryFrom phải từ chối mọi thứ from_hash_str từ chối (cùng hợp đồng).
    for bad in [
        "",
        "x",
        "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789",
    ] {
        assert!(IntegrityHash::try_from(bad).is_err(), "must reject '{bad}'");
    }
}

// === P0-2: export staging must live in the DESTINATION filesystem ===

#[test]
fn export_temp_is_created_in_destination_filesystem() {
    // Simulate two "filesystems": the store root dir and a separate export
    // dir. The staging temp (.mgc-export-*) must appear NEXT TO the
    // destination — never under the CAS tmp dir — so the final rename is
    // same-filesystem (EXDEV cannot happen).
    // Mô phỏng 2 "filesystem": store root và thư mục export riêng. Temp
    // staging (.mgc-export-*) phải nằm CẠNH đích — không bao giờ dưới tmp
    // của CAS — để rename cuối cùng filesystem (kh thể EXDEV).
    let root = tmp_store_dir("export-fs");
    let store = ContentStore::new(root.clone()).unwrap();
    let hash = store.import_bytes(b"cross-fs-payload").unwrap();

    let export_dir = root.join("project-fs"); // distinct subtree = distinct "volume"
    fs::create_dir_all(&export_dir).unwrap();
    let dest = export_dir.join("out.txt");

    store.export_to(&hash, &dest).unwrap();

    assert_eq!(fs::read(&dest).unwrap(), b"cross-fs-payload");
    // No staging leftovers anywhere (destination side AND CAS tmp side).
    // Không còn staging sót ở bất kỳ đâu (cả phía đích lẫn tmp CAS).
    let cas_tmp: Vec<_> = fs::read_dir(root.join("tmp"))
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    assert!(
        cas_tmp.is_empty(),
        "CAS tmp must be clean, found {cas_tmp:?}"
    );
    let dest_dir: Vec<_> = fs::read_dir(&export_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("mgc-export-"))
        .collect();
    assert!(
        dest_dir.is_empty(),
        "no .mgc-export-* staging leftovers allowed, found {dest_dir:?}"
    );
}

// === P0-2: export failure cleans the staging temp ===

#[test]
fn export_failure_leaves_no_staging_temp() {
    let root = tmp_store_dir("export-cleanup");
    let store = ContentStore::new(root.clone()).unwrap();
    let hash = store.import_bytes(b"cleanup-payload").unwrap();

    let export_dir = root.join("dest");
    fs::create_dir_all(&export_dir).unwrap();

    // Destination already exists with DIFFERENT content → hard error, and
    // the pre-existing file must be untouched (no staging leak).
    // Đích đã tồn tại với nội dung KHÁC → lỗi cứng, file cũ phải nguyên vẹn.
    let dest = export_dir.join("exists.bin");
    fs::write(&dest, b"other-content").unwrap();
    let res = store.export_to(&hash, &dest);
    assert!(res.is_err());

    assert_eq!(fs::read(&dest).unwrap(), b"other-content");
    let leftovers: Vec<_> = fs::read_dir(&export_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("mgc-export-"))
        .collect();
    assert!(leftovers.is_empty(), "staging leaked: {leftovers:?}");
}

// === P0-3: tarball import is atomic — 8 concurrent writers, one digest ===

#[test]
fn concurrent_tarball_import_same_content_is_atomic() {
    // 8 threads import the SAME tarball entry concurrently: every writer
    // must either publish the blob (first) or verify the winner (others) —
    // no AlreadyExists errors, no partial blob at the final CAS path.
    // 8 thread import CÙNG entry tarball: writer đầu publish, 7 writer kia
    // verify winner — không lỗi AlreadyExists, không blob nửa vời ở path
    // CAS cuối.
    let root = tmp_store_dir("tarball-atomic");
    let store = Arc::new(ContentStore::new(root.clone()).unwrap());

    let data = b"tarball-atomic-payload-0123456789".to_vec();
    let entries = || {
        vec![TarballEntry {
            path: "pkg/a.txt".into(),
            data: data.clone(),
            executable: false,
        }]
    };

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let store = Arc::clone(&store);
            let entries = entries();
            std::thread::spawn(move || store.import_tarball_entries(entries))
        })
        .collect();

    // Every writer must SUCCEED (winner publishes, losers verify winner).
    // Mọi writer phải THÀNH CÔNG (winner publish, thua verify winner).
    for (i, h) in handles.into_iter().enumerate() {
        let res = h.join().unwrap();
        assert!(
            res.is_ok(),
            "tarball writer {i} failed race handling: {:?}",
            res.err()
        );
    }

    // The final blob is complete and correct.
    // Blob cuối hoàn chỉnh và đúng.
    let hash = IntegrityHash::from_bytes(&data, false);
    let blob = fs::read(hash.cas_path(&root)).unwrap();
    assert_eq!(blob, data);

    // No leftover temps in the CAS tmp dir.
    // Không còn temp sót trong tmp CAS.
    let leftovers: Vec<_> = fs::read_dir(root.join("tmp"))
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with("write-bytes"))
                .collect()
        })
        .unwrap_or_default();
    assert!(leftovers.is_empty(), "CAS tmp leftovers: {leftovers:?}");
}

// === P0-4: COW export (clonefile/FICLONE) stays an independent file ===

#[test]
fn cow_or_copy_export_is_independent_from_store() {
    // Whether the export came from clonefile (APFS COW), FICLONE (reflink)
    // or plain copy — mutating the export must NEVER modify the CAS blob.
    // This is the poisoning test re-run against the COW strategy.
    // Dù export đến từ clonefile (APFS COW), FICLONE (reflink) hay copy
    // thường — sửa file export KHÔNG BAO GIỜ làm thay đổi blob CAS.
    // Đây là test đầu độc chạy lại trên chiến lược COW.
    let root = tmp_store_dir("cow-independence");
    let store = ContentStore::new(root.clone()).unwrap();
    let hash = store.import_bytes(b"cow-payload-immutable").unwrap();

    let export_dir = root.join("project");
    fs::create_dir_all(&export_dir).unwrap();
    let dest = export_dir.join("cow.txt");
    store.export_to(&hash, &dest).unwrap();

    let before = fs::read(hash.cas_path(&root)).unwrap();
    // Mutate the export the way a build tool would.
    // Sửa file export như một build tool vẫn làm.
    fs::write(&dest, b"MUTATED-COW-EXPORT").unwrap();
    let after = fs::read(hash.cas_path(&root)).unwrap();

    assert_eq!(
        before, after,
        "COW export leaked writes into the shared CAS blob"
    );
    assert_eq!(before, b"cow-payload-immutable".to_vec());

    #[cfg(unix)]
    {
        // nlink=1 proves independence for hardlinks; COW clones may share
        // extents with nlink=1, so the WRITE-through test above is the real
        // proof (extent copy-on-write keeps them independent).
        // nlink=1 chứng minh độc lập với hardlink; clone COW có thể chia sẻ
        // extent với nlink=1, nên test ghi-qua phía trên mới là bằng chứng
        // thật (extent copy-on-write giữ chúng độc lập).
        use std::os::unix::fs::MetadataExt;
        let meta = fs::metadata(&dest).unwrap();
        assert_eq!(meta.nlink(), 1, "export must never be a hardlink");
    }
}

// === P0-4: memo holds until the file changes — mutation re-verifies ===

#[test]
fn verified_memo_invalidates_on_disk_mutation() {
    // Same-process scenario: export #1 verifies the blob (memo recorded);
    // an attacker then rewrites the blob on disk; export #2 must FAIL —
    // the (len, mtime) fingerprint changed, the memo is dead, the fresh
    // verify catches the poison and quarantines it.
    // Kịch bản cùng-process: export #1 verify blob (ghi memo); kẻ tấn công
    // ghi đè blob trên đĩa; export #2 phải FAIL — fingerprint (len, mtime)
    // đã đổi, memo chết, verify mới bắt được độc và cách ly.
    let root = tmp_store_dir("memo-invalidate");
    let store = ContentStore::new(root.clone()).unwrap();
    let data = b"memo-protected-payload".to_vec();
    let hash = store.import_bytes(&data).unwrap();

    let out1 = root.join("e1").join("f.bin");
    store.export_to(&hash, &out1).unwrap(); // verifies + memoizes
    assert_eq!(fs::read(&out1).unwrap(), data);

    // Tamper directly on disk (mtime + content change).
    // Sửa trực tiếp trên đĩa (mtime + nội dung đổi).
    fs::write(hash.cas_path(&root), b"POISONED-AFTER-MEMO").unwrap();

    let out2 = root.join("e2").join("f.bin");
    let res = store.export_to(&hash, &out2);
    assert!(
        res.is_err(),
        "memo must NOT survive an on-disk mutation — poisoned blob must fail export"
    );
    assert!(!out2.exists(), "no export may be produced from poison");

    // The blob was quarantined + a re-import repairs it (self-healing).
    // Blob bị cách ly + import lại sẽ tự sửa.
    let repaired = store.import_bytes(&data).unwrap();
    assert_eq!(repaired.as_hex(), hash.as_hex());
    let out3 = root.join("e3").join("f.bin");
    store.export_to(&hash, &out3).unwrap();
    assert_eq!(fs::read(&out3).unwrap(), data);
}

// === P0-4: same-mtime attack — truncation to the same length is caught ===

#[test]
fn memo_catches_rewrite_with_identical_length() {
    // Same-length rewrite still changes mtime (write updates it on every
    // mainstream filesystem), so the memo must invalidate and the fresh
    // verify must fail-closed.
    // Ghi đè cùng độ dài vẫn đổi mtime (ghi luôn cập nhật trên mọi filesystem
    // phổ biến), nên memo phải vô hiệu và verify mới phải fail-cứng.
    let root = tmp_store_dir("memo-samelen");
    let store = ContentStore::new(root.clone()).unwrap();
    let data = b"same-length-aaaa".to_vec();
    let hash = store.import_bytes(&data).unwrap();

    let out1 = root.join("a1.bin");
    store.export_to(&hash, &out1).unwrap();

    // Overwrite with SAME length, different bytes.
    // Ghi đè CÙNG độ dài, bytes khác.
    fs::write(hash.cas_path(&root), b"same-length-bbbb").unwrap();

    let out2 = root.join("a2.bin");
    let res = store.export_to(&hash, &out2);
    assert!(
        res.is_err(),
        "same-length rewrite must be caught by the mtime half of the fingerprint"
    );
}

#[test]
fn repeated_exports_stay_correct_with_cow() {
    // Multiple exports of the same blob (dedup scenario) must each be a
    // correct independent copy — COW clone per destination, re-verified.
    // Export nhiều lần cùng blob (kịch bản dedup) — mỗi bản phải là copy
    // độc lập đúng — clone COW cho từng đích, có re-verify.
    let root = tmp_store_dir("cow-repeat");
    let store = ContentStore::new(root.clone()).unwrap();
    let hash = store.import_bytes(b"repeat-export").unwrap();

    for i in 0..3 {
        let dir = root.join(format!("p{i}"));
        let dest = dir.join("file.txt");
        store.export_to(&hash, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"repeat-export");
        // Re-export to the SAME path (idempotent verify path).
        // Export lại CÙNG path (đường verify idempotent).
        store.export_to(&hash, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"repeat-export");
    }
}

// === P0-3 (vòng-10): CAS root metadata phải fail-closed — không nuốt lỗi ===

#[test]
#[cfg(unix)]
fn cas_root_unreadable_metadata_fails_closed() {
    // P0-3 (adversarial review vòng-9): the OLD code fell through
    // `else if !root.exists()` — `Path::exists()` SWALLOWS the metadata
    // error, so a permission-denied root read as "missing" and the
    // store proceeded to create-and-trust. Contract: `symlink_metadata`
    // failing with anything but NotFound on the ROOT is a hard error
    // naming the cause.
    // (P0-3: code cũ rơi qua `else if !root.exists()` — `Path::exists()`
    // NUỐT lỗi metadata nên root bị từ chối quyền bị đọc thành "mất" và
    // store tiếp tục tạo-rồi-tin. Hợp đồng: `symlink_metadata` lỗi gì
    // KHÁC NotFound trên ROOT là lỗi cứng nêu rõ nguyên nhân.)
    use std::os::unix::fs::PermissionsExt;

    let root = tmp_store_dir("root-metadata-deny");
    let denied = root.join("denied");
    fs::create_dir_all(&denied).unwrap();
    fs::set_permissions(&denied, fs::Permissions::from_mode(0o000)).unwrap();

    let store_root = denied.join("store");
    let err = mgc_store::cas::validate_cas_root(&store_root).unwrap_err();
    let msg = match &err {
        mgc_store::cas::StoreError::Io { msg, .. } => msg.clone(),
        other => panic!("expected Io error, got {other:?}"),
    };
    assert!(
        msg.to_lowercase().contains("metadata"),
        "a permission-denied root must fail with a metadata error, got: {msg}"
    );

    // Restore for tempdir cleanup.
    // (Khôi phục permission để tempdir dọn được.)
    fs::set_permissions(&denied, fs::Permissions::from_mode(0o755)).unwrap();
}
