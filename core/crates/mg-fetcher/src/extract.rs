/// Archive extraction utilities
use anyhow::{bail, Result};
use flate2::read::GzDecoder;
use std::fs::File;
use std::path::{Component, Path, PathBuf};
use tar::Archive;

/// Extract a gzip-compressed tarball to a destination directory.
///
/// Entries are validated before unpacking so a malicious archive cannot write
/// outside `dest` or create links/special files.
pub fn extract_tarball(tarball_path: &Path, dest: &Path) -> Result<()> {
    let file = File::open(tarball_path)?;
    let decoder = GzDecoder::new(file);
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

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use tar::{Builder, Header};
    use tempfile::TempDir;

    #[test]
    fn test_extract_regular_file() {
        let temp_dir = TempDir::new().unwrap();
        let tarball = temp_dir.path().join("test.tgz");
        write_test_tarball(&tarball, "package/index.js", b"console.log('ok');");

        let dest = temp_dir.path().join("out");
        extract_tarball(&tarball, &dest).unwrap();

        assert_eq!(
            std::fs::read(dest.join("package/index.js")).unwrap(),
            b"console.log('ok');"
        );
    }

    #[test]
    fn test_sanitize_archive_path_rejects_parent_dir() {
        let err = sanitize_archive_path(Path::new("../evil.txt")).unwrap_err();
        assert!(err.to_string().contains("unsafe tar entry path"));
    }

    fn write_test_tarball(path: &Path, entry_path: &str, data: &[u8]) {
        let file = File::create(path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);
        let mut header = Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, entry_path, data).unwrap();
        builder.finish().unwrap();
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();
    }
}
