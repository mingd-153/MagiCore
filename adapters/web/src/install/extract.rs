//! `install/extract.rs` — CAS extraction, marker signature generation and validation.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::{
    collections::{HashMap, HashSet},
    fs::{File, Metadata, OpenOptions},
    io::Read,
};

use mgc_fetcher::extract::{
    extract_tarball_to_rebuildable_cas_and_link,
    extract_tarball_to_rebuildable_cas_and_verify_existing_root,
};
use mgc_store::{ContentStore, Database, Layout};
use mgc_types::adapter::ResolvedPackage;
use mgc_types::{MgError, MgResult, PackageId};
use rayon::prelude::*;
use sha2::{Digest, Sha256};

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
    let fast_marker = expected_extracted_package_marker_fast_from_path(pkg, tarball_path)?;
    let claim = cas_claim_context(layout, generation)?;
    if let Some(root) = try_reuse_verified_warm_root(
        layout,
        store,
        shared_cache,
        pkg,
        &fast_marker,
        || {
            std::fs::File::open(tarball_path).map_err(|err| {
                MgError::Other(format!(
                    "failed to open cached tarball '{}' for '{}': {}",
                    tarball_path.display(),
                    pkg.id.name_str(),
                    err
                ))
            })
        },
        &claim,
    )? {
        return Ok(root);
    }

    let expected_marker = expected_extracted_package_marker_from_path(pkg, tarball_path)?;
    ensure_extracted_package_root_with_marker(
        layout,
        store,
        shared_cache,
        pkg,
        &expected_marker,
        |temp_root| {
            let file = std::fs::File::open(tarball_path).map_err(|err| {
                MgError::Other(format!(
                    "failed to open tarball '{}' for '{}': {}",
                    tarball_path.display(),
                    pkg.id.name_str(),
                    err
                ))
            })?;
            extract_tarball_to_rebuildable_cas_and_link(
                file,
                temp_root,
                store,
                Some(claim_ctx(&claim)),
            )
            .map_err(|e| MgError::Other(e.to_string()))
        },
        |cached_root| {
            let file = std::fs::File::open(tarball_path).map_err(|err| {
                MgError::Other(format!(
                    "failed to open cached tarball '{}' for '{}': {}",
                    tarball_path.display(),
                    pkg.id.name_str(),
                    err
                ))
            })?;
            extract_tarball_to_rebuildable_cas_and_verify_existing_root(
                file,
                cached_root,
                store,
                Some(claim_ctx(&claim)),
            )
            .map_err(|e| MgError::Other(e.to_string()))
        },
    )
}

/// Try the one-pass warm path only when a complete identity marker exists.
/// Chỉ thử warm path một lượt khi có marker định danh đầy đủ.
fn try_reuse_verified_warm_root<R, F>(
    layout: &Layout,
    store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    pkg: &ResolvedPackage,
    fast_marker: &ExtractedPackageMarker,
    open_tarball: F,
    claim: &CasClaimContext,
) -> MgResult<Option<PathBuf>>
where
    R: Read,
    F: FnOnce() -> MgResult<R>,
{
    // A matching marker lets the archive verify and rehydrate a warm root in
    // one pass, avoiding a second tree hash and redundant exports.
    // Marker khớp cho phép archive xác minh và nạp lại root cache trong một
    // lượt, bỏ lần hash cây thứ hai và thao tác export dư thừa.
    let canonical_root = shared_cache
        .map(|shared| shared.extracted_package_root(pkg))
        .unwrap_or_else(|| local_extracted_package_root(layout, pkg));
    let warm_profile = std::env::var_os("MAGICORE_WEB_PROFILE_INSTALL").is_some();
    let canonical_lock = extracted_package_root_lock(&canonical_root);
    let _canonical_guard = match canonical_lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let mut warm_reason = "missing-package-root";
    let mut warm_hit = false;
    if canonical_root.join("package.json").exists() {
        warm_reason = "missing-marker";
        if let Some(marker) = read_extracted_package_marker(&canonical_root)? {
            warm_reason = "identity-mismatch";
            if extracted_marker_matches_fast(&marker, fast_marker) {
                warm_reason = "incomplete-marker";
                if extracted_marker_has_content_signature(&marker) {
                    warm_reason = "archive-cache-mismatch";
                    warm_hit = import_and_verify_cached_root_from_tarball(
                        open_tarball()?,
                        &canonical_root,
                        &marker,
                        store,
                        Some(claim_ctx(claim)),
                    )?;
                    if warm_hit {
                        warm_reason = "verified-hit";
                    }
                }
            }
        }
    }
    if warm_profile {
        eprintln!(
            "[magicore:web:warm-cache-profile] package={} result={warm_reason}",
            pkg.id
        );
    }
    Ok(warm_hit.then_some(canonical_root))
}

