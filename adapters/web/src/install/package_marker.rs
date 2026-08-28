//! Extracted package marker signatures — validates cached package roots.
//! Chữ ký marker gói đã extract — tách khỏi CAS extraction để dễ audit cache.

use std::path::{Path, PathBuf};

use hex;
use mgc_store::Layout;
use mgc_types::adapter::ResolvedPackage;
use mgc_types::{MgError, MgResult, PackageId};
use sha2::{Digest, Sha256};
use tar;
use walkdir::WalkDir;

use crate::cache::{
    shared_extracted_package_root, ExtractedPackageMarker, TarballContentSignature,
};
use crate::lockfile::installed_package_matches;
use crate::manifest::atomic_write;

pub fn compute_sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn compute_sha256_hex_from_path(path: &Path) -> MgResult<String> {
    let mut file = std::fs::File::open(path).map_err(|err| {
        MgError::Other(format!(
            "failed to open '{}' for sha256: {}",
            path.display(),
            err
        ))
    })?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|err| {
        MgError::Other(format!(
            "failed to hash '{}' with sha256: {}",
            path.display(),
            err
        ))
    })?;
    Ok(hex::encode(hasher.finalize()))
}

pub fn local_extracted_package_root(layout: &Layout, pkg: &ResolvedPackage) -> PathBuf {
    shared_extracted_package_root(layout.root(), pkg)
}

pub fn extracted_package_marker_path(root: &Path) -> PathBuf {
    root.join(".magicore-package-root.json")
}

pub fn expected_extracted_package_marker_from_bytes(
    pkg: &ResolvedPackage,
    tarball_bytes: &[u8],
) -> MgResult<ExtractedPackageMarker> {
    let mut marker = expected_extracted_package_marker_fast(pkg, tarball_bytes);
    let content = tarball_content_signature(tarball_bytes)?;
    marker.file_count = content.file_count;
    marker.unpacked_size = content.unpacked_size;
    marker.file_tree_sha256 = content.file_tree_sha256;
    Ok(marker)
}

pub fn expected_extracted_package_marker_from_path(
    pkg: &ResolvedPackage,
    tarball_path: &Path,
) -> MgResult<ExtractedPackageMarker> {
    let tarball_fingerprint = if pkg.integrity.is_empty() {
        compute_sha256_hex_from_path(tarball_path)?
    } else {
        format!("integrity:{}", pkg.integrity)
    };
    let content = tarball_content_signature_from_path(tarball_path)?;
    Ok(ExtractedPackageMarker {
        schema_version: 2,
        name: pkg.id.name_str().to_string(),
        version: pkg.id.version().to_string(),
        integrity: (!pkg.integrity.is_empty()).then(|| pkg.integrity.clone()),
        tarball_sha256: tarball_fingerprint,
        file_count: content.file_count,
        unpacked_size: content.unpacked_size,
        file_tree_sha256: content.file_tree_sha256,
    })
}

pub fn expected_extracted_package_marker_fast(
    pkg: &ResolvedPackage,
    tarball_bytes: &[u8],
) -> ExtractedPackageMarker {
    let tarball_fingerprint = if pkg.integrity.is_empty() {
        compute_sha256_hex(tarball_bytes)
    } else {
        format!("integrity:{}", pkg.integrity)
    };
    ExtractedPackageMarker {
        schema_version: 2,
        name: pkg.id.name_str().to_string(),
        version: pkg.id.version().to_string(),
        integrity: (!pkg.integrity.is_empty()).then(|| pkg.integrity.clone()),
        tarball_sha256: tarball_fingerprint,
        file_count: 0,
        unpacked_size: 0,
        file_tree_sha256: String::new(),
    }
}

pub fn extracted_marker_matches_fast(
    marker: &ExtractedPackageMarker,
    expected: &ExtractedPackageMarker,
) -> bool {
    marker.schema_version == 2
        && marker.name == expected.name
        && marker.version == expected.version
        && marker.integrity == expected.integrity
        && marker.tarball_sha256 == expected.tarball_sha256
}

pub fn extracted_marker_has_content_signature(marker: &ExtractedPackageMarker) -> bool {
    marker.file_count > 0 && marker.unpacked_size > 0 && !marker.file_tree_sha256.trim().is_empty()
}

pub fn tarball_content_signature(tarball_bytes: &[u8]) -> MgResult<TarballContentSignature> {
    tarball_content_signature_from_reader(std::io::Cursor::new(tarball_bytes))
}

pub fn tarball_content_signature_from_path(
    tarball_path: &Path,
) -> MgResult<TarballContentSignature> {
    let file = std::fs::File::open(tarball_path).map_err(|err| {
        MgError::Other(format!(
            "failed to open tarball '{}' for content signature: {}",
            tarball_path.display(),
            err
        ))
    })?;
    tarball_content_signature_from_reader(file)
}

