//! PackageAdapter implementation for library cores.
//! Điều phối TS/Rust/Python lib mà không nhồi mọi logic vào lib.rs.

use crate::language::{LibLanguage, detect_language, manifest_is_lib};
use crate::manifest::{
    parse_cargo_manifest, parse_go_mod_manifest, parse_pyproject_manifest, write_cargo_manifest,
    write_pyproject_manifest,
};
use crate::native::engine::resolve_with_protocol;
use crate::tooling::{
    cargo_lock_versions, check_pip_allowed, dist_info_versions, exec_tool, go_module_path,
    placeholder_id, version_from_manifest,
};
use anyhow::Result;
use async_trait::async_trait;
use mgc_lockfile::EcosystemTag;
use mgc_resolver::protocols::{CratesProtocol, PypiProtocol};
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
};
use mgc_types::capabilities::{
    ArtifactFetcher, AuditProvider, Capability, ContentStoreProvider, CoreIdent,
    DependencyResolver, LockfileProvider, ProjectDetector, ScaffoldProvider,
};
use mgc_types::{
    Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph, Version, VersionRange,
};
use std::path::{Path, PathBuf};

pub struct LibAdapter {
    language: LibLanguage,
    web: Option<mgc_web_adapter::WebAdapter>,
}

impl LibAdapter {
    /// Capability manifest (Global Gate 1) — code-reality notes:
    /// - ProjectDetector: `detect_language`/`manifest_is_lib` — real.
    /// - ScaffoldProvider: `mgc create-lib <lang>` CLI lane — real.
    /// - DependencyResolver: add/remove/update are REAL toolchain
    ///   delegations (cargo add / pip install / go get; TS → web
    ///   delegate); resolve delegates to the embedded web engine for TS
    ///   and FAILS CLOSED (unsupported) for toolchain-owned languages —
    ///   the old empty-graph Ok was a false success (P0-B).
    /// - LockfileProvider: real manifest writers (cargo/pyproject/web).
    /// - ArtifactFetcher: delegated fetch — TS rides the web engine; the
    ///   toolchains fetch during their install. Non-TS fetch FAILS
    ///   CLOSED (P0-B): a bare Ok(()) faked "fetched" without work.
    /// - ContentStoreProvider: crate::install::run_install with the
    ///   shared store (install/shared_store.rs) — real.
    /// - AuditProvider: per-language scanner dispatch — real.
    ///
    /// Bảng capability (Global Gate 1) — ghi chú theo code thật.
    pub const CAPABILITIES: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
        Capability::DependencyResolver,
        Capability::LockfileProvider,
        Capability::ArtifactFetcher,
        Capability::ContentStoreProvider,
        Capability::AuditProvider,
    ];

    // P0-4 (2026-09-15): fallible construction — the TS lane builds a
    // WebAdapter whose registry-URL guard is a typed error now, so these
    // builders propagate Result instead of aborting.
    // (P0-4: dựng adapter có thể lỗi — lane TS dựng WebAdapter với guard
    // URL typed, builder propagate Result thay vì abort.)
    fn for_language(
        language: LibLanguage,
        registry_url: Option<String>,
        token: Option<String>,
    ) -> Result<Self> {
        Self::for_language_with_chain(language, registry_url, token, &[])
    }

    fn for_language_with_chain(
        language: LibLanguage,
        registry_url: Option<String>,
        token: Option<String>,
        fallbacks: &[(String, Option<String>)],
    ) -> Result<Self> {
        let web = if language == LibLanguage::Ts {
            Some(match (registry_url, token) {
                (Some(url), token) => mgc_web_adapter::WebAdapter::with_registry_chain(
                    url,
                    token,
                    fallbacks.to_vec(),
                )?,
                (None, _) => mgc_web_adapter::WebAdapter::new()?,
            })
        } else {
            None
        };
        Ok(Self { language, web })
    }

    pub fn language(&self) -> &'static str {
        match self.language {
            LibLanguage::Ts => "ts",
            LibLanguage::Rust => "rust",
            LibLanguage::Python => "python",
            LibLanguage::Go => "go",
            LibLanguage::Java => "java",
            LibLanguage::DotNet => "dotnet",
        }
    }
}

impl CoreIdent for LibAdapter {
    fn core_id(&self) -> &'static str {
        "lib"
    }

    fn name(&self) -> &str {
        "lib"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Lib
    }
}

impl ProjectDetector for LibAdapter {
    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_lib(project_root)
    }
}