struct VerifiedCachedArchiveFile {
    archive_path: PathBuf,
    data: std::ops::Range<usize>,
    executable: bool,
    signature_executable: bool,
}

/// Verify a canonical extracted root against its lock-verified archive while
/// importing the archive's blobs into CAS, without rewriting the root.
/// Xác minh root đã extract với archive đã xác minh theo lock và nạp blob vào
/// CAS mà không ghi đè root.
pub fn import_and_verify_cached_root_from_tarball<R: Read>(
    reader: R,
    package_root: &Path,
    expected_marker: &ExtractedPackageMarker,
    store: &ContentStore,
    claim: Option<(&Database, &str, i64)>,
) -> MgResult<bool> {
    let profile = std::env::var_os("MAGICORE_WEB_PROFILE_INSTALL").is_some();
    let verify_started = std::time::Instant::now();
    if !extracted_marker_has_content_signature(expected_marker) {
        return Ok(false);
    }
    let root_metadata = std::fs::symlink_metadata(package_root).map_err(|err| {
        MgError::Other(format!(
            "failed to inspect cached package root '{}': {err}",
            package_root.display()
        ))
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Ok(false);
    }
    let Some(mut cached_files) = cached_package_files(package_root)? else {
        return Ok(false);
    };

    let decoder = flate2::read::GzDecoder::new(reader);
    let mut archive = tar::Archive::new(decoder);
    let mut files = Vec::<VerifiedCachedArchiveFile>::new();
    let mut archive_data = Vec::new();
    let mut archive_paths = HashSet::<PathBuf>::new();

    for entry in archive
        .entries()
        .map_err(|err| MgError::Other(format!("failed to read cached package archive: {err}")))?
    {
        let mut entry = entry
            .map_err(|err| MgError::Other(format!("failed to read cached archive entry: {err}")))?;
        let entry_type = entry.header().entry_type();
        if matches!(entry_type.as_byte(), b'g' | b'x') {
            continue;
        }
        let archive_path = sanitize_tarball_signature_path(
            entry
                .path()
                .map_err(|err| MgError::Other(format!("failed to read archive entry path: {err}")))?
                .as_ref(),
        )?;
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(MgError::Other(format!(
                "tar links are not allowed in cached package archive: {}",
                archive_path.display()
            )));
        }
        if entry_type.is_dir() {
            continue;
        }
        if !entry_type.is_file() {
            return Err(MgError::Other(format!(
                "unsupported tar entry type in cached package archive: {}",
                archive_path.display()
            )));
        }
        if !archive_paths.insert(archive_path.clone()) {
            return Ok(false);
        }

        let size = entry.header().size().map_err(|err| {
            MgError::Other(format!(
                "failed to read cached archive entry size '{}': {err}",
                archive_path.display()
            ))
        })?;
        let mode = entry.header().mode().map_err(|err| {
            MgError::Other(format!(
                "failed to read cached archive entry mode '{}': {err}",
                archive_path.display()
            ))
        })?;
        let signature_executable = mode_is_executable(mode);
        let cas_executable = mode & 0o111 != 0;
        let data_start = archive_data.len();
        entry.read_to_end(&mut archive_data).map_err(|err| {
            MgError::Other(format!(
                "failed to read cached archive entry '{}': {err}",
                archive_path.display()
            ))
        })?;
        let data_end = archive_data.len();
        if (data_end - data_start) as u64 != size {
            return Ok(false);
        }
        files.push(VerifiedCachedArchiveFile {
            archive_path,
            data: data_start..data_end,
            executable: cas_executable,
            signature_executable,
        });
    }

    let archive_paths_and_sizes = files
        .iter()
        .map(|file| {
            (
                path_to_signature_string(&file.archive_path),
                (file.data.end - file.data.start) as u64,
            )
        })
        .collect::<Vec<_>>();
    let root_prefix = common_tarball_root_prefix(&archive_paths_and_sizes);
    let mut files_to_verify = Vec::with_capacity(files.len());
    for file in &files {
        let relative_path = if root_prefix.is_some() {
            file.archive_path.components().skip(1).collect::<PathBuf>()
        } else {
            file.archive_path.clone()
        };
        if relative_path.as_os_str().is_empty() {
            return Ok(false);
        }
        let Some(cached_metadata) = cached_files.remove(&relative_path) else {
            return Ok(false);
        };
        files_to_verify.push((file, relative_path, cached_metadata));
    }
    if !cached_files.is_empty() {
        return Ok(false);
    }
    let verified_signature_files = files_to_verify
        .into_par_iter()
        .map(|(file, relative_path, cached_metadata)| {
            let expected_data = &archive_data[file.data.start..file.data.end];
            if !cached_archive_file_matches(
                package_root,
                &relative_path,
                cached_metadata,
                expected_data,
                file.signature_executable,
            )? {
                return Ok(None);
            }
            Ok(Some((
                path_to_signature_string(&relative_path),
                expected_data.len() as u64,
                file.signature_executable,
                compute_sha256_hex(expected_data),
            )))
        })
        .collect::<Result<Vec<_>, MgError>>()?;
    if verified_signature_files.iter().any(Option::is_none) {
        return Ok(false);
    }
    let mut signature_files = verified_signature_files
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    signature_files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut tree_hasher = Sha256::new();
    let mut unpacked_size = 0u64;
    for (path, size, executable, digest) in &signature_files {
        update_tree_hasher(&mut tree_hasher, path, *size, *executable, digest);
        unpacked_size = unpacked_size
            .checked_add(*size)
            .ok_or_else(|| MgError::Other("cached package size overflowed".to_string()))?;
    }
    if expected_marker.file_count != signature_files.len() as u64
        || expected_marker.unpacked_size != unpacked_size
        || expected_marker.file_tree_sha256 != hex::encode(tree_hasher.finalize())
    {
        return Ok(false);
    }
    let verify_ms = verify_started.elapsed().as_millis();

    let import_started = std::time::Instant::now();
    let hashes = store
        .import_rebuildable_bytes_with_exec_batch(files.iter().map(|file| {
            (
                &archive_data[file.data.start..file.data.end],
                file.executable,
            )
        }))
        .map_err(|err| {
            MgError::Store(format!(
                "failed to import verified cached package into CAS: {err}"
            ))
        })?;
    let cas_import_ms = import_started.elapsed().as_millis();
    mgc_store::failpoint::hit("after-cas-publish");
    let claim_started = std::time::Instant::now();
    if let Some((db, project_root, generation)) = claim {
        let mut imported = hashes
            .iter()
            .map(|hash| hash.as_hex().to_string())
            .collect::<Vec<_>>();
        imported.sort_unstable();
        imported.dedup();
        let hash_refs = imported.iter().map(String::as_str).collect::<Vec<_>>();
        if !hash_refs.is_empty() {
            db.cas_claim_batch(project_root, generation, &hash_refs)
                .map_err(|err| {
                    MgError::Store(format!(
                        "failed to claim verified cached package blobs: {err}"
                    ))
                })?;
            mgc_store::failpoint::hit("after-first-claim");
        }
    }
    if profile {
        eprintln!(
            "[magicore:web:warm-verify-profile] package_files={} unpacked_bytes={} verify_ms={verify_ms} cas_import_ms={cas_import_ms} claim_ms={}",
            signature_files.len(),
            unpacked_size,
            claim_started.elapsed().as_millis()
        );
    }

    Ok(true)
}

