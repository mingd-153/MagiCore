//! `profile.rs` — Tracing & profiling utilities for WebAdapter.
//!
//! Provides timeline markers and performance metrics for install, resolve, pipeline, and materialization phases.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use mg_types::PackageId;

#[derive(Default)]
pub struct InstallProfile {
    pub enabled: bool,
    pub marks: Vec<(&'static str, u128)>,
}

impl InstallProfile {
    pub fn from_env() -> Self {
        let enabled = std::env::var("MEGAGATE_WEB_PROFILE_INSTALL")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        Self {
            enabled,
            marks: Vec::new(),
        }
    }

    pub fn mark(&mut self, label: &'static str, started_at: Instant) {
        if self.enabled {
            self.marks.push((label, started_at.elapsed().as_millis()));
        }
    }

    pub fn mark_step(&mut self, label: &'static str, step_started_at: Instant) {
        if self.enabled {
            self.marks
                .push((label, step_started_at.elapsed().as_millis()));
        }
    }

    pub fn flush(&self, total_ms: u64) {
        if !self.enabled {
            return;
        }

        eprintln!("[megagate:web:install-profile] total={}ms", total_ms);
        for (label, millis) in &self.marks {
            eprintln!("[megagate:web:install-profile] {}={}ms", label, millis);
        }
    }
}

#[derive(Default)]
pub struct ResolveProfile {
    pub enabled: bool,
    pub marks: Vec<(&'static str, u128)>,
}

impl ResolveProfile {
    pub fn from_env() -> Self {
        let enabled = std::env::var("MEGAGATE_WEB_PROFILE_INSTALL")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        Self {
            enabled,
            marks: Vec::new(),
        }
    }

    pub fn mark(&mut self, label: &'static str, started_at: Instant) {
        if self.enabled {
            self.marks.push((label, started_at.elapsed().as_millis()));
        }
    }

    pub fn flush(&self, total_ms: u64) {
        if !self.enabled {
            return;
        }

        eprintln!("[megagate:web:resolve-profile] total={}ms", total_ms);
        for (label, millis) in &self.marks {
            eprintln!("[megagate:web:resolve-profile] {}={}ms", label, millis);
        }
    }
}

#[derive(Default)]
pub struct PipelineProfile {
    pub enabled: bool,
    pub package_count: AtomicU64,
    pub tarball_bytes: AtomicU64,
    pub download_ms_total: AtomicU64,
    pub extract_ms_total: AtomicU64,
    pub download_ms_max: AtomicU64,
    pub extract_ms_max: AtomicU64,
    pub slowest_downloads: Mutex<Vec<(u64, String, u64)>>,
    pub slowest_extracts: Mutex<Vec<(u64, String)>>,
}

pub enum TarballPayload {
    Bytes(Arc<[u8]>),
    CachedPath(PathBuf, u64),
}

impl TarballPayload {
    pub fn len(&self) -> u64 {
        match self {
            Self::Bytes(bytes) => bytes.len() as u64,
            Self::CachedPath(_, len) => *len,
        }
    }
}

pub struct TarballFetchResult {
    pub payload: TarballPayload,
    pub queue_wait_ms: u64,
    pub io_ms: u64,
    pub persist_to_shared_cache: bool,
}

impl PipelineProfile {
    pub fn from_env() -> Self {
        let enabled = std::env::var("MEGAGATE_WEB_PROFILE_INSTALL")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        Self {
            enabled,
            ..Default::default()
        }
    }

    pub fn record_download(
        &self,
        package: &PackageId,
        bytes: u64,
        elapsed_ms: u64,
        queue_wait_ms: u64,
        io_ms: u64,
    ) {
        if !self.enabled {
            return;
        }
        self.package_count.fetch_add(1, Ordering::Relaxed);
        self.tarball_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.download_ms_total
            .fetch_add(elapsed_ms, Ordering::Relaxed);
        self.download_ms_max
            .fetch_max(elapsed_ms, Ordering::Relaxed);
        self.record_slowest_download(package, bytes, elapsed_ms, queue_wait_ms, io_ms);
    }

