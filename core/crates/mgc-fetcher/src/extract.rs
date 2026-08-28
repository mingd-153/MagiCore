/// Archive extraction utilities
use anyhow::{bail, Result};
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
/// (Tham số claim: đăng ký mọi blob đã import vào bảng refcount cho project —
///  truyền None khi caller không có DB, ví dụ test.)
pub fn extract_tarball_to_cas_and_link<R: Read>(
    reader: R,
    dest: &Path,
    store: &mgc_store::ContentStore,
    claim: Option<(&mgc_store::Database, &str)>,
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
            //  đúng hex thuần.)
            imported
                .lock()
                .expect("lock poisoned")
                .insert(hash.hash.clone());
        }

        Ok(())
    })?;

    if let Some((db, project_root)) = claim {
        let mut conn = std::collections::HashSet::new();
        std::mem::swap(&mut conn, &mut imported.lock().expect("lock poisoned"));
        for name in conn {
            let _ = db.cas_claim(project_root, &name).map_err(|e| {
                // Fail-soft: refcount is an optimization; prune's nlink net
                // still protects these blobs. Never block extraction on it.
                // (Fail-soft: refcount chỉ là tối ưu; prune còn lưới nlink
                //  bảo vệ blob. Không bao giờ chặn extract vì claim lỗi.)
                eprintln!("warning: failed to claim CAS blob {name}: {e}");
            });
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
