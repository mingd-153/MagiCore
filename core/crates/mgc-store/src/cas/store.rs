// CAS content store: immutable content-addressed blob storage.
// Store CAS: blob content-addressed bất biến — mọi lần đọc/ghép lại đều
// rehash (fail-closed khi cache bị hỏng/sửa), export bằng copy/clone độc
// lập KHÔNG dùng hardlink (chống project làm bẩn shared store), temp luôn
// nằm CÙNG filesystem với đích rồi mới atomic-rename (P0-2 anti-EXDEV).

use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::integrity::{IntegrityHash, TarballEntry, validate_blake3_hex};
use super::lifecycle::{ensure_cas_dirs, set_cas_root_permissions, validate_cas_root};
use super::security::check_symlink_ancestors;
use super::write::{
    STREAM_THRESHOLD, stream_write_verify_and_set_perms, sync_parent_dir, unique_tmp_path,
    write_all_verify_and_set_perms,
};
use crate::cas::integrity;

#[derive(Debug)]
pub enum StoreError {
    Io {
        path: PathBuf,
        msg: String,
    },
    NotFound(String),
    HashMismatch {
        expected: String,
        actual: String,
    },
    InvalidHash(String),
    /// Two writers published DIVERGENT outputs for the same compilation key
    /// (P0-2 audit vòng-4): the valid winner stays in place, the loser gets
    /// a typed conflict — never a faked Ok, never a silent overwrite.
    ///
    /// (Hai writer publish output PHÂN KỲ cho cùng compilation key (P0-2
    /// audit vòng-4): winner hợp lệ giữ nguyên vị trí, bên thua nhận
    /// conflict typed — không giả Ok, không đè im lặng.)
    CacheConflict {
        path: PathBuf,
        expected: String,
        winner: String,
    },
    /// A compiled-cache entry failed self-validation (P0-3 audit vòng-4)
    /// — key/digest verification failed (poison/tamper) or the bytes are
    /// torn/unparseable. The corrupt bytes are quarantined for forensics;
    /// the caller must fail closed (dev server: treat as a miss, recompile).
    ///
    /// (Entry compiled-cache rơi khỏi bước tự xác thực (P0-3 audit vòng-4)
    /// — verify key/digest fail (đầu độc/gian lận) hoặc bytes đứt/không
    /// parse được. Bytes hỏng bị cách ly để điều tra; caller phải fail cứng
    /// (dev server: coi là miss, biên dịch lại).)
    CacheCorrupt {
        path: PathBuf,
        detail: String,
    },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, msg } => write!(f, "IO error at {}: {msg}", path.display()),
            Self::NotFound(hash) => write!(f, "content not found: {hash}"),
            Self::HashMismatch { expected, actual } => {
                write!(f, "hash mismatch: expected {expected}, got {actual}")
            }
            Self::InvalidHash(input) => write!(f, "invalid blake3 digest: {input}"),
            Self::CacheConflict {
                path,
                expected,
                winner,
            } => write!(
                f,
                "compiled-cache conflict at {}: expected output digest {expected}, but a divergent winner holds {winner} — nondeterministic compilation or tampering",
                path.display()
            ),
            Self::CacheCorrupt { path, detail } => write!(
                f,
                "compiled-cache entry failed self-validation at {}: {detail}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io {
            path: PathBuf::new(),
            msg: e.to_string(),
        }
    }
}

impl From<super::integrity::InvalidHashError> for StoreError {
    fn from(e: super::integrity::InvalidHashError) -> Self {
        Self::InvalidHash(e.input)
    }
}

/// Per-process memo of verified CAS blobs (P0-4 throughput fix, hardened
/// per adversarial audit vòng-3: (len, mtime) is NOT a security boundary —
/// an attacker with store write access can rewrite same-length content and
/// restore the mtime).
///
/// Hardened design:
/// - Key = the FULL CAS address (digest hex + executable bit) — the audit's
///   Bypass B (exec/non-exec collision) is structurally impossible.
/// - Fingerprint (Unix) = (device, inode, size, mtime, ctime). ctime is
///   updated by the kernel on EVERY metadata/content change and CANNOT be
///   restored by user code (utimensat only touches atime/mtime) — the
///   audit's Bypass A (same-length rewrite + mtime restore) is caught.
///   inode+device also catch file replacement by rename (new inode).
/// - Windows has no cheap stable file identity (NTFS file IDs can rotate
///   across reboot; BirthId APIs are niche) → the memo is DISABLED there:
///   every verify rehashes. Correctness before throughput, exactly as the
///   audit prescribes ("if there is no stable file identity: re-hash").
///
/// The FIRST touch of a blob still rehashes fully (fail-closed). The memo
/// only survives while the file identity stands — any mutation, replacement
/// or metadata-restore changes the fingerprint and forces a fresh rehash.
///
/// Memo theo-process của blob CAS đã verify (fix throughput P0-4, harden
/// theo adversarial audit vòng-3: (len, mtime) KHÔNG phải ranh giới bảo mật
/// — kẻ có quyền ghi store có thể ghi lại nội dung cùng độ dài rồi phục
/// hồi mtime).
///
/// Thiết kế đã harden:
/// - Key = địa chỉ CAS ĐẦY ĐỦ (hex digest + bit executable) — Bypass B của
///   audit (collision exec/non-exec) về cấu trúc là không thể.
/// - Fingerprint (Unix) = (device, inode, size, mtime, ctime). ctime do
///   kernel cập nhật MỌI thay đổi metadata/nội dung và KHÔNG thể phục hồi
///   bằng user code (utimensat chỉ chạm atime/mtime) — Bypass A của audit
///   (ghi đè cùng độ dài + phục hồi mtime) bị bắt. inode+device cũng bắt
///   file bị thay bằng rename (inode mới).
/// - Windows không có file identity ổn định rẻ (NTFS file ID có thể xoay
///   vòng sau reboot; API BirthId quá niche) → memo TẮT ở đó: mọi verify
///   rehash. Correctness trước throughput, đúng như audit chỉ dẫn ("không
///   có stable file identity: rehash lại").
///
/// Lần CHẠM ĐẦU blob vẫn rehash đầy đủ (fail-closed). Memo chỉ hiệu lực khi
/// file identity còn đứng — mọi sửa đổi, thay thế hay phục hồi metadata
/// đều đổi fingerprint và buộc rehash lại.
#[derive(Debug, Clone)]
pub struct ContentStore {
    root: PathBuf,
    /// Bounded verified-blob memo (P1-2 audit vòng-4): the old unbounded
    /// HashMap grew with every blob ever touched — a long-lived dev server
    /// or registry leaked memory per digest. Now an LRU with a fixed
    /// capacity: the LEAST recently used verification is evicted first,
    /// worst case a re-verify (fail-closed rehash), never a wrong serve.
    ///
    /// Memo blob-đã-verify CÓ GIỚI HẠN (P1-2 audit vòng-4): HashMap cũ
    /// không giới hạn phình theo từng blob từng chạm — dev server /
    /// registry chạy lâu rò memory theo số digest. Giờ là LRU với capacity
    /// cố định: verification ít dùng nhất bị đẩy ra trước, tệ nhất là
    /// re-verify (rehash fail-closed), không bao giờ serve sai.
    #[cfg(unix)]
    verified: std::sync::Arc<parking_lot::Mutex<lru::LruCache<VerifiedBlobKey, UnixFileIdentity>>>,
    /// Monotonic hit/miss/eviction counters for the memo (P1-2): a
    /// security-relevant cache must be measurable, not guessed.
    /// (Bộ đếm hit/miss/eviction monotonic của memo (P1-2): cache liên quan
    /// bảo mật phải đo được, không đoán mò.)
    #[cfg(unix)]
    memo_counters: std::sync::Arc<AtomicMemoCounters>,
    /// Non-Unix: no stable file identity → memo disabled structurally (the
    /// field exists to keep the struct shape stable but is never read; all
    /// verifies rehash — see `file_identity` for the rationale).
    /// (Non-Unix: không có stable identity → memo tắt về cấu trúc (field
    /// tồn tại để giữ shape struct ổn định nhưng không bao giờ đọc; mọi
    /// verify rehash — xem `file_identity` cho lý do).)
    #[cfg(not(unix))]
    verified: std::sync::Arc<parking_lot::Mutex<std::collections::HashMap<VerifiedBlobKey, ()>>>,
}