impl ScaffoldProvider for LibAdapter {
    /// Evidence: `mgc create-lib <language>` scaffold lane (CLI create
    /// commands). Dẫn chứng: lane scaffold `mgc create-lib <language>`.
    fn probe_scaffold(&self) -> MgResult<()> {
        Ok(())
    }
}

#[async_trait]
impl PackageAdapter for LibAdapter {
    fn capabilities(&self) -> &'static [Capability] {
        Self::CAPABILITIES
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        if let Some(web) = &self.web {
            return web.parse_manifest(project_root).await;
        }
        match self.language {
            LibLanguage::Rust => parse_cargo_manifest(project_root),
            LibLanguage::Python => parse_pyproject_manifest(project_root),
            // Go has no mgc-written manifest — `go mod` owns go.mod
            // (parse via the go list wrapper when needed).
            // Go không có manifest do mgc viết — `go mod` sở hữu go.mod.
            LibLanguage::Go => parse_go_mod_manifest(project_root),
            // Java/.NET (P2 audit parity): the gradle verification
            // metadata / packages.lock.json own the pin truth — audit
            // reads them; lifecycle manifest parsing is not wired yet
            // (honest empty until the lifecycle lane lands).
            // Java/.NET: metadata verification gradle / packages.lock
            // giữ truth ghim — audit đọc chúng; parse manifest
            // lifecycle chưa nối (rỗng trung thực).
            LibLanguage::Java | LibLanguage::DotNet => {
                Ok(Manifest::new("java-dotnet-lib", Ecosystem::Lib))
            }
            LibLanguage::Ts => unreachable!("ts handled by web delegate"),
        }
    }

    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>> {
        if let Some(web) = &self.web {
            return web.list(project_root).await;
        }
        let manifest = self.parse_manifest(project_root).await?;
        let installed: std::collections::HashMap<String, String> = match self.language {
            LibLanguage::Rust => cargo_lock_versions(project_root).into_iter().collect(),
            LibLanguage::Python => dist_info_versions(project_root).into_iter().collect(),
            // go.mod already holds pinned versions — manifest versions ARE
            // the installed set (no separate lock for Go).
            // go.mod giữ version đã ghim — version trong manifest chính là
            // tập đã cài (Go không có lock tách riêng).
            LibLanguage::Go => manifest
                .all_dependencies()
                .map(|dep| {
                    (
                        dep.name.as_str().to_string(),
                        dep.range
                            .satisfying_version()
                            .map(|v| v.to_string())
                            .unwrap_or_default(),
                    )
                })
                .collect(),
            // Java/.NET installed-set truth lives in the lockfiles —
            // read the pins straight from the scanner's readers.
            // Tập đã cài Java/.NET nằm trong lockfile — đọc ghim thẳng
            // từ reader của scanner.
            LibLanguage::Java => {
                let raw = std::fs::read_to_string(
                    project_root
                        .join("gradle")
                        .join("verification-metadata.xml"),
                )
                .unwrap_or_default();
                mgc_audit::scanners::read_gradle_verification_metadata(&raw)
                    .0
                    .into_iter()
                    .map(|pin| (pin.name, pin.version))
                    .collect()
            }
            LibLanguage::DotNet => {
                let raw = std::fs::read_to_string(project_root.join("packages.lock.json"))
                    .unwrap_or_default();
                let pins = mgc_audit::scanners::read_packages_lock(&raw)
                    .map(|(pins, _)| pins)
                    .unwrap_or_default();
                pins.into_iter()
                    .map(|pin| (pin.name, pin.version))
                    .collect()
            }
            LibLanguage::Ts => unreachable!("ts handled by web delegate"),
        };
        Ok(manifest
            .all_dependencies()
            .map(|dep| {
                let version = installed
                    .get(dep.name.as_str())
                    .and_then(|v| Version::parse(v).ok())
                    .or_else(|| dep.range.satisfying_version());
                InstalledPackage {
                    id: PackageId::new(
                        dep.name.clone(),
                        version.unwrap_or_else(|| Version::new(0, 1, 0)),
                    ),
                    path: PathBuf::new(),
                    integrity: None,
                    is_direct: true,
                    is_dev: dep.dev,
                }
            })
            .collect())
    }

    fn set_dedupe_pref(&self, enabled: bool) {
        if let Some(web) = &self.web {
            web.set_dedupe_pref(enabled);
        }
    }

    fn set_existing_versions(&self, versions: std::collections::HashMap<String, String>) {
        if let Some(web) = &self.web {
            web.set_existing_versions(versions);
        }
    }
}

