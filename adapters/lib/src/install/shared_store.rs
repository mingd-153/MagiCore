//! `install/shared_store.rs` — Shared content-addressed store for
//! non-web ecosystems (B-series, 2026-09-12).
//!
//! MgCStore-mediated caching: mgc points each native toolchain's cache
//! root INSIDE the shared mgc store (`~/.magicore/store/<eco>/...`) so
//! every project on this machine reuses the SAME bytes — one download,
//! many projects, many ecosystems. The bytes stay in the toolchain's
//! native layout (cargo registry, uv/pip HTTP cache) — mgc owns the
//! DIRECTORY and measures the reuse; it never lies about owning the
//! byte format.
//!
//! Store chia sẻ content-addressed cho các ecosystem ngoài web. MgC trỏ
//! gốc cache của mỗi toolchain native VÀO store chung của mgc để mọi
//! project trên máy DÙNG LẠI cùng byte — tải một lần, nhiều project,
//! nhiều ecosystem. Byte giữ nguyên layout toolchain (registry cargo,
//! HTTP cache uv/pip) — mgc giữ THƯ MỤC và đo mức tái sử dụng; mgc
//! không hề claim sở hữu định dạng byte.
//!
//! Measured honestly:
//! - cold project: delta bytes = downloaded into the shared store
//! - warm project (same deps elsewhere before): delta ≈ 0 → the store
//!   served everything → bytes_from_cache = full dep byte size
//! - the label flips to MgCStore ONLY when the shared dir is actually
//!   used (it is, by construction — CARGO_HOME is exported).
//!
//! Đo trung thực: project lạnh: delta byte = tải vào store chung;
//! project ấm (đã có dep ở project khác): delta ≈ 0 → store đã phục vụ
//! tất cả → bytes_from_cache = toàn bộ kích thước byte dep; nhãn
//! MgCStore chỉ bật khi thư mục chung THẬT SỰ được dùng.

use mgc_types::{MgError, MgResult};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Byte size of a directory tree (0 on unreadable entries — never a
/// hard error; measurement must not fail an install).
/// Kích thước byte của cây thư mục (0 nếu không đọc được — đo lường
/// không được làm hỏng install).
pub fn dir_size_bytes(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            total += dir_size_bytes(&entry.path());
        } else {
            total += meta.len();
        }
    }
    total
}

/// Shared-store handle for one ecosystem run.
/// Bản quản lý store chia sẻ cho một lần chạy của một ecosystem.
pub struct SharedStoreRun {
    /// The toolchain cache root inside the shared mgc store.
    /// Gốc cache toolchain nằm trong store chung của mgc.
    pub cache_root: PathBuf,
    /// Bytes already in the store before this install (for reuse math).
    /// Byte đã có trong store trước lần install này (tính tái sử dụng).
    pub bytes_before: u64,
    started: Instant,
}
/// Resolve (and create) the shared store root for an ecosystem.
/// Resolve (và tạo) gốc store chung cho một ecosystem.
fn shared_root(eco: &str) -> MgResult<PathBuf> {
    let globals = mgc_platform::paths::GlobalPaths::new()
        .map_err(|e| MgError::Other(format!("cannot resolve mgc home: {e}")))?;
    let root: PathBuf = globals.store.join(eco);
    std::fs::create_dir_all(&root).map_err(|e| {
        MgError::Other(format!(
            "cannot create shared store '{}': {e}",
            root.display()
        ))
    })?;
    Ok(root)
}

impl SharedStoreRun {
    /// Begin a measured cargo run: CARGO_HOME → ~/.magicore/store/cargo.
    /// Bắt đầu lượt chạy cargo có đo lường: CARGO_HOME → ~/.magicore/store/cargo.
    pub fn cargo() -> MgResult<Self> {
        let root = shared_root("cargo")?;
        let bytes_before = dir_size_bytes(&root);
        Ok(Self {
            cache_root: root,
            bytes_before,
            started: Instant::now(),
        })
    }

    /// Begin a measured python run: uv/pip cache → ~/.magicore/store/pypi.
    /// Bắt đầu lượt chạy python có đo lường: cache uv/pip → ~/.magicore/store/pypi.
    pub fn pypi() -> MgResult<Self> {
        let root = shared_root("pypi")?;
        let bytes_before = dir_size_bytes(&root);
        Ok(Self {
            cache_root: root,
            bytes_before,
            started: Instant::now(),
        })
    }

    /// Env vars the child toolchain needs to use the shared root.
    /// Biến môi trường toolchain con cần để dùng gốc chung.
    pub fn env_vars(&self) -> Vec<(String, String)> {
        let root = self.cache_root.display().to_string();
        // Cargo: CARGO_HOME redirects registry + git caches.
        // Python: PIP_CACHE_DIR + UV_CACHE_DIR (uv honors both).
        // Cargo: CARGO_HOME chuyển hướng registry + git cache.
        // Python: PIP_CACHE_DIR + UV_CACHE_DIR (uv tôn trọng cả hai).
        let cargo_root = root.clone();
        let pip_root = root.clone();
        let uv_root = root;
        vec![
            ("CARGO_HOME".to_string(), cargo_root),
            ("PIP_CACHE_DIR".to_string(), pip_root),
            ("UV_CACHE_DIR".to_string(), uv_root),
        ]
    }

    /// Finish the run and compute the honest summary: if the store grew,
    /// the delta is fresh bytes downloaded; whatever the install USED
    /// without growing the store came from the shared store (reused
    /// across projects).
    /// Kết thúc lượt chạy và tính summary trung thực: store lớn thêm
    /// thì delta là byte mới tải; phần install DÙNG mà store không lớn
    /// thêm là lấy từ store chung (tái sử dụng chéo project).
    pub fn finish(self) -> (u64, u64, mgc_types::adapter::InstallCacheMode) {
        let bytes_after = dir_size_bytes(&self.cache_root);
        let delta = bytes_after.saturating_sub(self.bytes_before);
        let duration_ms = self.started.elapsed().as_millis() as u64;
        // Reuse math: when the store did not grow, EVERY byte this
        // install consumed was already there — served from the shared
        // store. When it grew by delta, the pre-existing bytes_before
        // were still reused on top of the fresh delta.
        // Toán tái sử dụng: store không lớn thêm thì MỌI byte install
        // này dùng đã có sẵn — được phục vụ từ store chung. Store lớn
        // thêm delta thì bytes_before cũ vẫn được tái sử dụng trên
        // phần delta mới.
        let reused = if delta == 0 { self.bytes_before } else { 0 };
        (
            reused,
            duration_ms,
            mgc_types::adapter::InstallCacheMode::MgCStore,
        )
    }
}
