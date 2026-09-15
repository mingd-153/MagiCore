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
/// and hardlink the files to the destination directory.
/// Uses Rayon to perform hashing, CAS writing, and hardlinking in parallel.
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
    use rayon::prelude::*;
    use std::sync::{Arc, Mutex};

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

        if matches!(entry_type.as_byte(), b'g' | b'x') {
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

    // Process all files in parallel: Hash (Blake3) -> Save to CAS -> Hardlink
    let imported: Arc<Mutex<std::collections::HashSet<String>>> =
        Arc::new(Mutex::new(std::collections::HashSet::new()));
    let has_claim = claim.is_some();
    files.into_par_iter().try_for_each(|file| -> Result<()> {
        let hash = store
            .import_bytes_with_exec(&file.data, file.executable)
            .map_err(|e| anyhow::anyhow!("failed to import to CAS: {}", e))?;

        store
            .export_to(&hash, &file.path)
            .map_err(|e| anyhow::anyhow!("failed to hardlink from CAS: {}", e))?;

        if has_claim {
            // Refcount key = blake3 hex (exec suffix ".exec" stripped by
            // prune_cas_blobs_under(), so claim the bare hex).
            // (Khóa refcount = blake3 hex — prune strip đuôi .exec nên claim
            //  đúng hex thuần. Accessor as_hex() — field private P0-1.)
            imported
                .lock()
                .expect("lock poisoned")
                .insert(hash.as_hex().to_string());
        }

        Ok(())
    })?;

    // Test-only failpoint (Gate 11-B.2): park after every blob is imported
    // into the CAS and hardlinked, but BEFORE any refcount claim is filed —
    // a kill here leaves the staging generation claim-less (STALE) while
    // the blobs sit unclaimed (unreferenced, never counted by the doctor).
    // (Failpoint chỉ-cho-test: đỗ sau khi mọi blob đã import vào CAS và
    // hardlink, nhưng TRƯỚC khi ghi claim refcount nào — kill ở đây để
    // staging generation không claim (STALE) trong khi blob nằm chưa tham
    // chiếu (unreferenced, doctor không bao giờ đếm).)
    mgc_store::failpoint::hit("after-cas-publish");

    if let Some((db, project_root, generation)) = claim {
        let mut conn = std::collections::HashSet::new();
        std::mem::swap(&mut conn, &mut imported.lock().expect("lock poisoned"));
        let mut first_claim = true;
        for name in conn {
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
            db.cas_claim(project_root, generation, &name)?;
            // Test-only failpoint (Gate 11-B.2): park right after the FIRST
            // claim commits — a kill here leaves a claim-ful staging
            // generation (STALE when the baseline's promoted refset already
            // covers the hash, RETAINED otherwise — classifier decides).
            // (Failpoint chỉ-cho-test: đỗ ngay sau khi claim ĐẦU TIÊN commit
            // — kill ở đây để lại staging generation có claim (STALE khi
            // refset promoted của baseline đã phủ hash, còn không RETAINED —
            // classifier quyết định).)
            if first_claim {
                first_claim = false;
                mgc_store::failpoint::hit("after-first-claim");
            }
        }
    }

    Ok(())
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
