//! `install/fetch.rs` — Package fetching for Rust/Python libraries.
//! Tương tự web install/fetch.rs nhưng cho Rust (crates.io) và Python (PyPI).

use mgc_types::{MgError, MgResult, PackageId};
use std::path::{Path, PathBuf};

/// Fetch Rust crate from crates.io (via cargo).
/// Tải Rust crate từ crates.io (qua cargo).
///
/// Currently delegates to cargo fetch (exec passthrough).
/// Hiện tại delegate cho cargo fetch (exec passthrough).
/// TODO P2: native crates.io resolver to avoid cargo dependency.
pub fn fetch_rust_crate(_project_root: &Path, package_id: &PackageId) -> MgResult<PathBuf> {
    // Cargo fetch downloads to ~/.cargo/registry/cache/
    // cargo fetch tải về ~/.cargo/registry/cache/
    let cache_dir = dirs::home_dir()
        .ok_or_else(|| MgError::Other("cannot find home directory".to_string()))?
        .join(".cargo/registry/cache");

    // Crates.io uses index-based naming
    // Crates.io dùng naming theo index
    let crate_file = cache_dir.join("github.com-1ecc6299db9ec823").join(format!(
        "{}-{}.crate",
        package_id.name(),
        package_id.version()
    ));

    if crate_file.exists() {
        Ok(crate_file)
    } else {
        Err(MgError::Other(format!(
            "crate file not found after cargo fetch: {}",
            crate_file.display()
        )))
    }
}

/// Fetch Python wheel/sdist from PyPI.
/// Tải Python wheel/sdist từ PyPI.
///
/// Currently delegates to pip download (exec passthrough).
/// Hiện tại delegate cho pip download (exec passthrough).
/// TODO P2: native PyPI client (PEP 503 Simple Repository API).
pub fn fetch_python_package(project_root: &Path, package_id: &PackageId) -> MgResult<PathBuf> {
    // pip download stores in current directory by default
    // pip download lưu trong thư mục hiện tại mặc định
    let download_dir = project_root.join(".mgc-cache/python");
    std::fs::create_dir_all(&download_dir)
        .map_err(|e| MgError::Other(format!("failed to create cache dir: {}", e)))?;

    let args = vec![
        "download".to_string(),
        "--no-deps".to_string(),
        "--dest".to_string(),
        download_dir.display().to_string(),
        format!("{}=={}", package_id.name(), package_id.version()),
    ];

    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        ..Default::default()
    };

    let result = mgc_exec::run::run("pip", &args, &exec_opts)
        .map_err(|e| MgError::Other(format!("pip download failed: {}", e)))?;

    if result.exit_code != 0 {
        return Err(MgError::Other("pip download failed".to_string()));
    }

    // Find downloaded file (wheel or sdist)
    // Tìm file đã tải (wheel hoặc sdist)
    for entry in std::fs::read_dir(&download_dir)
        .map_err(|e| MgError::Other(format!("failed to read download dir: {}", e)))?
    {
        let entry = entry.map_err(|e| MgError::Other(format!("failed to read entry: {}", e)))?;
        let path = entry.path();
        let Some(file_name) = path.file_name() else {
            continue;
        };
        let filename = file_name.to_string_lossy();

        // Match wheel or sdist naming
        // Khớp naming wheel hoặc sdist
        if filename.starts_with(package_id.name().as_str())
            && filename.contains(&package_id.version().to_string())
        {
            return Ok(path);
        }
    }

    Err(MgError::Other(format!(
        "downloaded package file not found for {}",
        package_id
    )))
}

/// Construct URL for crate tarball download.
/// Xây dựng URL để tải crate tarball.
///
/// Format: https://crates.io/api/v1/crates/{name}/{version}/download
pub fn crate_tarball_url(package_id: &PackageId) -> String {
    format!(
        "https://crates.io/api/v1/crates/{}/{}/download",
        package_id.name(),
        package_id.version()
    )
}

/// Construct URL for PyPI package download.
/// Xây dựng URL để tải package PyPI.
///
/// Format: https://pypi.org/simple/{name}/ (HTML index with links)
pub fn pypi_package_index_url(package_name: &str) -> String {
    format!("https://pypi.org/simple/{}/", package_name)
}