#[derive(Clone, Copy)]
pub(crate) struct CachedFileMetadata {
    len: u64,
    executable: bool,
    identity: CachedFileIdentity,
}

#[cfg(unix)]
type CachedFileIdentity = (u64, u64);
#[cfg(windows)]
type CachedFileIdentity = (u32, u64);
#[cfg(not(any(unix, windows)))]
type CachedFileIdentity = ();

#[allow(unsafe_code)]
pub(crate) fn cached_file_identity(file: &File, metadata: &Metadata) -> Option<CachedFileIdentity> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = file;
        Some((metadata.dev(), metadata.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };

        let mut information = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        // SAFETY: `file` owns a live handle and `information` is writable for
        // the full structure; Windows initializes it when the call succeeds.
        // (AN TOÀN: `file` sở hữu handle còn sống và `information` ghi được
        // toàn bộ struct; Windows khởi tạo struct khi lời gọi thành công.)
        let succeeded = unsafe {
            GetFileInformationByHandle(
                file.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE,
                information.as_mut_ptr(),
            )
        };
        if succeeded == 0 {
            return None;
        }
        // SAFETY: Windows returned success, so it initialized every field.
        // (AN TOÀN: Windows trả thành công nên mọi field đã được khởi tạo.)
        let information = unsafe { information.assume_init() };
        let file_index =
            (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow);
        Some((information.dwVolumeSerialNumber, file_index))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (file, metadata);
        Some(())
    }
}

