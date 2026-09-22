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
use mgc_lib_adapter::native::engine::resolve_with_protocol;
use mgc_lockfile::EcosystemTag;
use mgc_resolver::protocols::PubProtocol;
use mgc_resolver::protocols::RegistryProtocol;
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    PreparedAdd,
};
use mgc_types::capabilities::{
    AuditProvider, Capability, ContentStoreProvider, CoreIdent, DependencyResolver,
    LifecycleRunner, LockfileProvider, ProjectDetector, ScaffoldProvider,
};
use mgc_types::{DependencySpec, MgError, PackageName, VersionRange};
use mgc_types::{Ecosystem, Manifest, MgResult, PackageId, ResolvedGraph, Version};
use std::path::{Path, PathBuf};

pub struct AppAdapter {
    pub language: AppLanguage,
    // Root captured at detection — the RN layered resolve needs the
    // project's tier files (gradle.lockfile / Podfile.lock) which the
    // `resolve(&manifest)` signature does not carry.
    // (Gốc bắt lúc detect — resolve phân tầng RN cần các file tier của
    // project (gradle.lockfile / Podfile.lock) mà chữ ký
    // `resolve(&manifest)` không mang theo.)
    pub project_root: PathBuf,
    // Native-resolve lock entries (Swift/RN lanes) carried from
    // `DependencyResolver::resolve` to `install` — the same pending-lock
    // pattern the lib adapter uses (same-instance CLI flow).
    // (Entry lock từ resolve native (lane Swift/RN) chuyển từ
    // `DependencyResolver::resolve` sang `install` — cùng pattern
    // pending-lock của lib adapter (flow CLI cùng instance).)
    pending_lock: std::sync::Mutex<Vec<mgc_lockfile::Package>>,
}