#[async_trait]
impl LockfileProvider for LibAdapter {
    /// Evidence: real manifest writers — cargo/pyproject (crate::manifest)
    /// and the web delegate for TS. Dẫn chứng: bộ viết manifest thật —
    /// cargo/pyproject (crate::manifest) và web delegate cho TS.
    fn probe_lockfile_provider(&self) -> MgResult<()> {
        Ok(())
    }

    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        if let Some(web) = &self.web {
            return web.write_manifest(project_root, manifest).await;
        }
        match self.language {
            LibLanguage::Rust => write_cargo_manifest(project_root, manifest),
            LibLanguage::Python => write_pyproject_manifest(project_root, manifest),
            // Never rewrite go.mod — the go toolchain is the sole owner.
            // Không bao giờ viết lại go.mod — go toolchain là chủ duy nhất.
            LibLanguage::Go => Ok(()),
            // Gradle/NuGet own their lockfiles — never rewritten by mgc.
            // Gradle/NuGet sở hữu lockfile của chúng — mgc không viết lại.
            LibLanguage::Java | LibLanguage::DotNet => Ok(()),
            LibLanguage::Ts => unreachable!("ts handled by web delegate"),
        }
    }
}

#[async_trait]
impl DependencyResolver for LibAdapter {
    /// Evidence: add/remove/update really delegate (cargo/pip/go; TS →
    /// web delegate); resolve rides the web engine for TS and FAILS
    /// CLOSED for toolchain-owned languages (P0-B — no empty-graph
    /// false success).
    /// Dẫn chứng: add/remove/update ủy quyền thật (cargo/pip/go; TS qua
    /// web delegate); resolve đi trên web engine cho TS và FAIL-CLOSED
    /// cho ngôn ngữ do toolchain sở hữu (P0-B — cấm thành công giả
    /// graph rỗng).
    fn probe_dependency_resolver(&self) -> MgResult<()> {
        Ok(())
    }

    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph> {
        if let Some(web) = &self.web {
            return web.resolve(manifest).await;
        }
        match self.language {
            // Native crates.io engine (Phase 2): fetch sparse index → select
            // → recurse → build graph + v3 lock entries (mgc-native, no
            // cargo spawn for resolve/fetch/install).
            // Engine crates.io native (Phase 2): fetch sparse index → chọn
            // → đệ quy → dựng graph + entry lock v3 (mgc-native, không spawn
            // cargo cho resolve/fetch/install).
            LibLanguage::Rust => {
                let protocol = CratesProtocol::from_env();
                let resolution = resolve_with_protocol(
                    &protocol,
                    EcosystemTag::Rust,
                    "crates://sparse+https://index.crates.io",
                    manifest,
                )
                .await?;
                Ok(resolution.graph)
            }
            // Native PyPI engine (Phase 2): JSON API → PEP 440 select →
            // wheel/sdist → recurse via requires_dist (mgc-native).
            // Engine PyPI native (Phase 2): JSON API → chọn PEP 440 →
            // wheel/sdist → đệ quy qua requires_dist (mgc-native).
            LibLanguage::Python => {
                let protocol = PypiProtocol::from_env();
                let resolution = resolve_with_protocol(
                    &protocol,
                    EcosystemTag::Python,
                    "pypi://pypi.org",
                    manifest,
                )
                .await?;
                Ok(resolution.graph)
            }
            // Go/Java/.NET: no native engine yet — toolchain-owned, fail
            // closed (never an empty-graph false success).
            // Go/Java/.NET: chưa có engine native — toolchain sở hữu,
            // fail-closed (không bao giờ thành công giả graph rỗng).
            LibLanguage::Go | LibLanguage::Java | LibLanguage::DotNet => {
                Err(mgc_types::MgError::Unsupported {
                    core: "lib",
                    capability: "resolve",
                    guidance: format!(
                        "{} dependency resolution is owned by its toolchain; mgc-native \
                         resolution lands with the native engine (Phase 2/3)",
                        self.language()
                    ),
                })
            }
            LibLanguage::Ts => unreachable!("ts handled by web delegate"),
        }
    }