    pub fn record_extract(&self, package: &PackageId, elapsed_ms: u64) {
        if !self.enabled {
            return;
        }
        self.extract_ms_total
            .fetch_add(elapsed_ms, Ordering::Relaxed);
        self.extract_ms_max.fetch_max(elapsed_ms, Ordering::Relaxed);
        self.record_slowest_extract(package, elapsed_ms);
    }

    pub fn record_slowest_download(
        &self,
        package: &PackageId,
        bytes: u64,
        elapsed_ms: u64,
        queue_wait_ms: u64,
        io_ms: u64,
    ) {
        let mut guard = self.slowest_downloads.lock().unwrap();
        guard.push((
            elapsed_ms,
            format!("{} queue_wait={}ms io={}ms", package, queue_wait_ms, io_ms),
            bytes,
        ));
        guard.sort_by(|a, b| b.0.cmp(&a.0));
        guard.truncate(5);
    }

    pub fn record_slowest_extract(&self, package: &PackageId, elapsed_ms: u64) {
        let mut guard = self.slowest_extracts.lock().unwrap();
        guard.push((elapsed_ms, package.to_string()));
        guard.sort_by(|a, b| b.0.cmp(&a.0));
        guard.truncate(5);
    }

    pub fn flush(&self) {
        if !self.enabled {
            return;
        }
        eprintln!(
            "[megagate:web:pipeline-profile] packages={} bytes={} download_ms_total={} download_ms_max={} extract_ms_total={} extract_ms_max={}",
            self.package_count.load(Ordering::Relaxed),
            self.tarball_bytes.load(Ordering::Relaxed),
            self.download_ms_total.load(Ordering::Relaxed),
            self.download_ms_max.load(Ordering::Relaxed),
            self.extract_ms_total.load(Ordering::Relaxed),
            self.extract_ms_max.load(Ordering::Relaxed),
        );
        for (elapsed_ms, package, bytes) in self.slowest_downloads.lock().unwrap().iter() {
            eprintln!(
                "[megagate:web:pipeline-profile] slow_download package={} elapsed={}ms bytes={}",
                package, elapsed_ms, bytes
            );
        }
        for (elapsed_ms, package) in self.slowest_extracts.lock().unwrap().iter() {
            eprintln!(
                "[megagate:web:pipeline-profile] slow_extract package={} elapsed={}ms",
                package, elapsed_ms
            );
        }
    }
}

#[derive(Default)]
pub struct MaterializationProfile {
    pub enabled: bool,
    pub packages_linked: AtomicUsize,
    pub files_seen: AtomicUsize,
    pub directories_seen: AtomicUsize,
    pub hardlinks: AtomicUsize,
    pub copies: AtomicUsize,
    pub reflinks: AtomicUsize,
    pub symlinks: AtomicUsize,
}

impl MaterializationProfile {
    pub fn from_env() -> Self {
        let enabled = std::env::var("MEGAGATE_WEB_PROFILE_INSTALL")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        Self {
            enabled,
            ..Default::default()
        }
    }

    pub fn record_package_linked(&self) {
        if self.enabled {
            self.packages_linked.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_directory(&self) {
        if self.enabled {
            self.directories_seen.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_file(&self) {
        if self.enabled {
            self.files_seen.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_hardlink(&self) {
        if self.enabled {
            self.hardlinks.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_copy(&self) {
        if self.enabled {
            self.copies.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_symlink(&self) {
        if self.enabled {
            self.symlinks.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_reflink(&self) {
        if self.enabled {
            self.reflinks.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn flush(&self) {
        if !self.enabled {
            return;
        }
        eprintln!(
            "[megagate:web:materialize-profile] packages_linked={} dirs={} files={} hardlinks={} copies={} reflinks={} symlinks={}",
            self.packages_linked.load(Ordering::Relaxed),
            self.directories_seen.load(Ordering::Relaxed),
            self.files_seen.load(Ordering::Relaxed),
            self.hardlinks.load(Ordering::Relaxed),
            self.copies.load(Ordering::Relaxed),
            self.reflinks.load(Ordering::Relaxed),
            self.symlinks.load(Ordering::Relaxed),
        );
    }
}
