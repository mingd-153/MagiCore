/// Archive extraction utilities
use anyhow::{Result, bail};
use flate2::read::GzDecoder;
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use tar::Archive;

/// Extract a gzip-compressed tarball to a destination directory.
///
/// Entries are validated before unpacking so a malicious archive cannot write
/// outside `dest` or create links/special files.
pub fn extract_tarball(tarball_path: &Path, dest: &Path) -> Result<()> {
    let file = File::open(tarball_path)?;
    extract_tarball_from_reader(file, dest)
}

/// Extract a gzip-compressed tarball from an arbitrary reader.
pub fn extract_tarball_from_reader<R: Read>(reader: R, dest: &Path) -> Result<()> {
    let decoder = GzDecoder::new(reader);
    let mut archive = Archive::new(decoder);

    std::fs::create_dir_all(dest)?;
    let dest_root = dest.canonicalize()?;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let rel_path = sanitize_archive_path(entry.path()?.as_ref())?;
        let target = dest_root.join(rel_path);

        if !target.starts_with(&dest_root) {
            bail!("tar entry escapes destination: {}", target.display());
        }

        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            bail!("tar links are not allowed: {}", target.display());
        }

        if entry_type.is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }

        // Some npm tarballs contain pax metadata entries. They are archive
        // metadata only and should not be materialized into the package tree.
        if matches!(entry_type.as_byte(), b'g' | b'x') {
            continue;
        }

        if !entry_type.is_file() {
            bail!("unsupported tar entry type for {}", target.display());
        }

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&target)?;
    }

    Ok(())
}

/// Extract a gzip-compressed tarball from an arbitrary reader directly into the CAS,
/// and publish independently verified copies under the destination directory.
/// Hashes and publishes files in parallel; the package cache is validated and rebuilt after a crash.
/// (Hash và publish file song song; cache package được xác minh và dựng lại sau crash.)
///
/// `claim` registers every imported blob against `project_root` in the store DB
/// (refcount wiring) — pass `None` when the caller has no database (e.g. tests).
/// The claim carries the install's GENERATION TOKEN (P0-A): claims land in the
/// caller's own generation, never in a global MAX(generation) bucket.
/// (Tham số claim: đăng ký mọi blob đã import vào bảng refcount cho project —
/// truyền None khi caller không có DB, ví dụ test. Claim mang TOKEN
/// generation của install (P0-A): claim rơi vào generation của chính caller,
/// không bao giờ vào sọt MAX(generation) toàn cục.)
pub fn extract_tarball_to_cas_and_link<R: Read>(
    reader: R,
    dest: &Path,
    store: &mgc_store::ContentStore,
    claim: Option<(&mgc_store::Database, &str, i64)>,
) -> Result<()> {
    extract_tarball_to_cas_and_link_inner(
        reader,
        dest,
        false,
        store,
        claim,
        ExtractionDurability::Durable,
    )
    .map(|_| ())
}

/// Extract a lock-verified tarball into a package cache whose files can be
/// recreated from that same verified archive. Callers must validate the cache
/// root before reusing it after a crash.
/// (Extract tarball đã xác minh theo lock vào cache package có thể dựng lại
/// từ archive đó. Caller phải xác minh root cache trước khi dùng lại sau crash.)
pub fn extract_tarball_to_rebuildable_cas_and_link<R: Read>(
    reader: R,
    dest: &Path,
    store: &mgc_store::ContentStore,
    claim: Option<(&mgc_store::Database, &str, i64)>,
) -> Result<()> {
    extract_tarball_to_cas_and_link_inner(
        reader,
        dest,
        false,
        store,
        claim,
        ExtractionDurability::Rebuildable,
    )
    .map(|_| ())
}

/// Import blobs from a standard npm `package/` tarball into CAS and verify
/// them against an already validated package root without creating a second
/// extracted tree. Returns `false` for non-standard archive roots so callers
/// can use the ordinary staging extraction path.
/// (Nạp blob từ tarball npm chuẩn `package/` vào CAS và xác minh với package
/// root đã kiểm tra mà không tạo cây extract thứ hai. Trả `false` nếu root
/// archive không chuẩn để caller dùng đường staging thông thường.)
pub fn extract_tarball_to_cas_and_verify_existing_root<R: Read>(
    reader: R,
    package_root: &Path,
    store: &mgc_store::ContentStore,
    claim: Option<(&mgc_store::Database, &str, i64)>,
) -> Result<bool> {
    extract_tarball_to_cas_and_link_inner(
        reader,
        package_root,
        true,
        store,
        claim,
        ExtractionDurability::Durable,
    )
}

