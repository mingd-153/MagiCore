//! `install/extract.rs` — CAS extraction, marker signature generation and validation.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use mgc_fetcher::extract::extract_tarball_to_cas_and_link;
use mgc_store::{ContentStore, Database, Layout};
use mgc_types::adapter::ResolvedPackage;
use mgc_types::{MgError, MgResult, PackageId};

use crate::cache::{ExtractedPackageMarker, SharedWebCache};
pub use crate::install::package_marker::*;

pub fn extracted_package_root_lock(root: &Path) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<std::collections::HashMap<PathBuf, Arc<Mutex<()>>>>> =
        OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut guard = match locks.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard
        .entry(root.to_path_buf())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

pub fn tarball_prefetch_lock(id: &PackageId) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: OnceLock<Mutex<std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
        OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let key = format!("{}@{}", id.name_str(), id.version());
    let mut guard = match locks.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard
        .entry(key)
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

/// A fresh, unique temp path next to `canonical_root` for extraction. Used
/// both for the real extract+rename and for the warm-cache "import + claim
/// only" throwaway (the temp tree is discarded after import).
/// (Đường dẫn temp mới, duy nhất cạnh `canonical_root` cho extraction. Dùng
/// cho cả extract+rename thật lẫn throwaway "chỉ import + claim" của warm
/// cache — cây temp bị hủy sau khi import.)
fn fresh_extract_temp_root(pkg: &ResolvedPackage, canonical_root: &Path) -> PathBuf {
    let parent = canonical_root.parent().unwrap_or(Path::new("."));
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    // PID + nanos: the staging name must be unique ACROSS processes, not
    // just within one — two processes racing the same digest share the same
    // parent dir, and a same-nanosecond collision made one process
    // remove_dir_all the other's LIVE staging tree.
    // (PID + nano: tên staging phải duy nhất GIỮA CÁC process, không chỉ
    // trong một process — hai process đua cùng digest dùng chung thư mục cha,
    // và trùng nano-giây khiến một process remove_dir_all cây staging ĐANG
    // SỐNG của process kia.)
    parent.join(format!(
        ".mgc-extract-{}-{}-{}",
        pkg.id.name_str(),
        std::process::id(),
        ts
    ))
}

pub struct CasClaimContext {
    /// Fail-closed DB handle (Gate 11-A, P0-5 of vòng-11 audit): the OLD
    /// field was `Result<Database, String>` and `as_ref()` folded a DB
    /// open failure into `None` — extraction continued WITHOUT claims,
    /// then the end-of-install promote retired previous claims: warm CAS
    /// silently became unreferenced. Now the constructor RETURNS
    /// `Result` and every caller propagates the error — an install that
    /// cannot open the claim DB FAILS, it never claims nothing.
    /// (Handle DB fail-closed (P0-5): field cũ là
    /// `Result<Database, String>` và `as_ref()` gộp lỗi mở DB thành
    /// `None` — extraction tiếp tục KHÔNG claim, rồi promote cuối install
    /// nghỉ hưu claim cũ: warm CAS âm thầm mất tham chiếu. Giờ constructor
    /// TRẢ `Result` và mọi caller propagate lỗi — install không mở được DB
    /// claim thì FAIL, không bao giờ "claim không gì".)
    pub db: Database,
    pub project_key: String,
    /// Install generation token (P0-A): every claim this context files lands
    /// in THIS generation — begin/promote/abort all key off it, never off a
    /// global MAX(generation).
    /// (Token generation của install (P0-A): mọi claim qua context này rơi
    /// vào generation NÀY — begin/promote/abort đều khóa theo nó, không bao
    /// giờ theo MAX(generation) toàn cục.)
    pub generation: i64,
}

/// Fail-closed claim context (Gate 11-A, P0-5): opening the claim DB is a
/// HARD prerequisite of a claiming install — the error propagates and the
/// install fails, exactly like the primary DB open. There is no code path
/// where blobs materialize unclaimed while the install still promotes.
/// (Context claim fail-closed (P0-5): mở DB claim là điều kiện TIÊN QUYẾT
/// cứng của install có claim — lỗi propagate và install fail, giống hệt mở
/// DB chính. Không tồn tại đường mà blob materialize không claim trong khi
/// install vẫn promote.)
pub fn cas_claim_context(layout: &Layout, generation: i64) -> MgResult<CasClaimContext> {
    let db = Database::open(&layout.db_path()).map_err(|e| {
        MgError::Store(format!(
            "claim database open failed (install refuses to materialize \
             unclaimed blobs — fail-closed, P0-5): {e}"
        ))
    })?;
    Ok(CasClaimContext {
        db,
        project_key: layout.root().to_string_lossy().into_owned(),
        generation,
    })
}

