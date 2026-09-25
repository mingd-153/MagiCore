//! `install/fetch.rs` — Package URL builders for Rust/Python libraries.
//! Build URL package cho Rust (crates.io) và Python (PyPI).
//!
//! Phase 2 (2026-09-16): actual fetching is NATIVE now — the protocol
//! engines (mgc-resolver) download + verify + CAS-import the artifact. The
//! legacy `cargo fetch` / `pip download` exec passthroughs were removed
//! (no PM toolchain spawn for resolve/fetch/install). These URL builders
//! remain for compatibility/tests.
//! Phase 2: fetch thật giờ là NATIVE — engine protocol (mgc-resolver) tải +
//! verify + import CAS artifact. Passthrough `cargo fetch` / `pip download`
//! cũ đã bị xóa (không spawn toolchain PM cho resolve/fetch/install). Các
//! builder URL còn lại chỉ để tương thích/test.

use mgc_types::PackageId;

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