pub fn tarball_content_signature_from_reader<R: std::io::Read>(
    reader: R,
) -> MgResult<TarballContentSignature> {
    let decoder = flate2::read::GzDecoder::new(reader);
    let mut archive = tar::Archive::new(decoder);
    let mut files = Vec::<(String, u64)>::new();

    for entry in archive
        .entries()
        .map_err(|err| MgError::Other(format!("failed to read tarball entries: {err}")))?
    {
        let entry =
            entry.map_err(|err| MgError::Other(format!("failed to read tarball entry: {err}")))?;
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() || matches!(entry_type.as_byte(), b'g' | b'x') {
            continue;
        }
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(MgError::Other(format!(
                "tar links are not allowed in cached package signature: {}",
                entry
                    .path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            )));
        }
        if !entry_type.is_file() {
            return Err(MgError::Other(format!(
                "unsupported tar entry type in cached package signature: {}",
                entry
                    .path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            )));
        }

        let path = sanitize_tarball_signature_path(
            entry
                .path()
                .map_err(|err| MgError::Other(format!("failed to read tarball entry path: {err}")))?
                .as_ref(),
        )?;
        let size = entry.header().size().map_err(|err| {
            MgError::Other(format!(
                "failed to read tarball entry size '{}': {err}",
                path.display()
            ))
        })?;
        files.push((path_to_signature_string(&path), size));
    }

    let root_prefix = common_tarball_root_prefix(&files);
    let mut normalized = files
        .into_iter()
        .filter_map(|(path, size)| {
            let stripped = root_prefix
                .as_ref()
                .and_then(|prefix| path.strip_prefix(prefix).and_then(|p| p.strip_prefix('/')))
                .unwrap_or(path.as_str());
            if stripped.is_empty() {
                None
            } else {
                Some((stripped.to_string(), size))
            }
        })
        .collect::<Vec<_>>();
    normalized.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    let mut unpacked_size = 0u64;
    for (path, size) in &normalized {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(size.to_string().as_bytes());
        hasher.update(*b"\n");
        unpacked_size = unpacked_size.saturating_add(*size);
    }

    Ok(TarballContentSignature {
        file_count: normalized.len() as u64,
        unpacked_size,
        file_tree_sha256: hex::encode(hasher.finalize()),
    })
}

pub fn extracted_content_matches(root: &Path, expected: &ExtractedPackageMarker) -> MgResult<bool> {
    if expected.file_tree_sha256.is_empty() {
        return Ok(false);
    }

    let mut files = Vec::<(String, u64)>::new();
    for entry in WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.path() == extracted_package_marker_path(root) {
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(root).map_err(|err| {
            MgError::Other(format!(
                "failed to inspect extracted package path '{}': {}",
                entry.path().display(),
                err
            ))
        })?;
        let size = entry
            .metadata()
            .map_err(|err| {
                MgError::Other(format!(
                    "failed to inspect extracted package file '{}': {}",
                    entry.path().display(),
                    err
                ))
            })?
            .len();
        files.push((path_to_signature_string(rel), size));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    let mut unpacked_size = 0u64;
    for (path, size) in &files {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(size.to_string().as_bytes());
        hasher.update(*b"\n");
        unpacked_size = unpacked_size.saturating_add(*size);
    }

    Ok(expected.file_count == files.len() as u64
        && expected.unpacked_size == unpacked_size
        && expected.file_tree_sha256 == hex::encode(hasher.finalize()))
}

pub fn sanitize_tarball_signature_path(path: &Path) -> MgResult<PathBuf> {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => clean.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(MgError::Other(format!(
                    "unsafe tar entry path in cached package signature: {}",
                    path.display()
                )));
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(MgError::Other(
            "empty tar entry path in cached package signature".to_string(),
        ));
    }
    Ok(clean)
}

pub fn path_to_signature_string(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => Some(part.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub fn common_tarball_root_prefix(files: &[(String, u64)]) -> Option<String> {
    let mut iter = files.iter().filter_map(|(path, _)| path.split('/').next());
    let first = iter.next()?.to_string();
    if first.is_empty() {
        return None;
    }
    if iter.all(|part| part == first) {
        Some(first)
    } else {
        None
    }
}

pub fn read_extracted_package_marker(root: &Path) -> MgResult<Option<ExtractedPackageMarker>> {
    let path = extracted_package_marker_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&path).map_err(|err| {
        MgError::Other(format!(
            "failed to read extracted package marker '{}': {}",
            path.display(),
            err
        ))
    })?;
    let marker = serde_json::from_str(&contents).map_err(|err| {
        MgError::Other(format!(
            "failed to parse extracted package marker '{}': {}",
            path.display(),
            err
        ))
    })?;
    Ok(Some(marker))
}

pub fn write_extracted_package_marker(
    root: &Path,
    marker: &ExtractedPackageMarker,
) -> MgResult<()> {
    let path = extracted_package_marker_path(root);
    let payload = serde_json::to_vec_pretty(marker).map_err(|err| {
        MgError::Other(format!(
            "failed to serialize extracted package marker '{}': {}",
            path.display(),
            err
        ))
    })?;
    atomic_write(&path, &payload)?;
    Ok(())
}

pub fn materialized_package_matches(
    target_root: &Path,
    package_id: &PackageId,
    source_marker: Option<&ExtractedPackageMarker>,
) -> MgResult<bool> {
    if !installed_package_matches(target_root, package_id) {
        return Ok(false);
    }

    let Some(source_marker) = source_marker else {
        return Ok(true);
    };

    let Some(target_marker) = read_extracted_package_marker(target_root)? else {
        return Ok(false);
    };

    Ok(target_marker == *source_marker)
}

pub fn write_materialized_package_marker(
    target_root: &Path,
    source_marker: Option<&ExtractedPackageMarker>,
) -> MgResult<()> {
    if let Some(marker) = source_marker {
        write_extracted_package_marker(target_root, marker)?;
    }
    Ok(())
}