pub fn claim_ctx(ctx: &CasClaimContext) -> (&Database, &str, i64) {
    (&ctx.db, &ctx.project_key, ctx.generation)
}

impl CasClaimContext {
    pub fn as_ref(&self) -> (&Database, &str, i64) {
        (&self.db, &self.project_key, self.generation)
    }
}

pub fn extracted_cache_full_validation_enabled() -> bool {
    std::env::var("MAGICORE_WEB_VALIDATE_EXTRACTED_CACHE")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
}

pub fn locate_package_dir(extract_root: &Path) -> MgResult<PathBuf> {
    let package_dir = extract_root.join("package");
    if package_dir.is_dir() {
        return Ok(package_dir);
    }

    let first_dir = std::fs::read_dir(extract_root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.is_dir());

    first_dir.ok_or_else(|| {
        MgError::Other(format!(
            "extracted tarball missing package root in '{}'",
            extract_root.display()
        ))
    })
}

pub fn ensure_extracted_package_root(
    layout: &Layout,
    store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    pkg: &ResolvedPackage,
    tarball_path: &Path,
    generation: i64,
) -> MgResult<PathBuf> {
    let fast_marker = expected_extracted_package_marker_from_path(pkg, tarball_path)?;
    let claim = cas_claim_context(layout, generation)?;
    ensure_extracted_package_root_with_marker(
        layout,
        store,
        shared_cache,
        pkg,
        &fast_marker,
        |temp_root| {
            let file = std::fs::File::open(tarball_path).map_err(|err| {
                MgError::Other(format!(
                    "failed to open tarball '{}' for '{}': {}",
                    tarball_path.display(),
                    pkg.id.name_str(),
                    err
                ))
            })?;
            extract_tarball_to_cas_and_link(file, temp_root, store, Some(claim_ctx(&claim)))
                .map_err(|e| MgError::Other(e.to_string()))
        },
    )
}

pub fn ensure_extracted_package_root_from_bytes(
    layout: &Layout,
    store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    pkg: &ResolvedPackage,
    tarball_bytes: &[u8],
    generation: i64,
) -> MgResult<PathBuf> {
    let expected_marker = expected_extracted_package_marker_from_bytes(pkg, tarball_bytes)?;
    let claim = cas_claim_context(layout, generation)?;
    ensure_extracted_package_root_with_marker(
        layout,
        store,
        shared_cache,
        pkg,
        &expected_marker,
        |temp_root| {
            extract_tarball_to_cas_and_link(
                std::io::Cursor::new(tarball_bytes),
                temp_root,
                store,
                Some(claim_ctx(&claim)),
            )
            .map_err(|e| MgError::Other(e.to_string()))
        },
    )
}