/// Default capacity of the bounded verified-blob memo (P1-2 audit vòng-4).
/// A compile-time constant, not a magic literal; large enough to keep hot
/// install/export flows memoized, small enough that a dev session cannot
/// leak unbounded memory.
///
/// Capacity mặc định của memo blob-đã-verify có giới hạn (P1-2 audit
/// vòng-4). Const tập trung, không phải literal rải rác; đủ lớn để giữ
/// memo cho luồng install/export nóng, đủ nhỏ để phiên dev không rò memory
/// không giới hạn.
pub const MEMO_CAPACITY: usize = 4096;

/// Atomic hit/miss/eviction counters (P1-2) — lock-free, monotonic.
/// (Bộ đếm hit/miss/eviction atomic (P1-2) — không khóa, monotonic.)
#[cfg(unix)]
#[derive(Debug, Default)]
struct AtomicMemoCounters {
    hits: std::sync::atomic::AtomicU64,
    misses: std::sync::atomic::AtomicU64,
    evictions: std::sync::atomic::AtomicU64,
}

/// Snapshot of memo counters (P1-2) — what tests and future metrics read.
/// (Ảnh chụp bộ đếm memo (P1-2) — thứ test và metrics tương lai đọc.)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

/// Full CAS address as the memo key — digest + executable (the CAS path is
/// derived from exactly these two, so two entries with the same key are the
/// same file; the audit's exec/non-exec collision cannot happen).
///
/// Địa chỉ CAS đầy đủ làm key memo — digest + executable (path CAS suy ra
/// từ đúng 2 giá trị này nên 2 entry cùng key là cùng file; collision
/// exec/non-exec của audit không thể xảy ra).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct VerifiedBlobKey {
    digest: String,
    executable: bool,
}

/// Platform file identity for the verified-blob memo.
/// Unix: (device, inode, size, mtime-nanos, ctime-nanos) — ctime cannot be
/// restored by user code, inode changes on replacement.
/// Windows: no identity — the memo is disabled (always rehash).
///
/// File identity theo platform cho memo blob-đã-verify.
/// Unix: (device, inode, size, mtime-nanos, ctime-nanos) — ctime không thể
/// phục hồi bằng user code, inode đổi khi file bị thay.
/// Windows: không có identity — memo tắt (luôn rehash).
/// Unix file identity: device + inode + size + mtime + ctime.
/// (Identity file Unix: device + inode + size + mtime + ctime.)
/// `None` from `file_identity()` = no stable identity (never memoize);
/// `Some(UnixFileIdentity)` is a complete kernel-maintained fingerprint:
/// ctime cannot be restored by user code (utimensat only touches
/// atime/mtime), inode changes on file replacement.
/// (`None` từ `file_identity()` = không có identity ổn định (không memo
/// hóa); `Some(UnixFileIdentity)` là fingerprint hoàn chỉnh do kernel giữ:
/// ctime không thể phục hồi bằng user code (utimensat chỉ chạm
/// atime/mtime), inode đổi khi file bị thay.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UnixFileIdentity {
    dev: u64,
    ino: u64,
    size: u64,
    mtime_nanos: u64,
    ctime_nanos: u64,
}

/// Fingerprint a file for memo purposes (Unix). Err on symlink/IO trouble.
/// (Lấy fingerprint file cho memo (Unix). Err khi symlink/lỗi IO.)
#[cfg(unix)]
fn file_identity(path: &Path) -> Result<Option<UnixFileIdentity>, StoreError> {
    // symlink_metadata: identity of the LINK TARGET must not be used.
    // (symlink_metadata: không được dùng identity của ĐÍCH link.)
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Err(StoreError::Io {
            path: path.to_path_buf(),
            msg: "path is a symlink, refusing to fingerprint".to_string(),
        });
    }
    use std::os::unix::fs::MetadataExt;
    let mtime_nanos = meta.mtime() as u64 * 1_000_000_000 + meta.mtime_nsec() as u64;
    // ctime (status-change time): kernel-maintained, user-restore impossible
    // — the anti-Bypass-A core of the fingerprint.
    // (ctime (status-change time): kernel giữ, không thể phục hồi — lõi
    // chống Bypass A của fingerprint.)
    let ctime_nanos = meta.ctime() as u64 * 1_000_000_000 + meta.ctime_nsec() as u64;
    Ok(Some(UnixFileIdentity {
        dev: meta.dev(),
        ino: meta.ino(),
        size: meta.size(),
        mtime_nanos,
        ctime_nanos,
    }))
}

/// Windows/other: no cheap stable file identity (NTFS file IDs rotate
/// across reboot; BirthId APIs are niche) — never memoize, always rehash
/// (exactly what the audit prescribes for missing stable identity).
/// (Windows/khác: không có file identity ổn định rẻ (NTFS file ID xoay
/// vòng sau reboot; API BirthId quá niche) — không memo hóa, luôn rehash
/// (đúng như audit chỉ dẫn khi thiếu stable identity).)
#[cfg(not(unix))]
fn file_identity(_path: &Path) -> Result<Option<std::convert::Infallible>, StoreError> {
    Ok(None)
}

impl ContentStore {
    pub fn new(root: PathBuf) -> Result<Self, StoreError> {
        validate_cas_root(&root)?;
        ensure_cas_dirs(&root)?;
        set_cas_root_permissions(&root)?;
        // Bounded LRU memo (P1-2 audit vòng-4) — capacity from the central
        // const, overridable per-store for tests via `with_memo_capacity`.
        // (Memo LRU có giới hạn (P1-2 audit vòng-4) — capacity từ const tập
        // trung, ghi đè được từng store cho test qua `with_memo_capacity`.)
        Ok(Self {
            root,
            #[cfg(unix)]
            verified: std::sync::Arc::new(parking_lot::Mutex::new(lru::LruCache::new(
                std::num::NonZeroUsize::new(MEMO_CAPACITY).expect("MEMO_CAPACITY is nonzero"),
            ))),
            #[cfg(unix)]
            memo_counters: std::sync::Arc::new(AtomicMemoCounters::default()),
            #[cfg(not(unix))]
            verified: std::sync::Arc::new(
                parking_lot::Mutex::new(std::collections::HashMap::new()),
            ),
        })
    }

