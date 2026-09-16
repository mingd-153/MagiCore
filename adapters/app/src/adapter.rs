//! PackageAdapter implementation for app cores.
//! Giữ orchestration app riêng khỏi phần detect language và SBOM.
//!
//! Global Gate 1 (2026-09-16): the registry-resolver surface that always
//! failed (resolve/fetch/add/remove/update) is GONE — the fail-closed
//! defaults from `mgc_types::capabilities` answer now. App installs run
//! the provider toolchain (flutter pub get / gradle / swift package
//! resolve) through the adapter's real install pipeline.
//! Global Gate 1: mặt registry-resolver vốn luôn lỗi
//! (resolve/fetch/add/remove/update) đã BỊ XÓA — default fail-closed trả
//! lời thay. Install app chạy toolchain provider qua pipeline install
//! thật của adapter.

use crate::language::{AppLanguage, detect_language, manifest_is_app};
use async_trait::async_trait;
use mgc_types::adapter::{
    AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
};
use mgc_types::capabilities::{
    AuditProvider, Capability, ContentStoreProvider, CoreIdent, LifecycleRunner, LockfileProvider,
    ProjectDetector, ScaffoldProvider,
};
use mgc_types::{Ecosystem, Manifest, MgResult, PackageId, Version};
use std::path::{Path, PathBuf};

pub struct AppAdapter {
    pub language: AppLanguage,
}

impl AppAdapter {
    /// Capability manifest (Global Gate 1) — code-reality notes:
    /// - ProjectDetector: `detect_language`/`manifest_is_app` — real.
    /// - ScaffoldProvider: `mgc create-app` CLI lane — real.
    /// - LifecycleRunner: install orchestrates the provider toolchain
    ///   lifecycle (flutter pub get / gradle / spm) — real.
    /// - ContentStoreProvider: `crate::install::run_install` (adapter.rs
    ///   install) is REAL for all app languages — claimed (delegated to
    ///   the native toolchain caches, honest mgc orchestration).
    /// - LockfileProvider: per-language manifest writers
    ///   (crate::manifest::write_manifest) are real — claimed.
    /// - AuditProvider: per-language scanner dispatch — real.
    ///
    /// DependencyResolver/ArtifactFetcher are NOT claimed: resolution and
    /// downloads are owned by flutter/gradle/swift, and the previous
    /// resolve/fetch stubs returned empty no-ops.
    /// Bảng capability (Global Gate 1) — ghi chú theo code thật.
    pub const CAPABILITIES: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
        Capability::LifecycleRunner,
        Capability::ContentStoreProvider,
        Capability::LockfileProvider,
        Capability::AuditProvider,
    ];
}

pub fn adapter_for(root: &Path) -> Option<AppAdapter> {
    let language = detect_language(root)?;
    Some(AppAdapter { language })
}

impl CoreIdent for AppAdapter {
    fn core_id(&self) -> &'static str {
        "app"
    }

    fn name(&self) -> &str {
        "app"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::App
    }
}

impl ProjectDetector for AppAdapter {
    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_app(project_root)
    }
}

impl ScaffoldProvider for AppAdapter {
    /// Evidence: `mgc create-app` scaffold lane (CLI create commands).
    /// Dẫn chứng: lane scaffold `mgc create-app` (lệnh create của CLI).
    fn probe_scaffold(&self) -> MgResult<()> {
        Ok(())
    }
}

impl LifecycleRunner for AppAdapter {
    /// Evidence: install orchestrates the provider toolchain lifecycle
    /// (flutter pub get / gradle / swift package resolve).
    /// Dẫn chứng: install điều phối lifecycle toolchain provider
    /// (flutter pub get / gradle / swift package resolve).
    fn probe_lifecycle_runner(&self) -> MgResult<()> {
        Ok(())
    }
}

#[async_trait]
impl ContentStoreProvider for AppAdapter {
    /// Evidence: real install pipeline for all app languages
    /// (crate::install::run_install — flutter/gradle/swift orchestration).
    /// Dẫn chứng: pipeline install thật cho mọi ngôn ngữ app
    /// (crate::install::run_install — điều phối flutter/gradle/swift).
    fn probe_content_store(&self) -> MgResult<()> {
        Ok(())
    }

    async fn install(
        &self,
        graph: &mgc_types::ResolvedGraph,
        project_root: &Path,
        opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        // Use new install pipeline
        crate::install::run_install(self.language, graph, project_root, opts, None).await
    }
}

#[async_trait]
impl LockfileProvider for AppAdapter {
    /// Evidence: per-language manifest writers
    /// (crate::manifest::write_manifest — pubspec/gradle/swift).
    /// Dẫn chứng: bộ viết manifest theo ngôn ngữ
    /// (crate::manifest::write_manifest — pubspec/gradle/swift).
    fn probe_lockfile_provider(&self) -> MgResult<()> {
        Ok(())
    }

    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        crate::manifest::write_manifest(self.language, project_root, manifest)
    }
}

#[async_trait]
impl AuditProvider for AppAdapter {
    /// Evidence: real scanner dispatch per language
    /// (crate::audit::run_audit — Kotlin→OWASP; Flutter/Swift honest
    /// unavailable). Dẫn chứng: điều phối scanner thật theo ngôn ngữ
    /// (crate::audit::run_audit).
    fn probe_audit_provider(&self) -> MgResult<()> {
        Ok(())
    }

    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport> {
        // Real scanner dispatch per language — Kotlin→OWASP dependency-check;
        // Flutter/Swift honestly unavailable (no CVE scanner exists).
        // Điều phối scanner thật theo ngôn ngữ — Kotlin→dependency-check;
        // Flutter/Swift trung thực unavailable (không có scanner CVE).
        crate::audit::run_audit(self.language, project_root).await
    }
}

#[async_trait]
impl PackageAdapter for AppAdapter {
    fn capabilities(&self) -> &'static [Capability] {
        Self::CAPABILITIES
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        crate::manifest::parse_manifest(self.language, project_root)
    }

    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>> {
        let manifest = self.parse_manifest(project_root).await?;
        Ok(manifest
            .all_dependencies()
            .map(|dep| InstalledPackage {
                id: PackageId::new(
                    dep.name.clone(),
                    dep.range
                        .satisfying_version()
                        .unwrap_or_else(|| Version::new(0, 1, 0)),
                ),
                path: PathBuf::new(),
                integrity: None,
                is_direct: true,
                is_dev: dep.dev,
            })
            .collect())
    }
}

// Unclaimed capabilities — empty impls inherit the fail-closed
// Unsupported probes/defaults from mgc_types::capabilities.
// Capability chưa claim — impl rỗng kế thừa probe/default fail-closed
// từ mgc_types::capabilities.
impl mgc_types::capabilities::DependencyResolver for AppAdapter {}
impl mgc_types::capabilities::ArtifactFetcher for AppAdapter {}
impl mgc_types::capabilities::OptimizerProvider for AppAdapter {}
impl mgc_types::capabilities::Materializer for AppAdapter {}
impl mgc_types::capabilities::SimulatorProvider for AppAdapter {}
impl mgc_types::capabilities::DeviceProvider for AppAdapter {}
impl mgc_types::capabilities::DeployProvider for AppAdapter {}
impl mgc_types::capabilities::ModelRuntimeProvider for AppAdapter {}