    async fn add(
        &self,
        project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        opts: AddOptions,
    ) -> MgResult<PackageId> {
        // DELEGATED: add runs the native toolchain for real (cargo add /
        // pip install / go get) — mgc orchestrates only, it does not own
        // this dependency lifecycle.
        // (DELEGATED: add chạy toolchain gốc thật (cargo add / pip
        // install / go get) — mgc chỉ điều phối, không sở hữu lifecycle
        // dependency này.)
        if let Some(web) = &self.web {
            return web.add(project_root, name, range, opts).await;
        }
        if opts.no_save {
            return Ok(placeholder_id(name, range));
        }
        match self.language {
            LibLanguage::Rust => {
                let mut args = vec!["add".to_string()];
                if let Some(r) = range.filter(|r| !r.is_star()) {
                    args.push(format!("{}@{}", name.as_str(), r.as_str()));
                } else {
                    args.push(name.as_str().to_string());
                }
                exec_tool(project_root, "cargo", &args)?;
                exec_tool(project_root, "cargo", &["fetch".to_string()])?;
                Ok(version_from_manifest(project_root, name, LibLanguage::Rust)
                    .map(|v| PackageId::new(name.clone(), v))
                    .unwrap_or_else(|| placeholder_id(name, range)))
            }
            LibLanguage::Python => {
                check_pip_allowed(project_root, name.as_str())?;
                exec_tool(
                    project_root,
                    "pip",
                    &["install".to_string(), name.as_str().to_string()],
                )?;
                Ok(
                    version_from_manifest(project_root, name, LibLanguage::Python)
                        .map(|v| PackageId::new(name.clone(), v))
                        .unwrap_or_else(|| placeholder_id(name, range)),
                )
            }
            // Go: `go get module@version` — the go toolchain rewrites
            // go.mod; mgc only delegates (never edits go.mod itself).
            // Go: `go get module@version` — go toolchain viết lại go.mod;
            // mgc chỉ ủy quyền (không tự sửa go.mod).
            LibLanguage::Go => {
                let target = match range.filter(|r| !r.is_star()) {
                    Some(r) => format!(
                        "{}@v{}",
                        go_module_path(project_root, name),
                        r.satisfying_version()
                            .unwrap_or_else(|| Version::new(0, 0, 0))
                    ),
                    None => go_module_path(project_root, name),
                };
                exec_tool(project_root, "go", &["get".to_string(), target])?;
                Ok(version_from_manifest(project_root, name, LibLanguage::Go)
                    .map(|v| PackageId::new(name.clone(), v))
                    .unwrap_or_else(|| placeholder_id(name, range)))
            }
            // Java/.NET lifecycle add is not wired (P2 audit parity
            // scope) — the honest manual step, never a silent no-op.
            // Add lifecycle Java/.NET chưa nối (scope parity audit P2)
            // — bước thủ công trung thực, không no-op âm thầm.
            LibLanguage::Java | LibLanguage::DotNet => {
                return Err(mgc_types::MgError::Other(
                    "java/.NET dependency add runs through gradle/dotnet directly (mgc audit reads the lockfile; lifecycle add lands with the java/.NET install lanes)"
                        .to_string(),
                ));
            }
            LibLanguage::Ts => unreachable!("ts handled by web delegate"),
        }
    }

    async fn remove(&self, project_root: &Path, name: &PackageName) -> MgResult<()> {
        // DELEGATED: remove runs the native toolchain for real (cargo
        // remove / pip uninstall) — mgc orchestrates only, it does not
        // own this dependency lifecycle.
        // (DELEGATED: remove chạy toolchain gốc thật (cargo remove / pip
        // uninstall) — mgc chỉ điều phối, không sở hữu lifecycle
        // dependency này.)
        if let Some(web) = &self.web {
            return web.remove(project_root, name).await;
        }
        match self.language {
            LibLanguage::Rust => {
                exec_tool(
                    project_root,
                    "cargo",
                    &["remove".to_string(), name.as_str().to_string()],
                )?;
            }
            LibLanguage::Python => {
                check_pip_allowed(project_root, name.as_str())?;
                exec_tool(
                    project_root,
                    "pip",
                    &[
                        "uninstall".to_string(),
                        "-y".to_string(),
                        name.as_str().to_string(),
                    ],
                )?;
            }
            // Go: drop from go.mod via `go mod tidy` after removing the
            // import — mgc cannot know the full module path from the
            // display name, so surface the honest manual step.
            // Go: rút khỏi go.mod bằng `go mod tidy` sau khi bỏ import —
            // mgc không biết path module đầy đủ từ tên hiển thị, nên nêu
            // bước thủ công trung thực.
            LibLanguage::Go => {
                return Err(mgc_types::MgError::Other(
                    "go module removal requires the full module path — remove the import then run `go mod tidy`".to_string(),
                ));
            }
            // Java/.NET lifecycle remove is not wired — honest manual
            // step (gradle/dotnet own dependency edits).
            // Remove lifecycle Java/.NET chưa nối — bước thủ công trung
            // thực (gradle/dotnet sở hữu việc sửa dependency).
            LibLanguage::Java | LibLanguage::DotNet => {
                return Err(mgc_types::MgError::Other(
                    "java/.NET dependency removal runs through gradle/dotnet directly (mgc audit reads the lockfile; lifecycle remove lands with the java/.NET install lanes)"
                        .to_string(),
                ));
            }
            LibLanguage::Ts => unreachable!("ts handled by web delegate"),
        }
        Ok(())
    }