    /// Test-observable memo stats (P1-2): hits/misses/evictions since store
    /// creation. Read by the LRU-bound test; not part of the serving API.
    ///
    /// (Chỉ số memo quan sát được cho test (P1-2): hit/miss/eviction từ lúc
    /// dựng store. Test LRU-bound đọc; không thuộc serving API.)
    #[cfg(unix)]
    #[doc(hidden)]
    pub fn memo_stats(&self) -> MemoStats {
        let c = &self.memo_counters;
        MemoStats {
            hits: c.hits.load(std::sync::atomic::Ordering::Relaxed),
            misses: c.misses.load(std::sync::atomic::Ordering::Relaxed),
            evictions: c.evictions.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// Test hook: build a store sharing this root with a TINY memo capacity
    /// to prove the LRU bound + eviction accounting (P1-2). Counters reset
    /// with the fresh handle. Not part of the serving API.
    ///
    /// (Hook test: dựng store cùng root này với capacity memo NHỎ để chứng
    /// minh giới hạn LRU + đếm eviction (P1-2). Bộ đếm reset cùng handle
    /// mới. Không thuộc serving API.)
    #[cfg(unix)]
    #[doc(hidden)]
    pub fn with_memo_capacity(&self, capacity: std::num::NonZeroUsize) -> Self {
        // NonZeroUsize (P1-3, Tech Lead vòng-5/6/7): the old `usize` +
        // `expect` panicked on 0 at runtime; the type now makes a
        // zero-capacity memo UNREPRESENTABLE — no panic path exists.
        // (NonZeroUsize (P1-3): kiểu cũ `usize` + `expect` panic khi 0
        // lúc runtime; giờ type khiến memo capacity-0 KHÔNG THỂ BIỂU DIỄN —
        // không còn đường panic.)
        Self {
            root: self.root.clone(),
            verified: std::sync::Arc::new(parking_lot::Mutex::new(lru::LruCache::new(capacity))),
            memo_counters: std::sync::Arc::new(AtomicMemoCounters::default()),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn compiled_cache(&self) -> crate::cas::CompiledCache {
        crate::cas::CompiledCache::new(self.root.clone())
    }

    pub fn import_file(&self, src: &Path) -> Result<IntegrityHash, StoreError> {
        if src.is_symlink() {
            return Err(StoreError::Io {
                path: src.to_path_buf(),
                msg: "source path is a symlink".to_string(),
            });
        }

        let meta = fs::metadata(src)?;
        let is_exec = is_executable(src);
        let use_streaming = meta.len() as usize >= STREAM_THRESHOLD;

        let hash = if use_streaming {
            let file = fs::File::open(src)?;
            let reader = BufReader::new(file);
            let tmp = self.tmp_path("import-file");
            fs::create_dir_all(tmp.parent().expect("tmp path has parent"))?;
            let writer = fs::File::create_new(&tmp)?;
            let hash = stream_write_verify_and_set_perms(writer, &tmp, reader, is_exec)?;
            let dest = hash.cas_path(&self.root);

            // Rehash path: streamed bytes were hashed on write; existing dest
            // is verified below via verify_stored_blob (mutation detection).
            if dest.exists() {
                fs::remove_file(&tmp)?;
                self.verify_stored_blob(&hash)?;
                return Ok(hash);
            }

            fs::create_dir_all(dest.parent().expect("dest path has parent"))?;
            match fs::rename(&tmp, &dest) {
                Ok(()) => {
                    // Crash-durable publish (audit vòng-3 P1-7).
                    // (Publish bền vững khi crash (P1-7 audit vòng-3).)
                    if let Some(parent) = dest.parent() {
                        sync_parent_dir(parent)?;
                    }
                }
                Err(e) if dest.exists() => {
                    // Lost the race: the winner is content-addressed and was
                    // fully written+fsynced before its rename — verify it and
                    // drop our temp (same semantics as write_bytes_with_hash).
                    // (Thua race: winner là content-addressed đã ghi xong +
                    // fsync trước khi rename — verify winner rồi bỏ temp
                    // (cùng semantics với write_bytes_with_hash).)
                    let _ = e;
                    fs::remove_file(&tmp)?;
                    self.verify_stored_blob(&hash)?;
                }
                Err(e) => {
                    fs::remove_file(&tmp).ok();
                    return Err(StoreError::Io {
                        path: dest.clone(),
                        msg: format!("move streamed file into CAS failed: {e}"),
                    });
                }
            }
            hash
        } else {
            let data = fs::read(src)?;
            let hash = IntegrityHash::from_bytes(&data, is_exec);
            // Same atomic primitive as every CAS write (unique temp →
            // fsync → verify → atomic publish → winner-verify on race).
            // (Cùng nguyên thủy nguyên tử như mọi ghi CAS (temp duy nhất →
            // fsync → verify → publish nguyên tử → verify-winner khi đua).)
            self.write_bytes_with_hash(&data, &hash, is_exec)?;
            hash
        };

        Ok(hash)
    }

    pub fn import_bytes(&self, data: &[u8]) -> Result<IntegrityHash, StoreError> {
        self.import_bytes_with_exec(data, false)
    }

    pub fn import_bytes_with_exec(
        &self,
        data: &[u8],
        executable: bool,
    ) -> Result<IntegrityHash, StoreError> {
        let hash = IntegrityHash::from_bytes(data, executable);
        self.write_bytes_with_hash(data, &hash, executable)
    }

    pub fn import_bytes_with_hash(
        &self,
        data: &[u8],
        hash_hex: &str,
        executable: bool,
    ) -> Result<IntegrityHash, StoreError> {
        let hash = IntegrityHash::from_hash_str(hash_hex, executable)?;
        self.write_bytes_with_hash(data, &hash, executable)
    }

    fn write_bytes_with_hash(
        &self,
        data: &[u8],
        hash: &IntegrityHash,
        executable: bool,
    ) -> Result<IntegrityHash, StoreError> {
        // ADVERSARIAL GATE (vòng-2 review finding R3): the declared hash must
        // match the actual bytes BEFORE any cache-hit shortcut. Without this
        // gate, importing content-A while declaring digest-B (whose blob
        // already exists in the CAS) would verify the OLD valid blob-B,
        // return Ok, and let the caller believe content-A was stored under
        // digest-B — a poisoning bypass.
        //
        // CỔNG ĐỐI KHÁNG (finding R3 vòng-2 review): digest khai phải khớp
        // bytes thực TRƯỚC mọi đường tắt cache-hit. Không có cổng này, import
        // content-A nhưng khai digest-B (mà blob-B đã có trong CAS) sẽ verify
        // blob-B cũ hợp lệ, trả Ok, và để caller tin content-A đã được lưu
        // dưới digest-B — bypass đầu độc.
        let actual = IntegrityHash::from_bytes(data, executable);
        if actual.as_hex() != hash.as_hex() {
            return Err(StoreError::HashMismatch {
                expected: hash.as_hex().to_string(),
                actual: actual.as_hex().to_string(),
            });
        }

        let dest = hash.cas_path(&self.root);
        if dest.exists() {
            // Existing content wins (dedup) but ONLY after re-hashing it.
            // Nội dung đã có (dedup) chỉ được dùng SAU khi rehash xong.
            self.verify_stored_blob(hash)?;
            return Ok(hash.clone());
        }

        fs::create_dir_all(dest.parent().expect("dest path has parent"))?;
        let tmp = self.tmp_path("write-bytes");
        fs::create_dir_all(tmp.parent().expect("tmp path has parent"))?;
        let writer = fs::File::create(&tmp)?;

        // The digest gate above already proved data==hash, and the write
        // primitives hash-while-writing for the torn-write check — the
        // returned hash must equal `actual` by construction.
        //
        // Cổng digest phía trên đã chứng minh data==hash, và nguyên thủy ghi
        // hash-while-writing để check torn-write — hash trả về buộc phải
        // bằng `actual` theo cấu trúc.
        let written = if data.len() >= STREAM_THRESHOLD {
            let cursor = std::io::Cursor::new(data);
            let reader = BufReader::new(cursor);
            stream_write_verify_and_set_perms(writer, &tmp, reader, executable)
        } else {
            write_all_verify_and_set_perms(writer, &tmp, data, executable)
        }?;
        if written.as_hex() != hash.as_hex() {
            let _ = fs::remove_file(&tmp);
            return Err(StoreError::HashMismatch {
                expected: hash.as_hex().to_string(),
                actual: written.as_hex().to_string(),
            });
        }

        // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
        if !dest.exists()
            && let Err(e) = fs::rename(&tmp, &dest)
        {
            // Lost the race: another writer moved it first — the winner is
            // content-addressed and was fully written+fsynced before its
            // rename, so verify the winner instead of failing blindly.
            // Thua race: writer khác move trước — winner là content-addressed
            // đã ghi xong + fsync trước khi rename, nên verify winner thay vì
            // fail mù.
            if !dest.exists() {
                let _ = fs::remove_file(&tmp);
                return Err(StoreError::Io {
                    path: dest.clone(),
                    msg: format!("move tmp file into CAS failed: {e}"),
                });
            }
            self.verify_stored_blob(hash)?;
        }
        let _ = fs::remove_file(&tmp);

        // Crash-durable publish: fsync the CAS parent dir after the rename
        // (audit vòng-3 P1-7 — without it the rename survives process crash
        // but not power loss).
        // (Publish bền vững khi crash: fsync thư mục cha CAS sau rename
        // (P1-7 audit vòng-3 — thiếu nó rename sống qua process crash
        // nhưng không qua mất điện).)
        if let Some(parent) = dest.parent() {
            sync_parent_dir(parent)?;
        }

        Ok(hash.clone())
    }

    /// Verify the stored blob for `hash` still hashes to `hash`.
    /// On mismatch the corrupt blob is moved to `quarantine/` (forensics) and
    /// the caller gets a hard error — a poisoned shared store must never be
    /// silently served to other projects.
    ///
    /// P0-4 memo (hardened vòng-3): the first verify rehashes fully
    /// (fail-closed). A later verify skips the rehash ONLY while the memo
    /// entry matches on the full CAS address (digest + executable) AND the
    /// file identity (device, inode, size, mtime, ctime on Unix) stands.
    /// ctime cannot be restored by user code, and inode changes on file
    /// replacement — the same-length-rewrite + mtime-restore attack from
    /// the audit changes ctime and is caught. Platforms without a stable
    /// identity (Windows) never memoize: every verify rehashes.
    ///
    /// Rehash blob đã lưu: khớp → OK; lệch → chuyển blob sang `quarantine/`
    /// (giữ bằng chứng) rồi báo lỗi cứng — store chung bị đầu độc không bao
    /// giờ được phục vụ im lặng cho project khác. Memo P0-4 (harden vòng-3):
    /// verify đầu rehash đầy đủ (fail-closed). Verify sau bỏ rehash CHỈ KHI
    /// memo khớp địa chỉ CAS đầy đủ (digest + executable) VÀ file identity
    /// (device, inode, size, mtime, ctime trên Unix) còn đúng. ctime không
    /// thể phục hồi bằng user code, inode đổi khi file bị thay — tấn công
    /// ghi-cùng-độ-dài + phục hồi mtime của audit đổi ctime và bị bắt.
    /// Platform không có identity ổn định (Windows) không memo hóa: verify
    /// nào cũng rehash.
    fn verify_stored_blob(&self, hash: &IntegrityHash) -> Result<(), StoreError> {
        let path = hash.cas_path(&self.root);

        // Memo key = FULL CAS address (digest + executable): the exec and
        // non-exec variants of one digest are distinct files and must never
        // share a verification (audit Bypass B).
        // (Key memo = địa chỉ CAS ĐẦY ĐỦ (digest + executable): 2 biến thể
        // exec/non-exec của cùng digest là 2 file khác nhau, không được chia
        // verification (Bypass B của audit).)
        let memo_key = VerifiedBlobKey {
            digest: hash.as_hex().to_string(),
            executable: hash.is_executable(),
        };

        // Identity gate: platform file identity of the blob right now.
        // (Cổng identity: file identity theo platform của blob lúc này.)
        let identity = file_identity(&path)?;

        // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
        // Memo hit requires: identity is memoizable AND matches the LRU
        // entry (P1-2: bounded cache, LRU-refresh on hit).
        // (Memo hit đòi hỏi: identity memo được VÀ khớp entry trong LRU
        // (P1-2: cache có giới hạn, refresh LRU khi hit).)
        #[cfg(unix)]
        if let Some(id) = identity
            && let Some(cached) = self.verified.lock().get(&memo_key)
            && *cached == id
        {
            self.memo_counters
                .hits
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Ok(());
        }
        #[cfg(not(unix))]
        {
            let _ = memo_key;
        }
        self.memo_miss();

        match self.verify(&path) {
            Ok(actual) if actual.as_hex() == hash.as_hex() => {
                // Memoize only when the platform provides a stable identity
                // (P1-2: LRU put — the least-recently-used entry is evicted
                // at capacity; worst case is a re-verify, never a wrong
                // serve).
                // (Chỉ memo khi platform có identity ổn định (P1-2: put LRU
                // — entry ít dùng nhất bị đẩy ra khi đầy; tệ nhất là
                // re-verify, không bao giờ serve sai).)
                #[cfg(unix)]
                if let Some(id) = identity {
                    let mut cache = self.verified.lock();
                    // Eviction accounting (P1-1 fix, adversarial review
                    // vòng-9 2026-09-14): the old `len_before == cap &&
                    // len == cap` check counted a REPLACEMENT of an
                    // existing key on a FULL cache as an eviction. The
                    // truthful rule: an eviction happened only when the
                    // cache was full AND the key was NOT already present
                    // (a fresh insert that must displace an entry). A
                    // re-verify of a known key is a replacement — the
                    // eviction counter must not move.
                    // (Kế toán eviction (sửa P1-1): check cũ
                    // `len_before == cap && len == cap` đếm THAY THẾ key
                    // đã có trên cache ĐẦY thành eviction. Luật đúng:
                    // eviction chỉ xảy ra khi cache đầy VÀ key CHƯA có
                    // sẵn (insert mới phải đẩy bớt entry). Verify lại key
                    // đã biết là replacement — bộ đếm eviction không được
                    // nhích.)
                    let len_before = cache.len();
                    let capacity = cache.cap().get();
                    let key_existed = cache.contains(&memo_key);
                    cache.push(memo_key, id);
                    if len_before == capacity && !key_existed {
                        self.memo_counters
                            .evictions
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
                #[cfg(not(unix))]
                {
                    let _ = identity;
                }
                Ok(())
            }
            Ok(actual) => {
                // Invalidation on mismatch (P1-2): the memo entry for this
                // address must die with the blob it vouched for.
                // (Vô hiệu hóa khi lệch (P1-2): memo entry cho địa chỉ này
                // phải chết cùng blob mà nó bảo chứng.)
                #[cfg(unix)]
                self.verified.lock().pop(&memo_key);
                self.quarantine_blob(&path, hash.as_hex(), actual.as_hex())?;
                Err(StoreError::HashMismatch {
                    expected: hash.as_hex().to_string(),
                    actual: actual.as_hex().to_string(),
                })
            }
            Err(StoreError::NotFound(_)) => Err(StoreError::NotFound(hash.as_hex().to_string())),
            Err(e) => Err(e),
        }
    }

    /// Count a memo miss (P1-2). Split out so the cfg(not(unix)) path (no
    /// memo at all) does not count phantom misses.
    /// (Đếm memo miss (P1-2). Tách riêng để đường cfg(not(unix)) (không có
    /// memo) không đếm miss giả.)
    fn memo_miss(&self) {
        #[cfg(unix)]
        self.memo_counters
            .misses
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Move a corrupt blob into `quarantine/` for forensics + later repair.
    /// Chuyển blob hỏng vào `quarantine/` để điều tra và sửa sau.
    fn quarantine_blob(&self, path: &Path, expected: &str, actual: &str) -> Result<(), StoreError> {
        let qdir = self.root.join("quarantine");
        let _ = fs::create_dir_all(&qdir);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let qfile = qdir.join(format!(
            "corrupt-{}-{}-{}.blob",
            &expected[..12.min(expected.len())],
            &actual[..12.min(actual.len())],
            stamp
        ));
        // Best-effort: quarantine failure must not mask the mismatch error.
        // Best-effort: lỗi quarantine không được che lỗi mismatch gốc.
        let _ = fs::rename(path, &qfile);
        let _ = fs::write(
            qdir.join(format!(
                "corrupt-{}-{}.report",
                &expected[..12.min(expected.len())],
                stamp
            )),
            format!(
                "expected blake3: {expected}\nactual blake3:   {actual}\nsource path: {}\nquarantined at: {stamp}\n",
                path.display()
            ),
        );
        Ok(())
    }

    /// Export a CAS blob into a project directory as an INDEPENDENT copy.
    /// Hardlinks were removed: a project-side mutation must never write
    /// through into the shared store (P0 shared-CAS poisoning).
    ///
    /// P0-2 (anti-EXDEV): the staging temp is created INSIDE THE DESTINATION
    /// directory (`.mgc-export-<unique>` next to the final path), so the
    /// final rename is always same-filesystem and never fails with EXDEV
    /// when the store and the project live on different volumes (Docker
    /// mounts, external drives, CI mounts, network shares).
    ///
    /// COW strategy (P0-4): clonefile (macOS/APFS) / FICLONE (Linux) /
    /// ReFS block cloning (Windows, best-effort) are attempted first —
    /// copy-on-write is fast AND keeps the export an independent file
    /// (mutating the export never touches the CAS blob). Plain copy is the
    /// fallback. The result is hashed after staging — a torn copy is
    /// deleted and reported.
    ///
    /// Xuất blob CAS ra project bằng BẢN SAO/CLONE ĐỘC LẬP. Bỏ hardlink:
    /// project sửa file không thể ghi ngược vào shared store (chống đầu
    /// độc P0). P0-2: temp dựng NGAY TRONG thư mục đích (`.mgc-export-*`)
    /// nên rename cuối luôn cùng filesystem — không EXDEV khi store và
    /// project khác volume. P0-4: thử clonefile/FICLONE/ReFS COW trước
    /// (nhanh mà vẫn độc lập), fallback copy thường; kết quả được rehash
    /// sau khi staging — copy hỏng bị xóa và báo lỗi.
    pub fn export_to(&self, hash: &IntegrityHash, dest: &Path) -> Result<(), StoreError> {
        let parent = dest.parent().ok_or_else(|| StoreError::Io {
            path: dest.to_path_buf(),
            msg: "export destination has no parent directory".to_string(),
        })?;
        fs::create_dir_all(parent)?;
        // Check the destination parent for symlinks (dest doesn't exist yet).
        // Kiểm tra thư mục cha của đích có symlink không (dest chưa tồn tại).
        check_symlink_ancestors(parent)?;

        let src = hash.cas_path(&self.root);
        if !src.exists() {
            return Err(StoreError::NotFound(hash.as_hex().to_string()));
        }

        if dest.exists() {
            let dest_hash = self.verify(dest)?;
            if dest_hash.as_hex() == hash.as_hex() {
                // Vòng-3 audit P1-3: identical bytes are not enough — the
                // executable SEMANTIC must also match, otherwise an export of
                // an executable blob onto a stale 0644 file reports success
                // while producing a wrong-mode artifact. Repair the mode on
                // the existing destination (idempotent permission fix).
                // (P1-3 audit vòng-3: bytes giống nhau chưa đủ — semantic
                // executable cũng phải khớp, nếu không export blob exec
                // lên file 0644 cũ sẽ báo thành công nhưng ra artifact sai
                // mode. Sửa mode trên đích đã tồn tại (fix permission
                // idempotent).)
                set_exported_permissions(dest, hash.is_executable())?;
                return Ok(());
            }
            return Err(StoreError::Io {
                path: dest.to_path_buf(),
                msg: "destination already exists".to_string(),
            });
        }

        // Fail-closed gate BEFORE staging: the source blob must verify
        // (first touch rehashes; memo holds while the file identity stands).
        // This is the mutation check that protects every downstream reader.
        // Cổng fail-closed TRƯỚC staging: blob nguồn phải verify (lần chạm
        // đầu rehash; memo hiệu lực khi file identity còn đúng). Đây là
        // check sửa đổi bảo vệ mọi reader phía sau.
        self.verify_stored_blob(hash)?;

        // P0-2: stage INSIDE the destination filesystem — never the CAS tmp
        // dir — so the final rename cannot hit EXDEV on cross-volume setups.
        // P0-2: staging TRONG filesystem đích — không dùng tmp của CAS —
        // rename cuối không thể gặp EXDEV khi khác volume.
        let tmp = unique_tmp_path(parent, "mgc-export");
        let export_guard = ExportTempGuard { path: &tmp };

        // COW first, plain copy fallback — both produce an independent file.
        // COW trước, copy thường dự phòng — cả hai đều ra file độc lập.
        stage_export_bytes(&src, &tmp)?;

        // Vòng-3 audit P0-3: FULL HASH of the staged bytes before publish.
        // A length check proves nothing about content — two different files
        // can share a length, and fs::copy/clonefile are not content-
        // integrity transactions (the source could be swapped mid-copy).
        // The staged artifact is hashed end-to-end and must equal the
        // declared digest, or the export fails closed and nothing is
        // published. Correctness outranks the saved re-read.
        //
        // (P0-3 audit vòng-3: HASH ĐẦY ĐỦ bytes staged trước khi publish. Check
        // độ dài không chứng minh gì về nội dung — 2 file khác nhau có thể
        // cùng độ dài, và fs::copy/clonefile không phải transaction
        // content-integrity (nguồn có thể bị đổi giữa lúc copy). Artifact
        // staged được hash end-to-end và phải bằng digest khai, không thì
        // export fail cứng và không publish gì cả. Correctness đứng trên
        // phần đọc lặp tiết kiệm được.)
        let staged_hash = hash_file(&tmp)?;
        if staged_hash != hash.as_hex() {
            drop(export_guard);
            let _ = fs::remove_file(&tmp);
            return Err(StoreError::HashMismatch {
                expected: hash.as_hex().to_string(),
                actual: staged_hash,
            });
        }

        // Same-filesystem atomic publish (P0-2) + crash-durable parent
        // fsync (audit vòng-3 P1-7).
        // Publish nguyên tử cùng filesystem (P0-2) + fsync cha bền vững
        // khi crash (P1-7 audit vòng-3).
        if let Err(e) = fs::rename(&tmp, dest) {
            return Err(StoreError::Io {
                path: dest.to_path_buf(),
                msg: format!("move exported copy into place failed: {e}"),
            });
        }
        drop(export_guard);
        set_exported_permissions(dest, hash.is_executable())?;
        sync_parent_dir(parent)?;

        Ok(())
    }

    pub fn verify(&self, path: &Path) -> Result<IntegrityHash, StoreError> {
        // Vòng-2 review R7: use symlink_metadata FIRST — fs::metadata FOLLOWS
        // the link, so a symlink-to-a-regular-file would slip past the old
        // is_symlink check (metadata of the TARGET, not the link).
        // (R7 vòng-2 review: dùng symlink_metadata TRƯỚC — fs::metadata
        // FOLLOW link nên symlink-trỏ-file-thường lọt qua check is_symlink
        // cũ (metadata của ĐÍCH, không phải của link).)
        let lmeta = fs::symlink_metadata(path)?;
        if lmeta.file_type().is_symlink() {
            return Err(StoreError::Io {
                path: path.to_path_buf(),
                msg: "path is a symlink, refusing to verify".to_string(),
            });
        }
        if !lmeta.is_file() {
            return Err(StoreError::Io {
                path: path.to_path_buf(),
                msg: "path is not a regular file".to_string(),
            });
        }
        // Streaming hash (P1-A, Tech Lead vòng-7): `fs::read` loaded the
        // WHOLE file into RAM — memory grew linearly with blob size, which
        // rules out AI-model/game-asset/monorepo-scale blobs. The fixed
        // 64-KiB buffer keeps RSS flat regardless of file size; the digest
        // is byte-identical to the old in-memory form (BLAKE3 over the
        // same bytes).
        // (Hash streaming (P1-A): `fs::read` load TOÀN BỘ file vào RAM —
        // memory tăng tuyến tính theo kích thước blob, vô hiệu hóa blob
        // cỡ AI-model/game-asset/monorepo. Buffer cố định 64-KiB giữ RSS
        // phẳng bất kể kích thước file; digest giống hệt byte với dạng
        // in-memory cũ (BLAKE3 trên cùng bytes).)
        let file = fs::File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut hasher = blake3::Hasher::new();
        let mut buf = [0u8; 65536];
        loop {
            use std::io::Read;
            let n = reader.read(&mut buf).map_err(|e| StoreError::Io {
                path: path.to_path_buf(),
                msg: format!("read for verify failed: {e}"),
            })?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let hex = hasher.finalize().to_hex().to_string();
        // Behavior-preserving: the old form always hashed with
        // `executable: false` — keep that exact contract (callers compare
        // `as_hex()` which carries no exec bit, and changing it here would
        // silently alter IntegrityHash equality semantics).
        // (Giữ nguyên hành vi: dạng cũ luôn hash với `executable: false` —
        // giữ đúng hợp đồng đó (caller so `as_hex()` không chứa exec bit,
        // và đổi ở đây sẽ âm thầm thay đổi semantics bằng-equal của
        // IntegrityHash).)
        IntegrityHash::from_hash_trusted(&hex, false).map_err(|e| StoreError::InvalidHash(e.input))
    }

    pub fn contains(&self, hash: &IntegrityHash) -> bool {
        hash.cas_path(&self.root).exists()
    }

    pub fn remove(&self, hash: &IntegrityHash) -> Result<(), StoreError> {
        let path = hash.cas_path(&self.root);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        // Memo invalidation on delete (P1-2 audit vòng-4): the entry vouched
        // for a file that no longer exists — it must not survive the blob.
        // (Vô hiệu hóa memo khi xóa (P1-2 audit vòng-4): entry bảo chứng cho
        // file không còn tồn tại — nó không được sống sót qua blob.)
        #[cfg(unix)]
        self.verified.lock().pop(&VerifiedBlobKey {
            digest: hash.as_hex().to_string(),
            executable: hash.is_executable(),
        });
        Ok(())
    }

    /// Import tarball entries into the CAS atomically (P0-3).
    /// Every entry goes through the SAME primitive as `write_bytes_with_hash`:
    /// unique temp (same CAS filesystem) → write + fsync → verify → atomic
    /// publish → if the rename race was lost, the winner (fully written,
    /// content-addressed) is re-verified instead of failing with
    /// AlreadyExists. Readers can never observe a half-written blob at the
    /// final CAS path.
    ///
    /// Import entry tarball vào CAS nguyên tử (P0-3): mọi entry đi qua CÙNG
    /// nguyên thủy với `write_bytes_with_hash` — temp duy nhất (cùng
    /// filesystem CAS) → ghi + fsync → verify → publish nguyên tử → thua
    /// race thì re-verify winner (đã ghi xong, content-addressed) thay vì
    /// fail AlreadyExists. Reader không bao giờ thấy blob nửa vời ở path
    /// CAS cuối.
    pub fn import_tarball_entries(
        &self,
        entries: Vec<TarballEntry>,
    ) -> Result<Vec<IntegrityHash>, StoreError> {
        let mut imported = Vec::with_capacity(entries.len());
        for entry in entries {
            let hash = integrity::IntegrityHash::from_bytes(&entry.data, entry.executable);
            self.write_bytes_with_hash(&entry.data, &hash, entry.executable)?;
            imported.push(hash);
        }
        Ok(imported)
    }

    fn tmp_path(&self, prefix: &str) -> PathBuf {
        let path = unique_tmp_path(&self.root.join("tmp"), prefix);
        let _ = fs::create_dir_all(path.parent().expect("tmp path has parent"));
        path
    }

    /// Test-only probe: which staging strategy `export_to` would use on
    /// this machine/filesystem pair. Lets tests assert the CowClone branch
    /// is LIVE on COW-capable filesystems (a fallback that silently never
    /// runs is a false-green — audit vòng-3 P0-2). Not part of the serving
    /// API.
    ///
    /// Probe chỉ dành cho test: chiến lược staging `export_to` sẽ dùng cho
    /// cặp máy/filesystem này. Để test assert nhánh CowClone còn SỐNG trên
    /// filesystem hỗ trợ COW (fallback im lặng không chạy là false-green —
    /// P0-2 audit vòng-3). Không thuộc serving API.
    #[doc(hidden)]
    pub fn staging_kind_probe(&self, hash: &IntegrityHash, dest_dir: &Path) -> StagingKind {
        // Stage into a scratch file next to a probe destination, mirror of
        // the real export staging path.
        // (Staging vào file scratch cạnh đích probe, phản chiếu đúng đường
        // staging export thật.)
        let src = hash.cas_path(&self.root);
        let tmp = unique_tmp_path(dest_dir, "mgc-export-probe");
        match try_cow_clone(&src, &tmp) {
            Some(()) => {
                let _ = fs::remove_file(&tmp);
                StagingKind::CowClone
            }
            None => {
                let _ = fs::remove_file(&tmp);
                StagingKind::PlainCopy
            }
        }
    }
}

/// RAII cleanup for the export staging temp — removes the temp file on every
/// early-return path (P0-2/P0-3 discipline: never leak staging files).
/// Dọn staging temp bằng RAII — xóa file temp ở mọi đường return sớm
/// (kỷ luật P0-2/P0-3: không rò rỉ file staging).
struct ExportTempGuard<'a> {
    path: &'a Path,
}

impl Drop for ExportTempGuard<'_> {
    fn drop(&mut self) {
        // Best-effort only: after a successful rename the temp no longer
        // exists and removal is a harmless no-op.
        // Chỉ best-effort: sau rename thành công temp không còn tồn tại,
        // remove là no-op vô hại.
        let _ = fs::remove_file(self.path);
    }
}

/// Stage export bytes: try platform COW clone first, plain copy as fallback.
/// COW clones are INDEPENDENT files (copy-on-write) — a later mutation of
/// the export rehydrates a private block, it never writes through into the
/// CAS blob (the P0-1 hardlink-poisoning property is preserved).
/// NOTE: clonefile(2) requires the destination to NOT exist, so no file is
/// pre-created before the clone attempt — the fallback copy creates its
/// own destination.
///
/// Staging bytes export: thử clone COW theo platform trước, copy thường dự
/// phòng. Clone COW là file ĐỘC LẬP (copy-on-write) — sau này sửa file
/// export sẽ rehydrate block riêng, không bao giờ ghi ngược vào blob CAS
/// (giữ nguyên tính chất chống đầu độc hardlink của P0-1). LƯU Ý:
/// clonefile(2) yêu cầu đích CHƯA tồn tại nên không tạo file trước attempt
/// clone — nhánh copy dự phòng tự tạo đích của nó.
/// How export staging produced the temp file — tests assert the ACTUAL
/// branch used (a fast path that silently never runs is a false-green
/// waiting to happen, per adversarial audit vòng-3).
///
/// Cách staging export đã tạo temp — test assert branch THẬT được dùng
/// (fast path im lặng không chạy là false-green chờ xảy ra, theo audit
/// vòng-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StagingKind {
    /// Kernel copy-on-write clone (macOS clonefile / Linux FICLONE).
    /// (Clone copy-on-write cấp kernel: clonefile macOS / FICLONE Linux.)
    CowClone,
    /// Plain byte-copy fallback.
    /// (Fallback copy bytes thường.)
    PlainCopy,
}

impl StagingKind {
    /// Human-readable name (benchmark labels, tests).
    /// (Tên đọc được (nhãn benchmark, test).)
    pub fn label(self) -> &'static str {
        match self {
            Self::CowClone => "clonefile/FICLONE (COW)",
            Self::PlainCopy => "plain copy",
        }
    }
}

/// Stage export bytes: platform COW clone first, plain copy fallback.
/// Returns WHICH strategy ran so tests can prove the fast path is live
/// (and callers/tests can detect unsupported filesystems).
///
/// COW clones are INDEPENDENT files (copy-on-write) — a later mutation of
/// the export rehydrates a private block, it never writes through into the
/// CAS blob (the P0-1 hardlink-poisoning property is preserved).
/// NOTE (audit vòng-3 P0-2): clonefile(2) requires the destination to NOT
/// exist; Linux FICLONE requires the destination to BE a freshly-created
/// EMPTY file (open O_CREAT|O_EXCL). Both create-or-fail semantics are
/// handled per platform, and a failed clone attempt cleans up its temp so
/// the fs::copy fallback starts from a clean slate.
///
/// Staging bytes export: clone COW theo platform trước, copy thường dự
/// phòng. Trả về chiến lược ĐÃ CHẠY để test chứng minh fast path còn sống
/// (và caller/test phát hiện filesystem không hỗ trợ).
/// Clone COW là file ĐỘC LẬP (copy-on-write) — sau này sửa file export sẽ
/// rehydrate block riêng, không bao giờ ghi ngược vào blob CAS (giữ tính
/// chống đầu độc hardlink của P0-1). LƯU Ý (P0-2 audit vòng-3):
/// clonefile(2) yêu cầu đích CHƯA tồn tại; FICLONE Linux yêu cầu đích là
/// file rỗng MỚI TẠO (open O_CREAT|O_EXCL). Mỗi platform tự xử lý semantics
/// create-hoặc-lỗi của nó, và clone fail thì dọn temp để fs::copy fallback
/// bắt đầu từ trạng thái sạch.
fn stage_export_bytes(src: &Path, tmp: &Path) -> Result<StagingKind, StoreError> {
    match try_cow_clone(src, tmp) {
        Some(()) => Ok(StagingKind::CowClone),
        None => {
            // Fallback: plain byte copy (creates/truncates the destination
            // itself; any half-created clone temp was removed above).
            // Dự phòng: copy bytes thường (tự tạo/ghi đè đích; temp clone
            // nửa vời phía trên đã bị dọn).
            fs::copy(src, tmp).map_err(|e| StoreError::Io {
                path: tmp.to_path_buf(),
                msg: format!("copy blob for export failed: {e}"),
            })?;
            Ok(StagingKind::PlainCopy)
        }
    }
}

/// Platform copy-on-write clone attempts (P0-4).
/// - macOS: `clonefile(2)` on APFS (destination must NOT exist)
/// - Linux: `FICLONE` ioctl on reflink-capable filesystems (destination must
///   be a freshly created EMPTY file: O_CREAT|O_EXCL — audit vòng-3 P0-2
///   found the old code opening a non-existent path and ALWAYS falling back
///   to plain copy; the clone never ran)
/// - Windows: ReFS block-cloning via `DuplicateClusters` is not exposed as
///   a stable syscall API — DeviceIoControl paths are undocumented and
///   fragile; plain copy is used instead (documented limitation, not a
///   silent skip).
///
/// Returns Some(()) on a successful clone, None when cloning is
/// unavailable/failed (the caller falls back to plain copy). On a failed
/// clone attempt the temp destination is removed so the fallback copy
/// starts clean.
///
/// Thử clone copy-on-write theo platform (P0-4). macOS: clonefile(2) trên
/// APFS (đích KHÔNG được tồn tại). Linux: ioctl FICLONE trên filesystem hỗ
/// trợ reflink (đích phải là file rỗng MỚI TẠO: O_CREAT|O_EXCL — P0-2 audit
/// vòng-3 phát hiện code cũ mở path không tồn tại và LUÔN rơi fallback
/// copy; clone chưa bao giờ chạy). Windows: ReFS block-clone không có API
/// syscall ổn định — copy thường (giới hạn ghi rõ, không bỏ qua im lặng).
///
/// Trả Some(()) khi clone thành công, None khi clone không khả dụng/lỗi
/// (caller fallback copy thường). Clone lỗi thì temp bị xóa để fallback
/// copy bắt đầu sạch.
///
/// SAFETY: the two libc calls below are thin wrappers over documented
/// syscalls with plain C string/fd arguments and no memory reinterpretation:
/// - clonefile(src, dst, 0): reads only the two NUL-terminated CStrings
///   built above from validated Path values; the flags argument 0 is the
///   only documented value; the kernel performs its own path validation.
/// - ioctl(fd, FICLONE, src_fd): passes raw fds of two open File handles
///   owned by this function; FICLONE takes an int source-fd argument by
///   value (no pointer dereference); failure is returned as -1 and handled.
///
/// Neither call touches memory we must uphold invariants for.
/// (An toàn: 2 lệnh libc là wrapper mỏng của syscall có tài liệu, đối số
/// CString/fd thuần, không diễn giải lại bộ nhớ — kernel tự validate path;
/// FICLONE nhận source-fd theo giá trị; lỗi -1 được xử lý fallback copy.)
#[allow(unsafe_code)]
#[allow(unused_variables)]
fn try_cow_clone(src: &Path, tmp: &Path) -> Option<()> {
    #[cfg(target_os = "macos")]
    {
        // clonefile(2): atomic COW clone on APFS — independent file, zero
        // data copy until first mutation. Destination must not exist.
        // clonefile(2): clone COW nguyên tử trên APFS — file độc lập, không
        // copy data tới lần sửa đầu. Đích không được tồn tại.
        use std::ffi::CString;
        let Ok(src_c) = CString::new(src.as_os_str().to_string_lossy().as_bytes()) else {
            return None;
        };
        let Ok(tmp_c) = CString::new(tmp.as_os_str().to_string_lossy().as_bytes()) else {
            return None;
        };
        let ret = unsafe {
            // SAFETY: see the function-level safety proof above.
            // (An toàn: xem giải trình SAFETY ở cấp hàm phía trên.)
            libc::clonefile(src_c.as_ptr(), tmp_c.as_ptr(), 0)
        };
        if ret == 0 {
            Some(())
        } else {
            // clonefile failed (non-APFS, EXDEV, unsupported) — the
            // destination was never created by us; nothing to clean.
            // clonefile lỗi (non-APFS, EXDEV, không hỗ trợ) — đích chưa
            // được tạo bởi ta; không có gì phải dọn.
            None
        }
    }

    #[cfg(target_os = "linux")]
    {
        // FICLONE ioctl: reflink the src file into a FRESHLY-CREATED empty
        // temp file (O_CREAT|O_EXCL). The old code opened the non-existent
        // temp with plain .write(true).open() → NotFound → permanent
        // fallback (audit vòng-3 P0-2).
        // FICLONE ioctl: reflink file src vào temp rỗng MỚI TẠO
        // (O_CREAT|O_EXCL). Code cũ mở temp chưa tồn tại bằng .write(true)
        // .open() → NotFound → fallback vĩnh viễn (P0-2 audit vòng-3).
        use std::os::unix::io::AsRawFd;
        const FICLONE: libc::c_ulong = 0x40049409;
        let Ok(src_file) = fs::File::open(src) else {
            return None;
        };
        let tmp_file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(tmp);
        let Ok(tmp_file) = tmp_file else {
            return None;
        };
        let ret = unsafe {
            // SAFETY: see the function-level safety proof above.
            // (An toàn: xem giải trình SAFETY ở cấp hàm phía trên.)
            libc::ioctl(tmp_file.as_raw_fd(), FICLONE, src_file.as_raw_fd())
        };
        if ret == 0 {
            Some(())
        } else {
            // Clone failed (non-reflink FS, EXDEV, ...) — remove the empty
            // temp we created so the fs::copy fallback starts clean.
            // Clone lỗi (FS không reflink, EXDEV...) — xóa temp rỗng ta đã
            // tạo để fs::copy fallback bắt đầu sạch.
            drop(tmp_file);
            let _ = fs::remove_file(tmp);
            None
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        // Windows/other: no stable COW clone syscall API — plain copy.
        // Windows/khác: không có API syscall COW ổn định — copy thường.
        None
    }
}

/// Hash a file's bytes (BLAKE3, streaming — no full in-memory load).
/// Used by export to verify the STAGED artifact end-to-end before publish.
///
/// Hash bytes của file (BLAKE3, streaming — không load toàn bộ vào memory).
/// Dùng bởi export để verify artifact STAGED end-to-end trước khi publish.
fn hash_file(path: &Path) -> Result<String, StoreError> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = reader.read(&mut buf).map_err(|e| StoreError::Io {
            path: path.to_path_buf(),
            msg: format!("read for hash failed: {e}"),
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// Set permissions on an exported (project-side) file. Exported files are
/// ordinary project files — never a hardlink into the CAS.
/// Đặt permission cho file đã xuất ra project. File export là file project
/// bình thường — không bao giờ là hardlink vào CAS.
fn set_exported_permissions(path: &Path, executable: bool) -> Result<(), StoreError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if executable { 0o755 } else { 0o644 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|e| {
            StoreError::Io {
                path: path.to_path_buf(),
                msg: format!("set permissions failed: {e}"),
            }
        })?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, executable);
    }
    Ok(())
}

/// Validate an external hash string before it reaches a CAS API.
/// Helper cho caller ngoài crate: validate hash trước khi chạm CAS API.
pub fn validate_external_hash(hash_hex: &str) -> Result<(), StoreError> {
    validate_blake3_hex(hash_hex)
        .map(|_| ())
        .map_err(|e| StoreError::InvalidHash(e.input))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(path) {
            return (meta.permissions().mode() & 0o111) != 0;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    false
}
