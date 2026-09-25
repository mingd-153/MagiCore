//! PackageAdapter implementation for app cores.
//! Giữ orchestration app riêng khỏi phần detect language và SBOM.
//!
//! App dependency operations are implemented only for explicitly supported
//! native lanes; unsupported ecosystems fail closed without provider PMs.
//! Chỉ lane native đã hỗ trợ mới chạy; ecosystem khác bị từ chối, không gọi PM ngoài.

use crate::language::{AppLanguage, detect_language, manifest_is_app};
use async_trait::async_trait;
use mgc_lib_adapter::native::engine::resolve_with_protocol;
use mgc_lockfile::EcosystemTag;
use mgc_resolver::protocols::PubProtocol;
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    PreparedAdd,
};
use mgc_types::capabilities::{
    AuditProvider, Capability, ContentStoreProvider, CoreIdent, DependencyResolver,
    LifecycleRunner, LockfileProvider, ProjectDetector, ScaffoldProvider,
};
use mgc_types::{DependencySpec, MgError, PackageName, VersionRange};
use mgc_types::{Ecosystem, Manifest, MgResult, PackageId, ResolvedGraph};
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

    /// Capability manifest (Global Gate 1) — code-reality notes:
    /// per-language truth, never a core-level blanket (a blanket
    /// DependencyResolver claim overclaims for Kotlin/ObjC, which fail
    /// closed). `CAPABILITIES` (used by `mgc capabilities`) is the BASE
    /// set every language shares; the instance method below adds
    /// DependencyResolver exactly for Flutter/Swift/RN, and the
    /// capabilities test enforces both directions.
    pub const CAPABILITIES: &'static [Capability] = Self::CAPABILITIES_BASE;

    const CAPABILITIES_BASE: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
        Capability::AuditProvider,
    ];
    const CAPABILITIES_RESOLVER: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
        Capability::ContentStoreProvider,
        Capability::LockfileProvider,
        Capability::AuditProvider,
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
    fn probe_lifecycle_runner(&self) -> MgResult<()> {
        Err(mgc_types::capabilities::unsupported_capability(
            "app",
            "lifecycle_runner",
            "build/test/run are compiler and runtime operations, not AppAdapter package-lifecycle capabilities",
        ))
    }
}

#[async_trait]
impl ContentStoreProvider for AppAdapter {
    fn probe_content_store(&self) -> MgResult<()> {
        match self.language {
            AppLanguage::Flutter | AppLanguage::Swift | AppLanguage::ReactNative => Ok(()),
            AppLanguage::Kotlin | AppLanguage::ObjC | AppLanguage::Multi => {
                Err(mgc_types::capabilities::unsupported_capability(
                    "app",
                    "content_store",
                    "this ecosystem has no MagiCore-native install path",
                ))
            }
        }
    }

    async fn install(
        &self,
        graph: &mgc_types::ResolvedGraph,
        project_root: &Path,
        opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        if !matches!(
            self.language,
            AppLanguage::Flutter | AppLanguage::Swift | AppLanguage::ReactNative
        ) {
            return Err(mgc_types::capabilities::unsupported_capability(
                "app",
                "install",
                "Kotlin, Objective-C and multi-platform dependency installation are unsupported until MagiCore owns their complete native lifecycle",
            ));
        }
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
    fn probe_lockfile_provider(&self) -> MgResult<()> {
        match self.language {
            AppLanguage::Flutter | AppLanguage::Swift | AppLanguage::ReactNative => Ok(()),
            AppLanguage::Kotlin | AppLanguage::ObjC | AppLanguage::Multi => {
                Err(mgc_types::capabilities::unsupported_capability(
                    "app",
                    "lockfile_provider",
                    "this ecosystem has no MagiCore-owned lockfile writer",
                ))
            }
        }
    }

    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        if !matches!(
            self.language,
            AppLanguage::Flutter | AppLanguage::Swift | AppLanguage::ReactNative
        ) {
            return Err(mgc_types::capabilities::unsupported_capability(
                "app",
                "write_manifest",
                "this app ecosystem has no safe MagiCore-owned manifest writer",
            ));
        }
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
    fn manifest_identity(&self) -> Option<mgc_types::ManifestIdentity> {
        // Source-verified manifest per language (manifest/ module).
        // (Manifest theo language — đúng file module đọc/viết.)
        let (language, format, relpath) = match self.language {
            AppLanguage::Flutter => ("flutter", "pubspec.yaml", "pubspec.yaml"),
            AppLanguage::Swift => ("swift", "Package.swift", "Package.swift"),
            AppLanguage::Kotlin => ("kotlin", "build.gradle", "build.gradle"),
            AppLanguage::ReactNative => ("react-native", "package.json", "package.json"),
            AppLanguage::ObjC => ("objc", "Podfile", "Podfile"),
            AppLanguage::Multi => ("multi", "multi", "multi"),
        };
        Some(mgc_types::ManifestIdentity {
            core: "app".to_string(),
            language: language.to_string(),
            format: format.to_string(),
            relpath: relpath.to_string(),
        })
    }

    fn capabilities(&self) -> &'static [Capability] {
        // Per-language truth: only lanes with a native resolver claim it.
        match self.language {
            AppLanguage::Flutter | AppLanguage::Swift | AppLanguage::ReactNative => {
                Self::CAPABILITIES_RESOLVER
            }
            AppLanguage::Kotlin | AppLanguage::ObjC | AppLanguage::Multi => Self::CAPABILITIES_BASE,
        }
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
        if manifest.all_dependencies().next().is_none() {
            return Ok(Vec::new());
        }
        Err(mgc_types::MgError::Unsupported {
            core: "app",
            capability: "list",
            guidance: "the app adapter does not yet reconcile resolved locks with installed files; refusing to label declared/ranged dependencies as installed versions".to_string(),
        })
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
                let expanded = crate::manifest::flutter::expand_flutter_sdk_dependencies(
                    &self.project_root,
                    manifest,
                )?;
                let resolution = resolve_with_protocol(
                    &protocol,
                    EcosystemTag::Dart,
                    "pub://pub.dev",
                    &expanded,
                )
                .await?;
                *self.pending_lock.lock().expect("app pending lock poisoned") =
                    resolution.lock_packages;
                Ok(resolution.graph)
            }
            // Swift registry archives are handled natively; Git-source
            // dependencies fail closed until MGC owns Git transport.
            // (Archive Swift registry được xử lý native; dependency Git bị
            // từ chối tới khi MGC tự sở hữu transport.)
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