/// Open one cache file without accepting a final symlink/reparse point.
/// Mở file cache và từ chối symlink/reparse point ở thành phần cuối.
fn open_cached_file_no_follow(
    package_root: &Path,
    relative_path: &Path,
) -> std::io::Result<Option<File>> {
    use std::path::Component;

    if relative_path.as_os_str().is_empty()
        || !relative_path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Ok(None);
    }

    #[cfg(unix)]
    let file = {
        use rustix::fs::{Mode, OFlags, openat};
        use rustix::io::Errno;
        use std::os::fd::AsFd;
        use std::os::unix::fs::OpenOptionsExt;

        fn missing_or_link(error: Errno) -> bool {
            matches!(error, Errno::NOENT | Errno::LOOP | Errno::NOTDIR)
        }

        let mut directory = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(package_root)
        {
            Ok(directory) => directory,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) if error.raw_os_error() == Some(libc::ELOOP) => return Ok(None),
            Err(error) => return Err(error),
        };
        let components = relative_path.components().collect::<Vec<_>>();
        for component in &components[..components.len() - 1] {
            let Component::Normal(name) = component else {
                return Ok(None);
            };
            // Resolve each directory from the already-open parent handle; a path-level
            // check alone could follow a replaced ancestor symlink.
            // Mở từng thư mục tương đối với handle cha; chỉ kiểm tra chuỗi path có thể đi theo symlink tổ tiên đã bị thay.
            let opened = openat(
                directory.as_fd(),
                *name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            );
            directory = match opened {
                Ok(opened) => File::from(opened),
                Err(error) if missing_or_link(error) => return Ok(None),
                Err(error) => return Err(error.into()),
            };
        }

        let Component::Normal(name) = &components[components.len() - 1] else {
            return Ok(None);
        };
        match openat(
            directory.as_fd(),
            *name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        ) {
            Ok(opened) => File::from(opened),
            Err(error) if missing_or_link(error) => return Ok(None),
            Err(error) => return Err(error.into()),
        }
    };
    #[cfg(windows)]
    let file = {
        let target = package_root.join(relative_path);
        if !windows_cached_path_is_safe(package_root, relative_path)? {
            return Ok(None);
        }
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        let root_handle = match OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(package_root)
        {
            Ok(root) => root,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if root_handle.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Ok(None);
        }
        let root_final_path = windows_final_path(&root_handle)?;
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&target)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Ok(None);
        }
        if !windows_opened_path_is_within(&root_final_path, &windows_final_path(&file)?) {
            return Ok(None);
        }
        if !windows_cached_path_is_safe(package_root, relative_path)? {
            return Ok(None);
        }
        file
    };
    #[cfg(not(any(unix, windows)))]
    let file = match File::open(package_root.join(relative_path)) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };

    if !file.metadata()?.is_file() {
        return Ok(None);
    }
    Ok(Some(file))
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn windows_final_path(file: &File) -> std::io::Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_NAME_NORMALIZED, GetFinalPathNameByHandleW, VOLUME_NAME_DOS,
    };

    let handle = file.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
    // SAFETY: `file` owns a valid open Windows handle; a null output buffer with
    // length zero is the documented size query for this API.
    // (AN TOÀN: `file` sở hữu handle Windows đang mở; buffer null với độ dài 0
    // là cách API quy định để hỏi kích thước.)
    let required = unsafe {
        GetFinalPathNameByHandleW(
            handle,
            std::ptr::null_mut(),
            0,
            FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
        )
    };
    if required == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut buffer = vec![0u16; required as usize];
    // SAFETY: `buffer` is writable for `required` UTF-16 code units and the
    // handle remains owned by `file` for the duration of the call.
    // (AN TOÀN: `buffer` ghi được `required` đơn vị UTF-16 và `file` vẫn giữ
    // handle hợp lệ trong suốt lời gọi.)
    let written = unsafe {
        GetFinalPathNameByHandleW(
            handle,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
        )
    };
    if written == 0 || written >= buffer.len() as u32 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(PathBuf::from(std::ffi::OsString::from_wide(
        &buffer[..written as usize],
    )))
}