pub fn ensure_extracted_package_root_with_marker<F>(
    layout: &Layout,
    _store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    pkg: &ResolvedPackage,
    expected_marker: &ExtractedPackageMarker,
    extract_into: F,
) -> MgResult<PathBuf>
where
    F: FnOnce(&Path) -> MgResult<()>,
{
    let canonical_root = shared_cache
        .map(|shared| shared.extracted_package_root(pkg))
        .unwrap_or_else(|| local_extracted_package_root(layout, pkg));
    let canonical_lock = extracted_package_root_lock(&canonical_root);
    let _canonical_guard = match canonical_lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if canonical_root.join("package.json").exists() {
        let marker = read_extracted_package_marker(&canonical_root)?;
        // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
        if let Some(marker) = marker.as_ref()
            && extracted_marker_matches_fast(marker, expected_marker)
            && extracted_marker_has_content_signature(marker)
            && (!extracted_cache_full_validation_enabled()
                || extracted_content_matches(&canonical_root, marker)?)
        {
            // Warm-cache reuse (Gate 11-B.2 Task D): skip the re-extraction +
            // rename, but STILL import the tarball's blobs into THIS project's
            // per-project CAS and file its refcount claims. A fresh project
            // that materializes from a warm shared cache must not end up with
            // an empty CAS (0 blobs) and zero live refs — its own doctor/prune
            // must be able to track the materialized package. The temp tree is
            // throwaway: only the CAS import + claims persist.
            // (Reuse warm-cache (Gate 11-B.2 Task D): bỏ qua extract lại +
            // rename, nhưng VẪN import blob của tarball vào CAS per-project và
            // ghi claim refcount. Project mới materialize từ warm shared cache
            // không được rơi vào CAS rỗng (0 blob) và 0 ref sống — doctor/prune
            // của chính nó phải theo dõi được package đã materialize. Cây temp
            // là throwaway: chỉ import CAS + claim tồn tại.)
            let throwaway = fresh_extract_temp_root(pkg, &canonical_root);
            if throwaway.exists() {
                std::fs::remove_dir_all(&throwaway).map_err(|err| {
                    MgError::Other(format!(
                        "failed to remove stale temp root '{}' for '{}': {}",
                        throwaway.display(),
                        pkg.id.name_str(),
                        err
                    ))
                })?;
            }
            let import_result = extract_into(&throwaway);
            if throwaway.exists() {
                let _ = std::fs::remove_dir_all(&throwaway);
            }
            import_result?;
            return Ok(canonical_root);
        }
    }

    let temp_root = fresh_extract_temp_root(pkg, &canonical_root);
    if temp_root.exists() {
        std::fs::remove_dir_all(&temp_root).map_err(|err| {
            MgError::Other(format!(
                "failed to remove stale temp root '{}' for '{}': {}",
                temp_root.display(),
                pkg.id.name_str(),
                err
            ))
        })?;
    }
    let extract_result: MgResult<()> = (|| {
        extract_into(&temp_root)?;
        let package_root = locate_package_dir(&temp_root)?;
        // Write the marker INTO the staging root BEFORE the rename so the
        // rename itself is the single atomic publish point: a concurrent
        // same-digest installer either sees the previous COMPLETE root
        // (marker included) or its own — never a half-published root
        // without a marker, which is what made the old post-rename marker
        // write unverifiable during a race.
        // (Ghi marker VÀO staging root TRƯỚC khi rename để chính rename
        // là điểm publish nguyên tử duy nhất: installer cùng digest chạy
        // đồng thời hoặc thấy root TRỌN VẸN trước đó (kèm marker) hoặc
        // thấy root của chính nó — không bao giờ thấy root nửa chừng không
        // marker, vốn khiến marker ghi SAU rename không thể verify khi đua.)
        write_extracted_package_marker(&package_root, expected_marker)?;
        if let Some(parent) = canonical_root.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                MgError::Other(format!(
                    "failed to create canonical parent '{}' for '{}': {}",
                    parent.display(),
                    pkg.id.name_str(),
                    err
                ))
            })?;
        }
        publish_extracted_root(
            pkg.id.name_str(),
            &package_root,
            &canonical_root,
            expected_marker,
        )
    })();
    if temp_root.exists() {
        let _ = std::fs::remove_dir_all(&temp_root);
    }
    extract_result?;
    // The marker already traveled inside the published root (pre-written
    // into staging) — no post-rename write, so nothing can observe a
    // published root that lacks its marker.
    // (Marker đã đi cùng root khi publish (ghi trước trong staging) —
    // không ghi sau rename, nên không gì nhìn thấy root đã publish mà
    // thiếu marker.)
    Ok(canonical_root)
}

/// Cross-process publish attempts before giving up — bounded, fail-closed.
/// (Số lần thử publish cross-process trước khi bỏ — có chặn trên, fail-closed.)
const PUBLISH_RACE_ATTEMPTS: usize = 4;

/// Backoff between publish attempts — long enough for a racing publisher to
/// finish its remove+rename, short enough not to stall a normal install.
/// (Backoff giữa các lần thử — đủ dài để publisher đua kịp remove+rename,
/// đủ ngắn để không làm chậm install thường.)
const PUBLISH_RACE_BACKOFF: std::time::Duration = std::time::Duration::from_millis(25);