impl AppAdapter {
    /// Public constructor for direct-language use (tests, CLI lanes) —
    /// the pending lock starts empty and the project root is unknown
    /// (RN tier files resolve against an empty root → honest skips).
    /// (Constructor public cho dùng trực tiếp theo ngôn ngữ (test, lane
    /// CLI) — pending lock khởi tạo rỗng và gốc project không biết (file
    /// tier RN resolve theo gốc rỗng → skip trung thực).)
    pub fn new(language: AppLanguage) -> Self {
        Self {
            language,
            project_root: PathBuf::new(),
            pending_lock: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Native Swift update: resolve each target to latest through the
    /// SwiftPM registry, bump the `from:`/`exact:` requirement in
    /// Package.swift text, and verify by re-scan (no toolchain needed
    /// for the edit itself). Returns `(updated, skipped)` with honest
    /// skip reasons — git/branch/revision/range pins are not bumpable;
    /// an unconfigured registry fails the whole op closed (never a
    /// partial silent pass).
    /// (Update Swift native: resolve latest + sửa Package.swift +
    /// verify bằng quét lại.)
    /// Native Kotlin update through the version catalog
    /// (`gradle/libs.versions.toml`): resolve each target to latest
    /// through Maven Central, bump the catalog (ref values or inline
    /// literals), verify by re-parse. No install tail — install stays
    /// delegated-gradle (the user builds with the toolchain). Projects
    /// without a catalog fail closed with guidance (never a silent
    /// partial pass, never a guessed DSL edit).
    /// (Update Kotlin native qua version catalog.)
    pub async fn update_kotlin_native(
        &self,
        project_root: &Path,
        packages: &[String],
    ) -> MgResult<Vec<(String, String, String)>> {
        use crate::manifest::gradle::{bump_catalog_pin, parse_version_catalog};
        use mgc_resolver::protocols::MavenProtocol;
        let entries = parse_version_catalog(project_root).ok_or_else(|| {
            MgError::Other(
                "kotlin update needs gradle/libs.versions.toml — plain build.gradle scripts are programs, not safely editable manifests (declare versions in a catalog for native update, or run gradle with --compat-runtime)"
                    .to_string(),
            )
        })?;
        let targets: Vec<(String, String)> = if packages.is_empty() {
            entries
                .iter()
                .map(|e| (format!("{}:{}", e.group, e.artifact), e.version.clone()))
                .collect()
        } else {
            let mut out = Vec::new();
            for name in packages {
                // Accept `group:artifact` or bare artifact (unambiguous only).
                let hits: Vec<_> = entries
                    .iter()
                    .filter(|e| {
                        format!("{}:{}", e.group, e.artifact) == *name || e.artifact == *name
                    })
                    .collect();
                if hits.len() != 1 {
                    return Err(MgError::Other(format!(
                        "cannot update '{name}': {} catalog match(es) — use group:artifact",
                        hits.len()
                    )));
                }
                out.push((
                    format!("{}:{}", hits[0].group, hits[0].artifact),
                    hits[0].version.clone(),
                ));
            }
            out
        };
        let protocol = MavenProtocol::from_env();
        let mut updated = Vec::new();
        for (coord, from) in &targets {
            let entry = protocol.resolve(coord, "*").await.map_err(|e| {
                MgError::Other(format!("kotlin resolve-latest for '{coord}' failed: {e}"))
            })?;
            let latest = entry.version.clone();
            if *latest == *from {
                continue;
            }
            let (group, artifact) = coord.split_once(':').unwrap_or(("", ""));
            bump_catalog_pin(project_root, group, artifact, &latest)?;
            updated.push((coord.clone(), from.clone(), latest));
        }
        Ok(updated)
    }

    pub async fn update_swift_native(
        &self,
        project_root: &Path,
        packages: &[String],
    ) -> MgResult<(Vec<(String, String, String)>, Vec<(String, String)>)> {
        use mgc_resolver::protocols::bump_swift_requirement;
        let path = project_root.join("Package.swift");
        let text = std::fs::read_to_string(&path)
            .map_err(|e| MgError::Other(format!("read Package.swift: {e}")))?;
        // Targets: named packages must exist in the manifest; empty
        // means every registry-bumpable dep.
        let manifest = self.parse_manifest(project_root).await?;
        let targets: Vec<String> = if packages.is_empty() {
            manifest
                .all_dependencies()
                .map(|d| d.name.as_str().to_string())
                .collect()
        } else {
            for name in packages {
                if manifest.find_dep(name).is_none() {
                    return Err(MgError::Other(format!(
                        "cannot update '{name}': not in the project manifest — add it first"
                    )));
                }
            }
            packages.to_vec()
        };
        let protocol = mgc_resolver::protocols::SwiftRegistryProtocol::from_env();
        let mut current = text;
        let mut updated = Vec::new();
        let mut skipped = Vec::new();
        for name in &targets {
            // Latest through the registry (fail-closed without config).
            let entry = protocol.resolve(name, "*").await.map_err(|e| {
                MgError::Other(format!("swift resolve-latest for '{name}' failed: {e}"))
            })?;
            let latest = entry.version.clone();
            match bump_swift_requirement(&current, name, &latest) {
                Some(next) => {
                    // from-version for the report: previous pin in text.
                    let from = manifest
                        .find_dep(name)
                        .and_then(|d| d.range.satisfying_version().map(|v| v.to_string()))
                        .unwrap_or_else(|| "?".to_string());
                    current = next;
                    updated.push((name.clone(), from, latest));
                }
                None => skipped.push((
                    name.clone(),
                    "not a registry from:/exact: pin (branch/revision/range/git or ambiguous) — left untouched".to_string(),
                )),
            }
        }
        if !updated.is_empty() {
            std::fs::write(&path, &current)
                .map_err(|e| MgError::Other(format!("write Package.swift: {e}")))?;
        }
        Ok((updated, skipped))
    }

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
        // Phase 2 native engines — Flutter (pub.dev), Swift (SwiftPM
        // registry + git deps) and React Native (layered JS/Android/iOS)
        // resolve/fetch/install are mgc-native; the remaining app
        // languages stay toolchain-owned.
        // (Engine native Phase 2 — resolve/fetch/install của Flutter
        // (pub.dev), Swift (SwiftPM registry + dep git) và React Native
        // (phân tầng JS/Android/iOS) là mgc-native; các ngôn ngữ app còn
        // lại vẫn do toolchain giữ.)
        Capability::DependencyResolver,
    ];
}

pub fn adapter_for(root: &Path) -> Option<AppAdapter> {
    let language = detect_language(root)?;
    Some(AppAdapter {
        language,
        project_root: root.to_path_buf(),
        pending_lock: std::sync::Mutex::new(Vec::new()),
    })
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
        // Use new install pipeline (pending native-resolve lock entries are
        // consumed here — same-instance flow).
        // (Dùng install pipeline mới (entry lock từ resolve native được
        // tiêu thụ ở đây — flow cùng instance).)
        let lock_packages =
            std::mem::take(&mut *self.pending_lock.lock().expect("app pending lock poisoned"));
        crate::install::run_install(
            self.language,
            graph,
            project_root,
            opts,
            None,
            lock_packages,
        )
        .await
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

    fn supports_native_update(&self) -> bool {
        // Only Flutter owns the full native round-trip (resolve-first +
        // pubspec writer); other languages fail closed in prepare_add.
        matches!(self.language, AppLanguage::Flutter)
    }

    async fn prepare_add(
        &self,
        project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        opts: AddOptions,
    ) -> MgResult<PreparedAdd> {
        let AppLanguage::Flutter = self.language else {
            return Err(MgError::Other(format!(
                "native prepare-add is flutter-only; {:?} stays toolchain-owned",
                self.language
            )));
        };
        let _ = project_root;
        // Resolve-first (same contract as lib): the pubspec writer cannot
        // persist `*`, so resolve the real version natively FIRST for
        // every range; failures error honestly instead of fake-adding.
        let wanted = range.cloned().unwrap_or_else(VersionRange::star);
        let mut scratch = Manifest::new("scratch", Ecosystem::App);
        scratch.add_dep(
            DependencySpec::new(name.clone(), wanted.clone()),
            opts.dev,
            opts.optional,
            opts.peer,
        );
        let protocol = PubProtocol::from_env();
        let resolution =
            resolve_with_protocol(&protocol, EcosystemTag::Dart, "pub://pub.dev", &scratch).await?;
        let resolved = resolution
            .graph
            .packages
            .iter()
            .find(|p| p.id.name_str() == name.as_str())
            .ok_or_else(|| {
                MgError::Other(format!(
                    "native resolve returned no entry for '{}' — refusing to book an unresolved dep",
                    name.as_str()
                ))
            })?;
        let pinned = if wanted.is_star() {
            VersionRange::parse(&resolved.id.version().to_string())?
        } else {
            wanted
        };
        *self.pending_lock.lock().expect("app pending lock poisoned") = resolution.lock_packages;
        Ok(PreparedAdd {
            id: PackageId::new(name.clone(), resolved.id.version().clone()),
            range: pinned,
        })
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

#[async_trait]
impl DependencyResolver for AppAdapter {
    /// Evidence: Flutter resolves through the native pub.dev engine (Phase
    /// 2); Swift resolves through the native SwiftPM engine (Phase 2);
    /// other app languages stay toolchain-owned (gradle/pod) and fail
    /// closed.
    /// Dẫn chứng: Flutter resolve qua engine pub.dev native (Phase 2);
    /// Swift resolve qua engine SwiftPM native (Phase 2); ngôn ngữ app
    /// khác vẫn do toolchain giữ (gradle/pod) và fail-closed.
    fn probe_dependency_resolver(&self) -> MgResult<()> {
        // Executable per-language truth (not a blanket claim): native
        // engines resolve Flutter/Swift/RN; Kotlin/ObjC/Multi fail closed
        // here exactly as resolve() does below.
        // (Probe theo language, khớp resolve() bên dưới.)
        match self.language {
            AppLanguage::Flutter | AppLanguage::Swift | AppLanguage::ReactNative => Ok(()),
            AppLanguage::Kotlin | AppLanguage::ObjC | AppLanguage::Multi => {
                Err(mgc_types::MgError::Unsupported {
                    core: "app",
                    capability: "resolve",
                    guidance: format!(
                        "{} has no native resolve engine; dependency resolution is owned by its toolchain",
                        self.language.as_str()
                    ),
                })
            }
        }
    }

    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph> {
        match self.language {
            AppLanguage::Flutter => {
                let protocol = PubProtocol::from_env();
                let resolution =
                    resolve_with_protocol(&protocol, EcosystemTag::Dart, "pub://pub.dev", manifest)
                        .await?;
                *self.pending_lock.lock().expect("app pending lock poisoned") =
                    resolution.lock_packages;
                Ok(resolution.graph)
            }
            // Native SwiftPM engine (Phase 2): registry-scope/name packages
            // from the SwiftPM registry + git-only deps through allowlisted
            // `git clone --depth 1 --branch {tag}` with commit-SHA
            // provenance; lock entries ride the pending lock into install
            // (Package.resolved export + checkout SHA verification).
            // (Engine SwiftPM native (Phase 2): package registry
            // scope/name + dep chỉ-git qua `git clone --depth 1 --branch
            // {tag}` đã allowlist với provenance SHA commit; entry lock
            // đi qua pending lock vào install (export Package.resolved +
            // xác minh SHA checkout).)
            AppLanguage::Swift => {
                let protocol = mgc_resolver::protocols::SwiftRegistryProtocol::from_env();
                let registry = swift_registry_tag(&protocol);
                let resolution =
                    resolve_with_protocol(&protocol, EcosystemTag::Swift, &registry, manifest)
                        .await?;
                *self.pending_lock.lock().expect("app pending lock poisoned") =
                    resolution.lock_packages;
                Ok(resolution.graph)
            }
            // RN layered engine (Phase 2): the JS tier delegates to the web
            // adapter's npm pipeline; gradle.lockfile pins resolve through
            // the native Maven engine (ecosystem=maven) and Podfile.lock
            // pods are verified against the CocoaPods CDN sha1 checksums
            // (ecosystem=cocoapods) — per-tier entries in mgc.lock.
            // (Engine phân tầng RN (Phase 2): tier JS ủy quyền cho pipeline
            // npm của adapter web; pin gradle.lockfile resolve qua engine
            // Maven native (ecosystem=maven) và pod Podfile.lock được xác
            // minh theo checksum sha1 của CDN CocoaPods
            // (ecosystem=cocoapods) — entry riêng theo tier trong mgc.lock.)
            AppLanguage::ReactNative => {
                let resolution =
                    crate::native::rn_layers::resolve_rn_layers(manifest, &self.project_root)
                        .await?;
                *self.pending_lock.lock().expect("app pending lock poisoned") =
                    resolution.lock_packages;
                Ok(resolution.graph)
            }
            // Kotlin/ObjC/Multi: no native engine yet — toolchain-owned,
            // fail closed (never an empty-graph false success).
            // (Kotlin/ObjC/Multi: chưa có engine native — toolchain sở
            // hữu, fail-closed (không thành công giả graph rỗng).)
            AppLanguage::Kotlin | AppLanguage::ObjC | AppLanguage::Multi => {
                Err(mgc_types::MgError::Unsupported {
                    core: "app",
                    capability: "resolve",
                    guidance: format!(
                        "{} dependency resolution is owned by its toolchain; mgc-native \
                         resolution lands with the native engine (Phase 2/3)",
                        self.language.as_str()
                    ),
                })
            }
        }
    }
}

/// Registry tag for lock provenance (`swift://{host}` from the configured
/// base; unconfigured → the honest `swift://unconfigured` marker).
/// Tag registry cho provenance lock (`swift://{host}` từ base đã cấu hình;
/// chưa cấu hình → marker trung thực `swift://unconfigured`).
fn swift_registry_tag(protocol: &mgc_resolver::protocols::SwiftRegistryProtocol) -> String {
    let host = protocol
        .registry_host()
        .unwrap_or_else(|| "unconfigured".to_string());
    format!("swift://{host}")
}

// Unclaimed capabilities — empty impls inherit the fail-closed
// Unsupported probes/defaults from mgc_types::capabilities.
// Capability chưa claim — impl rỗng kế thừa probe/default fail-closed
// từ mgc_types::capabilities.
impl mgc_types::capabilities::ArtifactFetcher for AppAdapter {}
impl mgc_types::capabilities::OptimizerProvider for AppAdapter {}
impl mgc_types::capabilities::Materializer for AppAdapter {}
impl mgc_types::capabilities::SimulatorProvider for AppAdapter {}
impl mgc_types::capabilities::DeviceProvider for AppAdapter {}
impl mgc_types::capabilities::DeployProvider for AppAdapter {}
impl mgc_types::capabilities::ModelRuntimeProvider for AppAdapter {}