#[cfg(windows)]
fn windows_opened_path_is_within(root: &Path, target: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;

    let root = root.as_os_str().encode_wide().collect::<Vec<_>>();
    let target = target.as_os_str().encode_wide().collect::<Vec<_>>();
    if target.len() <= root.len() || !target.starts_with(&root) {
        return false;
    }
    root.last() == Some(&(b'\\' as u16)) || target.get(root.len()) == Some(&(b'\\' as u16))
}

#[cfg(windows)]
fn windows_cached_path_is_safe(package_root: &Path, relative_path: &Path) -> std::io::Result<bool> {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    let root_metadata = std::fs::symlink_metadata(package_root)?;
    if root_metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Ok(false);
    }
    let canonical_root = std::fs::canonicalize(package_root)?;
    let mut current = package_root.to_path_buf();
    for component in relative_path.components() {
        current.push(component.as_os_str());
        let metadata = std::fs::symlink_metadata(&current)?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Ok(false);
        }
    }
    Ok(std::fs::canonicalize(current)?.starts_with(canonical_root))
}

pub(crate) fn cached_archive_file_matches(
    package_root: &Path,
    relative_path: &Path,
    cached_metadata: CachedFileMetadata,
    expected_data: &[u8],
    expected_executable: bool,
) -> MgResult<bool> {
    if cached_metadata.len != expected_data.len() as u64
        || cached_metadata.executable != expected_executable
    {
        return Ok(false);
    }
    let target = package_root.join(relative_path);
    let Some(mut file) =
        open_cached_file_no_follow(package_root, relative_path).map_err(|err| {
            MgError::Other(format!(
                "failed to open cached package file '{}': {err}",
                target.display()
            ))
        })?
    else {
        return Ok(false);
    };
    let opened_metadata = file.metadata().map_err(|err| {
        MgError::Other(format!(
            "failed to inspect opened cached package file '{}': {err}",
            target.display()
        ))
    })?;
    if cached_file_identity(&file, &opened_metadata) != Some(cached_metadata.identity)
        || opened_metadata.len() != cached_metadata.len
        || metadata_is_executable(&opened_metadata) != cached_metadata.executable
    {
        return Ok(false);
    }
    let mut offset = 0usize;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|err| {
            MgError::Other(format!(
                "failed to compare cached package file '{}': {err}",
                target.display()
            ))
        })?;
        if read == 0 {
            return Ok(offset == expected_data.len());
        }
        let Some(end) = offset.checked_add(read) else {
            return Ok(false);
        };
        if end > expected_data.len() || buffer[..read] != expected_data[offset..end] {
            return Ok(false);
        }
        offset = end;
    }
}

pub(crate) fn cached_package_files(
    root: &Path,
) -> MgResult<Option<HashMap<PathBuf, CachedFileMetadata>>> {
    let mut files = HashMap::new();
    for entry in walkdir::WalkDir::new(root).min_depth(1) {
        let entry = entry.map_err(|err| {
            MgError::Other(format!(
                "failed to walk cached package '{}': {err}",
                root.display()
            ))
        })?;
        if entry.path() == extracted_package_marker_path(root) {
            continue;
        }
        if entry.file_type().is_dir() {
            continue;
        }
        if !entry.file_type().is_file() {
            return Ok(None);
        }
        let relative = entry.path().strip_prefix(root).map_err(|err| {
            MgError::Other(format!(
                "failed to inspect cached package path '{}': {err}",
                entry.path().display()
            ))
        })?;
        let Some(file) = open_cached_file_no_follow(root, relative).map_err(|err| {
            MgError::Other(format!(
                "failed to securely open cached package file '{}': {err}",
                entry.path().display()
            ))
        })?
        else {
            return Ok(None);
        };
        let metadata = file.metadata().map_err(|err| {
            MgError::Other(format!(
                "failed to inspect opened cached package file '{}': {err}",
                entry.path().display()
            ))
        })?;
        let Some(identity) = cached_file_identity(&file, &metadata) else {
            return Ok(None);
        };
        files.insert(
            relative.to_path_buf(),
            CachedFileMetadata {
                len: metadata.len(),
                executable: metadata_is_executable(&metadata),
                identity,
            },
        );
    }
    Ok(Some(files))
}