    async fn update(
        &self,
        project_root: &Path,
        name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>> {
        // DELEGATED: update runs the native toolchain for real (cargo
        // update / pip install --upgrade / go get -u) — mgc orchestrates
        // only, it does not own this dependency lifecycle.
        // (DELEGATED: update chạy toolchain gốc thật (cargo update / pip
        // install --upgrade / go get -u) — mgc chỉ điều phối, không sở
        // hữu lifecycle dependency này.)
        if let Some(web) = &self.web {
            return web.update(project_root, name).await;
        }
        match self.language {
            LibLanguage::Rust => {
                let mut args = vec!["update".to_string()];
                if let Some(n) = name {
                    args.push(n.as_str().to_string());
                }
                exec_tool(project_root, "cargo", &args)?;
            }
            LibLanguage::Python => {
                if let Some(n) = name {
                    check_pip_allowed(project_root, n.as_str())?;
                } else {
                    return Err(mgc_types::MgError::Other(
                        "pip update-all is not allowed — name a package (Q9 allowlist)".to_string(),
                    ));
                }
                let mut args = vec!["install".to_string(), "--upgrade".to_string()];
                if let Some(n) = name {
                    args.push(n.as_str().to_string());
                }
                exec_tool(project_root, "pip", &args)?;
            }
            // Go: `go get -u` upgrades the named module (update-all is
            // refused — same honest constraint as pip).
            // Go: `go get -u` nâng module được nêu (update-all bị từ
            // chối — ràng buộc trung thực như pip).
            LibLanguage::Go => {
                let Some(n) = name else {
                    return Err(mgc_types::MgError::Other(
                        "go update-all is not allowed — name a module (go toolchain policy)"
                            .to_string(),
                    ));
                };
                let target = go_module_path(project_root, n);
                exec_tool(
                    project_root,
                    "go",
                    &["get".to_string(), "-u".to_string(), target],
                )?;
            }
            // Java/.NET lifecycle update is not wired — honest manual
            // step, same constraint as go/pip update-all.
            // Update lifecycle Java/.NET chưa nối — bước thủ công trung
            // thực, ràng buộc như update-all go/pip.
            LibLanguage::Java | LibLanguage::DotNet => {
                return Err(mgc_types::MgError::Other(
                    "java/.NET dependency updates run through gradle/dotnet directly (mgc audit reads the lockfile; lifecycle update lands with the java/.NET install lanes)"
                        .to_string(),
                ));
            }
            LibLanguage::Ts => unreachable!("ts handled by web delegate"),
        }
        Ok(vec![])
    }
}

#[async_trait]
impl ArtifactFetcher for LibAdapter {
    /// Evidence: delegated fetch — TS rides the web engine (mgc fetcher +
    /// CAS); non-TS lanes FAIL CLOSED (P0-B): a bare Ok(()) claimed
    /// "artifacts fetched" while nothing was downloaded.
    /// Dẫn chứng: fetch ủy quyền — TS đi trên web engine (mgc fetcher +
    /// CAS); lane non-TS FAIL-CLOSED (P0-B): Ok(()) trần tuyên bố "đã
    /// fetch" trong khi không tải gì cả.
    fn probe_artifact_fetcher(&self) -> MgResult<()> {
        Ok(())
    }

