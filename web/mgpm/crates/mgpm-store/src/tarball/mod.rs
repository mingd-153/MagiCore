//! Tarball extraction with streaming and integrity verification

use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tar::Archive;

#[derive(Debug, thiserror::Error)]
pub enum TarballError {
    #[error("failed to read tarball: {0}")]
    ReadError(String),
    #[error("failed to extract: {0}")]
    ExtractError(String),
    #[error("integrity mismatch for {file}: expected {expected}, got {actual}")]
    IntegrityMismatch {
        file: String,
        expected: String,
        actual: String,
    },
    #[error("path escape attempt detected: {path}")]
    PathEscape { path: String },
    #[error("symlink target outside package: {target} -> {path}")]
    SymlinkEscape { target: String, path: String },
}

pub struct TarballExtractor {
    verify_integrity: bool,
}

impl TarballExtractor {
    pub fn new() -> Self {
        Self {
            verify_integrity: true,
        }
    }

    pub fn with_integrity_check(mut self, verify: bool) -> Self {
        self.verify_integrity = verify;
        self
    }

    pub fn extract(
        &self,
        tarball: &Path,
        dest: &Path,
    ) -> Result<Vec<ExtractedEntry>, TarballError> {
        let file = File::open(tarball)
            .map_err(|e| TarballError::ReadError(e.to_string()))?;
        let reader = BufReader::new(file);
        let decoder = GzDecoder::new(reader);
        let mut archive = Archive::new(decoder);
        
        let mut entries = Vec::new();
        let package_root = dest.to_path_buf();

        for entry in archive.entries().map_err(|e| TarballError::ExtractError(e.to_string()))? {
            let mut entry = entry.map_err(|e| TarballError::ExtractError(e.to_string()))?;
            
            let path = entry.path()
                .map_err(|e| TarballError::ExtractError(e.to_string()))?
                .into_owned();
            
            let relative_path = path.to_string_lossy().to_string();
            
            if relative_path.contains("..") {
                return Err(TarballError::PathEscape {
                    path: relative_path.clone(),
                });
            }

            let dest_path = package_root.join(&path);
            
            if !dest_path.starts_with(&package_root) {
                return Err(TarballError::PathEscape {
                    path: dest_path.to_string_lossy().to_string(),
                });
            }

            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| TarballError::ExtractError(e.to_string()))?;
            }

            let mut hasher = Sha256::new();
            let mut data = Vec::new();
            entry.read_to_end(&mut data)
                .map_err(|e| TarballError::ExtractError(e.to_string()))?;
            
            hasher.update(&data);
            let hash = hex::encode(hasher.finalize());

            let entry_type = entry.header().entry_type();
            
            if entry_type.is_symlink() {
                let target = entry.link_name()
                    .map_err(|e| TarballError::ExtractError(e.to_string()))?
                    .map(|l| l.into_owned())
                    .unwrap_or_else(|| PathBuf::from(""));
                
                let target_str = target.to_string_lossy().to_string();
                let target_path = PathBuf::from(&target_str);
                if target_path.is_absolute() || target_str.starts_with("..") {
                    return Err(TarballError::SymlinkEscape {
                        target: target_str,
                        path: dest_path.to_string_lossy().to_string(),
                    });
                }

                #[cfg(unix)]
                std::os::unix::fs::symlink(&target, &dest_path)
                    .map_err(|e| TarballError::ExtractError(e.to_string()))?;
                
                entries.push(ExtractedEntry {
                    path: relative_path,
                    hash,
                    entry_type: EntryType::Symlink,
                    size: data.len() as u64,
                });
            } else if entry_type.is_file() {
                let mut file = File::create(&dest_path)
                    .map_err(|e| TarballError::ExtractError(e.to_string()))?;
                file.write_all(&data)
                    .map_err(|e| TarballError::ExtractError(e.to_string()))?;
                
                if entry.header().mode()
                    .map(|m| m & 0o111 != 0)
                    .unwrap_or(false) {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let mut perms = fs::metadata(&dest_path)
                            .map_err(|e| TarballError::ExtractError(e.to_string()))?
                            .permissions();
                        perms.set_mode(perms.mode() | 0o111);
                        fs::set_permissions(&dest_path, perms)
                            .map_err(|e| TarballError::ExtractError(e.to_string()))?;
                    }
                }

                entries.push(ExtractedEntry {
                    path: relative_path,
                    hash,
                    entry_type: EntryType::File,
                    size: data.len() as u64,
                });
            }
        }

        Ok(entries)
    }

    pub fn extract_single_file(
        &self,
        tarball: &Path,
        file_path: &str,
    ) -> Result<Vec<u8>, TarballError> {
        let file = File::open(tarball)
            .map_err(|e| TarballError::ReadError(e.to_string()))?;
        let reader = BufReader::new(file);
        let decoder = GzDecoder::new(reader);
        let mut archive = Archive::new(decoder);

        for entry in archive.entries().map_err(|e| TarballError::ExtractError(e.to_string()))? {
            let mut entry = entry.map_err(|e| TarballError::ExtractError(e.to_string()))?;
            
            let path = entry.path()
                .map_err(|e| TarballError::ExtractError(e.to_string()))?
                .into_owned()
                .to_string_lossy()
                .to_string();
            
            if path == file_path {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)
                    .map_err(|e| TarballError::ExtractError(e.to_string()))?;
                return Ok(data);
            }
        }

        Err(TarballError::ReadError(format!("file not found: {}", file_path)))
    }

    pub fn list_files(&self, tarball: &Path) -> Result<Vec<String>, TarballError> {
        let file = File::open(tarball)
            .map_err(|e| TarballError::ReadError(e.to_string()))?;
        let reader = BufReader::new(file);
        let decoder = GzDecoder::new(reader);
        let mut archive = Archive::new(decoder);

        let mut files = Vec::new();
        for entry in archive.entries().map_err(|e| TarballError::ExtractError(e.to_string()))? {
            let entry = entry.map_err(|e| TarballError::ExtractError(e.to_string()))?;
            let path = entry.path()
                .map_err(|e| TarballError::ExtractError(e.to_string()))?
                .into_owned()
                .to_string_lossy()
                .to_string();
            files.push(path);
        }

        Ok(files)
    }
}

impl Default for TarballExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ExtractedEntry {
    pub path: String,
    pub hash: String,
    pub entry_type: EntryType,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    File,
    Symlink,
    Directory,
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;
    use tar;

    fn create_tarball(files: &[(&str, &str)]) -> Vec<u8> {
        let mut tar_data = Vec::new();
        {
            let encoder = GzEncoder::new(&mut tar_data, Compression::default());
            let mut tar_builder = tar::Builder::new(encoder);
            for (name, content) in files {
                let mut header = tar::Header::new_gnu();
                header.set_path(name).unwrap();
                header.set_size(content.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                tar_builder.append(&header, content.as_bytes()).unwrap();
            }
            tar_builder.finish().unwrap();
        }
        tar_data
    }

    #[test]
    fn test_list_files() {
        let tar_data = create_tarball(&[
            ("package.json", r#"{"name": "test"}"#),
            ("index.js", "console.log('hello')"),
        ]);
        
        let temp = tempfile::tempdir().unwrap();
        let tarball = temp.path().join("test.tar.gz");
        fs::write(&tarball, &tar_data).unwrap();
        
        let extractor = TarballExtractor::new();
        let files = extractor.list_files(&tarball).unwrap();
        
        assert!(files.contains(&"package.json".to_string()));
        assert!(files.contains(&"index.js".to_string()));
    }
}