/// Publish the extracted staging `package_root` into the canonical root
/// atomically and IDEMPOTENTLY (safe under same-digest concurrent installs).
///
/// The canonical root is a DETERMINISTIC SHARED path (same package+integrity
/// ⇒ same path, `shared_extracted_package_root`) that separate PROCESSES can
/// hit simultaneously — the in-process `extracted_package_root_lock` cannot
/// serialize them. The old publish was a cross-process TOCTOU ("exists? →
/// remove → rename"): a racer's rename landing between our check and our
/// rename made OUR rename fail with ENOTEMPTY ("Directory not empty") — the
/// flake seen in `concurrent_same_digest_does_not_double_store`.
///
/// Race contract (mirrors the CAS publish contract in
/// `mgc_store` compiled-cache `put` — the first COMPLETE writer wins, the
/// loser READS and VERIFIES the winner, never trusted blindly):
///   1. `rename(staging → canonical)` is the publish point (the marker is
///      already inside staging, so a published root is always complete).
///   2. On `AlreadyExists`/`DirectoryNotEmpty` a concurrent publisher won:
///      - its marker matches ours (same digest ⇒ identical content) → keep
///        the winner, discard our own staging (legit dedup, no double-store);
///      - stale/tampered/divergent winner → remove it + bounded retry
///        (last-writer-wins, same as the pre-fix sequential semantics).
///   3. Attempts are bounded: exhaustion returns the last OS error — no
///      faked success.
///
/// (Publish staging `package_root` vào root canonical nguyên tử và
/// IDEMPOTENT (an toàn khi install cùng digest chạy đồng thời).
///
/// Root canonical là path DÙNG CHUNG TẤT ĐỊNH (cùng package+integrity ⇒
/// cùng path, `shared_extracted_package_root`) mà các process RIÊNG BIỆT
/// có thể chạm cùng lúc — lock `extracted_package_root_lock` chỉ
/// trong-process, không tuần tự hóa được chúng. Publish cũ là TOCTOU
/// cross-process ("exists? → remove → rename"): rename của racer rơi vào
/// giữa check và rename của mình khiến rename CỦA MÌNH fail ENOTEMPTY
/// ("Directory not empty") — flake thấy ở
/// `concurrent_same_digest_does_not_double_store`.
///
/// Hợp đồng race (ảnh chiếu hợp đồng publish CAS trong `mgc_store`
/// compiled-cache `put` — writer HOÀN CHỈNH đầu tiên thắng, bên thua ĐỌC
/// và VERIFY winner, không tin mù):
///   1. `rename(staging → canonical)` là điểm publish (marker đã nằm
///      trong staging nên root đã publish luôn trọn vẹn).
///   2. Khi `AlreadyExists`/`DirectoryNotEmpty`, một publisher đồng thời
///      đã thắng:
///      - marker của nó khớp mình (cùng digest ⇒ nội dung giống hệt) →
///        giữ winner, bỏ staging của mình (dedup hợp lệ, không
///        double-store);
///      - winner stale/tamper/phân kỳ → remove + retry có chặn
///        (last-writer-wins, đúng ngữ nghĩa tuần tự trước khi sửa).
///   3. Số lần thử có chặn: cạn lượt trả lỗi OS cuối — không giả thành
///      công.)
fn publish_extracted_root(
    pkg_name: &str,
    package_root: &Path,
    canonical_root: &Path,
    expected_marker: &ExtractedPackageMarker,
) -> MgResult<()> {
    let mut last_err: Option<std::io::Error> = None;
    for _attempt in 0..PUBLISH_RACE_ATTEMPTS {
        if canonical_root.exists() {
            match std::fs::remove_dir_all(canonical_root) {
                Ok(()) => {}
                // A racer removed it between our check and our remove —
                // nothing stale left, go straight to the rename.
                // (Racer khác đã xóa giữa lúc mình check và lúc mình xóa —
                // không còn gì stale, đi thẳng vào rename.)
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    last_err = Some(err);
                    std::thread::sleep(PUBLISH_RACE_BACKOFF);
                    continue;
                }
            }
        }
        match std::fs::rename(package_root, canonical_root) {
            Ok(()) => return Ok(()),
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::DirectoryNotEmpty
                ) =>
            {
                // Lost the publish race to a concurrent installer of the
                // SAME canonical root. Verify the winner before reusing it.
                // (Thua race publish cho installer đồng thời vào CÙNG root
                // canonical. Verify winner trước khi tái sử dụng.)
                let winner_matches = read_extracted_package_marker(canonical_root)
                    .ok()
                    .flatten()
                    .is_some_and(|marker| {
                        extracted_marker_matches_fast(&marker, expected_marker)
                            && extracted_marker_has_content_signature(&marker)
                    });
                if winner_matches {
                    // Same digest ⇒ identical content: keep the winner, our
                    // staging copy is discarded by the caller's temp cleanup.
                    // (Cùng digest ⇒ nội dung giống hệt: giữ winner, bản
                    // staging của mình do caller dọn cùng temp.)
                    return Ok(());
                }
                // Stale/tampered/divergent winner — replace it (bounded).
                // (Winner stale/tamper/phân kỳ — thay thế (có chặn).)
                last_err = Some(err);
                std::thread::sleep(PUBLISH_RACE_BACKOFF);
                continue;
            }
            Err(err) => {
                return Err(MgError::Other(format!(
                    "failed to rename extracted '{}' to canonical '{}' for '{}': {}",
                    package_root.display(),
                    canonical_root.display(),
                    pkg_name,
                    err
                )));
            }
        }
    }
    Err(MgError::Other(format!(
        "failed to publish extracted '{}' to canonical '{}' for '{}' after {} \
         attempt(s) under concurrent publish race: {}",
        package_root.display(),
        canonical_root.display(),
        pkg_name,
        PUBLISH_RACE_ATTEMPTS,
        last_err
            .map(|err| err.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    )))
}