pub fn ensure_extracted_package_root_from_bytes(
    layout: &Layout,
    store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    pkg: &ResolvedPackage,
    tarball_bytes: &[u8],
    generation: i64,
) -> MgResult<PathBuf> {
    let fast_marker = expected_extracted_package_marker_fast(pkg, tarball_bytes);
    let claim = cas_claim_context(layout, generation)?;
    if let Some(root) = try_reuse_verified_warm_root(
        layout,
        store,
        shared_cache,
        pkg,
        &fast_marker,
        || Ok(std::io::Cursor::new(tarball_bytes)),
        &claim,
    )? {
        return Ok(root);
    }

    let expected_marker = expected_extracted_package_marker_from_bytes(pkg, tarball_bytes)?;
    ensure_extracted_package_root_with_marker(
        layout,
        store,
        shared_cache,
        pkg,
        &expected_marker,
        |temp_root| {
            extract_tarball_to_rebuildable_cas_and_link(
                std::io::Cursor::new(tarball_bytes),
                temp_root,
                store,
                Some(claim_ctx(&claim)),
            )
            .map_err(|e| MgError::Other(e.to_string()))
        },
        |cached_root| {
            extract_tarball_to_rebuildable_cas_and_verify_existing_root(
                std::io::Cursor::new(tarball_bytes),
                cached_root,
                store,
                Some(claim_ctx(&claim)),
            )
            .map_err(|e| MgError::Other(e.to_string()))
        },
    )
}

pub fn ensure_extracted_package_root_with_marker<F, C>(
    layout: &Layout,
    _store: &ContentStore,
    shared_cache: Option<&SharedWebCache>,
    pkg: &ResolvedPackage,
    expected_marker: &ExtractedPackageMarker,
    extract_into: F,
    import_cached_root: C,
) -> MgResult<PathBuf>
where
    F: FnOnce(&Path) -> MgResult<()>,
    C: FnOnce(&Path) -> MgResult<bool>,
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
            && marker == expected_marker
            && extracted_marker_matches_fast(marker, expected_marker)
            && extracted_marker_has_content_signature(marker)
            && extracted_content_matches(&canonical_root, expected_marker)?
        {
            // Reuse the root only when its complete marker matches the
            // tarball-derived signature. Import and claim directly against
            // the verified tree; unsupported archive layouts fall through
            // to the ordinary staging path.
            // (Chỉ reuse root khi marker đầy đủ khớp chữ ký từ tarball. Nạp
            // CAS và claim trực tiếp với cây đã xác minh; layout không hỗ trợ
            // sẽ quay về đường staging thông thường.)
            if import_cached_root(&canonical_root)? {
                return Ok(canonical_root);
            }
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
            // Same-digest racer already published a GOOD winner: keep it
            // (dedup, no double-store) — NEVER remove-then-replace a root
            // whose marker matches, because a concurrent install may be
            // linking FROM it right now (ENOENT flake in
            // `concurrent_same_digest_does_not_double_store`: the old code
            // removed the winner another process had just published and
            // was about to link).
            // Always validate content: a marker cannot prove that a cached
            // root is complete or untampered. A matching winner is immutable
            // for this publish decision and must never be removed.
            // Luôn xác thực nội dung: marker không chứng minh cây cache đủ
            // file hay chưa bị sửa. Winner khớp không bị xóa trong quyết định này.
            let winner_matches = match read_extracted_package_marker(canonical_root)? {
                Some(marker)
                    if extracted_marker_matches_fast(&marker, expected_marker)
                        && extracted_marker_has_content_signature(&marker) =>
                {
                    extracted_content_matches(canonical_root, expected_marker)?
                }
                _ => false,
            };
            if winner_matches {
                return Ok(());
            }
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
                let winner_matches = match read_extracted_package_marker(canonical_root)? {
                    Some(marker)
                        if extracted_marker_matches_fast(&marker, expected_marker)
                            && extracted_marker_has_content_signature(&marker) =>
                    {
                        extracted_content_matches(canonical_root, expected_marker)?
                    }
                    _ => false,
                };
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