/// Import an integrity-verified tarball into CAS after the caller has already
/// validated the existing package root's marker and complete file signature.
/// Missing/corrupt cache files must be detected and rebuilt before reuse.
/// (Nạp tarball đã xác minh vào CAS sau khi caller đã xác minh marker và chữ
/// ký đầy đủ của root package hiện có. File cache thiếu/hỏng phải được phát
/// hiện và dựng lại trước khi sử dụng.)
pub fn extract_tarball_to_rebuildable_cas_and_verify_existing_root<R: Read>(
    reader: R,
    package_root: &Path,
    store: &mgc_store::ContentStore,
    claim: Option<(&mgc_store::Database, &str, i64)>,
) -> Result<bool> {
    extract_tarball_to_cas_and_link_inner(
        reader,
        package_root,
        true,
        store,
        claim,
        ExtractionDurability::Rebuildable,
    )
}

#[derive(Clone, Copy)]
enum ExtractionDurability {
    Durable,
    Rebuildable,
}

fn extract_tarball_to_cas_and_link_inner<R: Read>(
    reader: R,
    dest: &Path,
    verify_existing_package_root: bool,
    store: &mgc_store::ContentStore,
    claim: Option<(&mgc_store::Database, &str, i64)>,
    durability: ExtractionDurability,
) -> Result<bool> {
    let profiling = std::env::var_os("MAGICORE_FETCHER_PROFILE").is_some();
    let total_started = std::time::Instant::now();
    let decoder = GzDecoder::new(reader);
    let mut archive = Archive::new(decoder);

    std::fs::create_dir_all(dest)?;
    let dest_root = dest.canonicalize()?;

    struct FileEntry {
        path: PathBuf,
        data: Vec<u8>,
        executable: bool,
    }
    let mut files_map = std::collections::HashMap::new();
    let mut directories = Vec::new();
    let archive_read_started = std::time::Instant::now();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let rel_path = sanitize_archive_path(entry.path()?.as_ref())?;
        let entry_type = entry.header().entry_type();
        if matches!(entry_type.as_byte(), b'g' | b'x') {
            continue;
        }
        let mapped_rel_path = if verify_existing_package_root {
            let mut components = rel_path.components();
            if !matches!(components.next(), Some(Component::Normal(part)) if part == "package") {
                return Ok(false);
            }
            let remainder: PathBuf = components.collect();
            if remainder.as_os_str().is_empty() && !entry_type.is_dir() {
                return Ok(false);
            }
            remainder
        } else {
            rel_path
        };
        let target = dest_root.join(mapped_rel_path);

        if !target.starts_with(&dest_root) {
            bail!("tar entry escapes destination: {}", target.display());
        }

        if entry_type.is_symlink() || entry_type.is_hard_link() {
            bail!("tar links are not allowed: {}", target.display());
        }

        if entry_type.is_dir() {
            directories.push(target);
            continue;
        }

        if !entry_type.is_file() {
            bail!("unsupported tar entry type for {}", target.display());
        }

        let mode = entry.header().mode().unwrap_or(0o644);
        let executable = mode & 0o111 != 0;

        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;

        files_map.insert(
            target.clone(),
            FileEntry {
                path: target,
                data,
                executable,
            },
        );
    }

    let files: Vec<_> = files_map.into_values().collect();
    let file_count = files.len();
    let archive_read_ms = archive_read_started.elapsed().as_millis() as u64;

    for dir in directories {
        std::fs::create_dir_all(dir)?;
    }

    let mut dirs: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    for file in &files {
        if let Some(parent) = file.path.parent() {
            dirs.insert(parent.to_path_buf());
        }
    }
    let mut dirs: Vec<_> = dirs.into_iter().collect();
    dirs.sort_by_key(|d| d.components().count());
    for dir in dirs {
        std::fs::create_dir_all(dir)?;
    }

    // The rebuildable mode is used only for verified package archives; ordinary
    // extractor entry points retain durable CAS writes and directory syncs.
    // (Chế độ có thể dựng lại chỉ dùng với archive package đã xác minh; API
    // extractor thông thường vẫn ghi CAS bền vững và sync thư mục.)
    let has_claim = claim.is_some();
    let cas_import_started = std::time::Instant::now();
    let import_result = match durability {
        ExtractionDurability::Durable => store.import_bytes_with_exec_batch(
            files
                .iter()
                .map(|file| (file.data.as_slice(), file.executable)),
        ),
        ExtractionDurability::Rebuildable => store.import_rebuildable_bytes_with_exec_batch(
            files
                .iter()
                .map(|file| (file.data.as_slice(), file.executable)),
        ),
    };
    let hashes = import_result.map_err(|e| anyhow::anyhow!("failed to import to CAS: {}", e))?;
    let cas_import_ms = cas_import_started.elapsed().as_millis();

    let mut imported = std::collections::HashSet::new();
    if has_claim {
        // Refcount keys use bare BLAKE3 hex; executable state stays in CAS address.
        // (Khóa refcount dùng BLAKE3 hex thuần; trạng thái executable nằm trong địa chỉ CAS.)
        imported.extend(hashes.iter().map(|hash| hash.as_hex().to_string()));
    }

    let file_export_started = std::time::Instant::now();
    let export_result = match durability {
        ExtractionDurability::Durable => store.export_batch_to(
            files
                .into_iter()
                .zip(hashes.iter().cloned())
                .map(|(file, hash)| (hash, file.path)),
        ),
        ExtractionDurability::Rebuildable => store.export_batch_to_rebuildable_root(
            files
                .into_iter()
                .zip(hashes.iter().cloned())
                .map(|(file, hash)| (hash, file.path)),
        ),
    };
    export_result.map_err(|e| anyhow::anyhow!("failed to export from CAS: {}", e))?;
    let file_export_ms = file_export_started.elapsed().as_millis();

    // Test-only failpoint (Gate 11-B.2): park after every blob is imported
    // into the CAS and exported, but BEFORE any refcount claim is filed —
    // a kill here leaves the staging generation claim-less (STALE) while
    // the blobs sit unclaimed (unreferenced, never counted by the doctor).
    // (Failpoint chỉ-cho-test: đỗ sau khi mọi blob đã import vào CAS và
    // export, nhưng TRƯỚC khi ghi claim refcount nào — kill ở đây để
    // staging generation không claim (STALE) trong khi blob nằm chưa tham
    // chiếu (unreferenced, doctor không bao giờ đếm).)
    mgc_store::failpoint::hit("after-cas-publish");

    if let Some((db, project_root, generation)) = claim {
        let mut hashes: Vec<String> = imported.into_iter().collect();
        hashes.sort_unstable();
        let hash_refs: Vec<&str> = hashes.iter().map(String::as_str).collect();
        if !hashes.is_empty() {
            // Fail-closed claims (Gate 11-A, P0-6 of vòng-11 audit): the
            // old code logged a claim failure as a warning and continued,
            // with a comment claiming the end-of-install promote would
            // "re-stamp claims" — false: promote only retires rows that
            // exist, it never re-derives the graph→blob mapping. A missed
            // claim meant the blob materialized in node_modules while the
            // DB refcount never vouched for it — the next prune ate the
            // warm-cache blob and, worse, a claim written by an OLDER
            // generation could be retired by this install's promote. A
            // claiming install whose claim write fails must FAIL the
            // extraction — over-claiming is safe, under-claiming is data
            // loss.
            // (Claim fail-closed (P0-6): code cũ log lỗi claim thành
            // warning rồi chạy tiếp, kèm comment cho rằng promote cuối
            // install sẽ "đóng lại claim" — SAI: promote chỉ nghỉ hưu row
            // đang có, không bao giờ dựng lại ánh xạ graph→blob. Claim bị
            // bỏ nghĩa là blob materialize trong node_modules trong khi
            // refcount DB không bảo chứng — prune kế tiếp xóa blob
            // warm-cache, và tệ hơn, claim do generation CŨ ghi có thể bị
            // promote của install này nghỉ hưu. Install có claim mà ghi
            // claim fail PHẢI FAIL extraction — claim thừa an toàn, claim
            // thiếu là mất dữ liệu.)
            db.cas_claim_batch(project_root, generation, &hash_refs)?;
            // Test-only failpoint (Gate 11-B.2): park right after the FIRST
            // tarball claim batch commits — a kill here leaves a claim-ful staging
            // generation (STALE when the baseline's promoted refset already
            // covers the hash, RETAINED otherwise — classifier decides).
            // (Failpoint chỉ-cho-test: đỗ ngay sau batch claim đầu tiên commit
            // — kill ở đây để lại staging generation có claim (STALE khi
            // refset promoted của baseline đã phủ hash, còn không RETAINED —
            // classifier quyết định).)
            mgc_store::failpoint::hit("after-first-claim");
        }
    }

    if profiling {
        eprintln!(
            "[magicore:fetcher-profile] files={} archive_read_ms={} cas_import_batch_ms={} file_export_batch_ms={} total_ms={}",
            file_count,
            archive_read_ms,
            cas_import_ms,
            file_export_ms,
            total_started.elapsed().as_millis()
        );
    }

    Ok(true)
}

fn sanitize_archive_path(path: &Path) -> Result<PathBuf> {
    let mut clean = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("unsafe tar entry path: {}", path.display());
            }
        }
    }

    if clean.as_os_str().is_empty() {
        bail!("empty tar entry path");
    }

    Ok(clean)
}