    async fn fetch(&self, graph: &ResolvedGraph) -> MgResult<()> {
        if let Some(web) = &self.web {
            return web.fetch(graph).await;
        }
        // P0-B (2026-09-16) fail-closed: delegated fetch must never
        // answer a fake Ok. The toolchain fetches during its own
        // install (managed store); mgc-native fetch lands with the
        // native engine (Phase 2/3).
        // P0-B (2026-09-16) fail-closed: fetch ủy quyền không bao giờ
        // được trả Ok giả. Toolchain tự fetch trong install của nó
        // (store có quản lý); fetch mgc-native thuộc native engine
        // (Phase 2/3).
        Err(mgc_types::MgError::Unsupported {
            core: "lib",
            capability: "fetch",
            guidance: format!(
                "{} artifacts are fetched by the toolchain itself during its \
                 install; mgc-native fetch lands with the native engine \
                 (Phase 2/3)",
                self.language()
            ),
        })
    }
}

#[async_trait]
impl ContentStoreProvider for LibAdapter {
    /// Evidence: crate::install::run_install with the shared store
    /// (install/shared_store.rs — mgc-managed caches per toolchain).
    /// Dẫn chứng: crate::install::run_install với shared store
    /// (install/shared_store.rs — cache do mgc quản theo toolchain).
    fn probe_content_store(&self) -> MgResult<()> {
        Ok(())
    }

    async fn install(
        &self,
        graph: &ResolvedGraph,
        project_root: &Path,
        opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        // Use new install pipeline (install/mod.rs)
        // Dùng install pipeline mới (install/mod.rs)
        crate::install::run_install(
            self.language,
            self.web.as_ref(),
            graph,
            project_root,
            opts,
            None, // Issue #6: pass ContentStore when available
        )
        .await
    }
}

#[async_trait]
impl AuditProvider for LibAdapter {
    /// Evidence: per-language scanner dispatch below — Rust→cargo-audit,
    /// Python→pip-audit (fail-closed parsers), TS → web aggregate.
    /// Dẫn chứng: điều phối scanner theo ngôn ngữ bên dưới —
    /// Rust→cargo-audit, Python→pip-audit (parser fail-closed), TS qua
    /// aggregate web.
    fn probe_audit_provider(&self) -> MgResult<()> {
        Ok(())
    }

    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport> {
        if let Some(web) = &self.web {
            return web.audit(project_root).await;
        }
        // Real scanner dispatch per language — Rust→cargo-audit,
        // Python→pip-audit (fail-closed parsers, honest unavailable states).
        // Điều phối scanner thật theo ngôn ngữ — Rust→cargo-audit,
        // Python→pip-audit (parser fail-closed, unavailable trung thực).
        crate::audit::run_audit(self.language, project_root).await
    }
}

// Unclaimed capabilities — empty impls inherit the fail-closed
// Unsupported probes/defaults from mgc_types::capabilities.
// Capability chưa claim — impl rỗng kế thừa probe/default fail-closed
// từ mgc_types::capabilities.
impl mgc_types::capabilities::LifecycleRunner for LibAdapter {}
impl mgc_types::capabilities::OptimizerProvider for LibAdapter {}
impl mgc_types::capabilities::Materializer for LibAdapter {}
impl mgc_types::capabilities::SimulatorProvider for LibAdapter {}
impl mgc_types::capabilities::DeviceProvider for LibAdapter {}
impl mgc_types::capabilities::DeployProvider for LibAdapter {}
impl mgc_types::capabilities::ModelRuntimeProvider for LibAdapter {}

// P0-4 (2026-09-15): Result<Option<_>> — Ok(None) means "not a lib
// project" (an absence, not an error); Err carries the typed
// registry-URL failure from the TS/web lane.
// (P0-4: Result<Option<_>> — Ok(None) nghĩa là "không phải project lib"
// (vắng mặt, không phải lỗi); Err mang lỗi registry-URL typed từ lane TS/web.)
pub fn adapter_for(
    root: &Path,
    registry_url: Option<String>,
    token: Option<String>,
) -> Result<Option<LibAdapter>> {
    let Some(language) = detect_language(root) else {
        return Ok(None);
    };
    Ok(Some(LibAdapter::for_language(
        language,
        registry_url,
        token,
    )?))
}

pub fn adapter_for_with_chain(
    root: &Path,
    registry_url: Option<String>,
    token: Option<String>,
    fallbacks: &[(String, Option<String>)],
) -> Result<Option<LibAdapter>> {
    let Some(language) = detect_language(root) else {
        return Ok(None);
    };
    Ok(Some(LibAdapter::for_language_with_chain(
        language,
        registry_url,
        token,
        fallbacks,
    )?))
}
