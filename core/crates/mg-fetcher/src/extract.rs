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

    #[test]
    fn test_extract_skips_pax_global_header_entries() {
        let temp_dir = TempDir::new().unwrap();
        let tarball = temp_dir.path().join("pax.tgz");

        let file = File::create(&tarball).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        let mut pax = Header::new_gnu();
        pax.set_entry_type(tar::EntryType::new(b'g'));
        pax.set_size(0);
        pax.set_mode(0o644);
        pax.set_cksum();
        builder
            .append_data(&mut pax, "package/pax_global_header", std::io::empty())
            .unwrap();

        let mut file_header = Header::new_gnu();
        file_header.set_size(2);
        file_header.set_mode(0o644);
        file_header.set_cksum();
        builder
            .append_data(&mut file_header, "package/index.js", &b"ok"[..])
            .unwrap();

        builder.finish().unwrap();
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();

        let dest = temp_dir.path().join("out");
        extract_tarball(&tarball, &dest).unwrap();

        assert!(dest.join("package/index.js").exists());
        assert!(!dest.join("package/pax_global_header").exists());
    }

    #[test]
    fn test_sanitize_normal_path() {
        let result = sanitize_archive_path(Path::new("package/index.js")).unwrap();
        assert_eq!(result, PathBuf::from("package/index.js"));
    }

    #[test]
    fn test_sanitize_strips_curdir() {
        let result = sanitize_archive_path(Path::new("./foo/bar")).unwrap();
        assert_eq!(result, PathBuf::from("foo/bar"));
    }

    #[test]
    fn test_sanitize_rejects_root() {
        let err = sanitize_archive_path(Path::new("/foo")).unwrap_err();
        assert!(err.to_string().contains("unsafe tar entry path"));
    }

    #[test]
    fn test_sanitize_rejects_empty() {
        let err = sanitize_archive_path(Path::new("")).unwrap_err();
        assert!(err.to_string().contains("empty tar entry path"));
    }

    #[test]
    fn test_sanitize_preserves_multiple_dots() {
        let result = sanitize_archive_path(Path::new("foo/...")).unwrap();
        assert_eq!(result, PathBuf::from("foo/..."));
    }

    #[test]
    fn test_extract_directory_entry() {
        let temp_dir = TempDir::new().unwrap();
        let tarball = temp_dir.path().join("dir.tgz");

        let file = File::create(&tarball).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        let mut dir_header = Header::new_gnu();
        dir_header.set_entry_type(tar::EntryType::Directory);
        dir_header.set_size(0);
        dir_header.set_mode(0o755);
        dir_header.set_cksum();
        builder
            .append_data(&mut dir_header, "mydir", std::io::empty())
            .unwrap();

        let mut file_header = Header::new_gnu();
        file_header.set_size(4);
        file_header.set_mode(0o644);
        file_header.set_cksum();
        builder
            .append_data(&mut file_header, "mydir/file.txt", &b"data"[..])
            .unwrap();

        builder.finish().unwrap();
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();

        let dest = temp_dir.path().join("out");
        extract_tarball(&tarball, &dest).unwrap();
        assert!(dest.join("mydir").is_dir());
        assert_eq!(std::fs::read(dest.join("mydir/file.txt")).unwrap(), b"data");
    }

    #[test]
    fn test_extract_nested_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let tarball = temp_dir.path().join("nested.tgz");

        let file = File::create(&tarball).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        for dir in &["a", "a/b", "a/b/c"] {
            let mut h = Header::new_gnu();
            h.set_entry_type(tar::EntryType::Directory);
            h.set_size(0);
            h.set_mode(0o755);
            h.set_cksum();
            builder.append_data(&mut h, dir, std::io::empty()).unwrap();
        }

        let mut fh = Header::new_gnu();
        fh.set_size(4);
        fh.set_mode(0o644);
        fh.set_cksum();
        builder
            .append_data(&mut fh, "a/b/c/file.txt", &b"data"[..])
            .unwrap();

        builder.finish().unwrap();
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();

        let dest = temp_dir.path().join("out");
        extract_tarball(&tarball, &dest).unwrap();
        assert!(dest.join("a").is_dir());
        assert!(dest.join("a/b").is_dir());
        assert!(dest.join("a/b/c").is_dir());
        assert_eq!(std::fs::read(dest.join("a/b/c/file.txt")).unwrap(), b"data");
    }

    #[test]
    fn test_extract_rejects_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let tarball = temp_dir.path().join("symlink.tgz");

        let file = File::create(&tarball).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        let mut header = Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_link(&mut header, "mylink", "target")
            .unwrap();

        builder.finish().unwrap();
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();

        let dest = temp_dir.path().join("out");
        let err = extract_tarball(&tarball, &dest).unwrap_err();
        assert!(err.to_string().contains("links are not allowed"));
    }

    #[test]
    fn test_extract_tarball_rejects_absolute_path() {
        let temp_dir = TempDir::new().unwrap();
        let tarball = temp_dir.path().join("abs.tgz");

        // Builder rejects absolute paths by default; enable preservation to
        // construct a tarball with an absolute entry path.
        let file = File::create(&tarball).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);
        builder.preserve_absolute(true);

        let mut header = Header::new_gnu();
        header.set_size(4);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "/etc/passwd", &b"evil"[..])
            .unwrap();

        builder.finish().unwrap();
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();

        let dest = temp_dir.path().join("out");
        let err = extract_tarball(&tarball, &dest).unwrap_err();
        assert!(err.to_string().contains("unsafe tar entry path"));
    }
}
