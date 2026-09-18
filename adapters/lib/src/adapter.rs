//! PackageAdapter implementation for library cores.
//! Điều phối TS/Rust/Python lib mà không nhồi mọi logic vào lib.rs.

use crate::language::{LibLanguage, detect_language, manifest_is_lib};
use crate::manifest::{
    parse_cargo_manifest, parse_csproj_manifest, parse_go_mod_manifest, parse_maven_manifest,
    parse_pyproject_manifest, write_cargo_manifest, write_pyproject_manifest,
};
use crate::native::engine::resolve_with_protocol;
use crate::tooling::{
    cargo_lock_versions, check_pip_allowed, dist_info_versions, exec_tool, go_module_path,
    pip_binary, placeholder_id, version_from_manifest,
};
use anyhow::Result;
use async_trait::async_trait;
use mgc_lockfile::EcosystemTag;
use mgc_resolver::protocols::{
    CratesProtocol, GoModProtocol, MavenProtocol, NuGetProtocol, PypiProtocol,
};
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    PreparedAdd, UpdatedPackage,
};
use mgc_types::capabilities::{
    ArtifactFetcher, AuditProvider, Capability, ContentStoreProvider, CoreIdent,
    DependencyResolver, LockfileProvider, ProjectDetector, ScaffoldProvider,
};
use mgc_types::{
    DependencySpec, Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph, Version,
    VersionRange,
};
use std::path::{Path, PathBuf};

/// Which build manifest owns a Java lib project — Maven pom.xml is natively
/// parseable/resolvable; Gradle build scripts are toolchain programs and can
/// only fail closed honestly.
/// Build manifest nào sở hữu project lib Java — pom.xml của Maven parse và
/// resolve native được; build script Gradle là chương trình toolchain, chỉ
/// có thể fail-closed trung thực.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum JavaManifestKind {
    Pom,
    Gradle,
    #[default]
    None,
}

pub struct LibAdapter {
    language: LibLanguage,
    /// Java-only: which build manifest the project carries (detected at
    /// construction, when the project root is known).
    /// (Chỉ cho Java: project mang build manifest nào (detect lúc dựng, khi
    /// đã biết project root).)
    java_kind: JavaManifestKind,
    /// .NET-only: the project's `<TargetFramework>` (first of
    /// `<TargetFrameworks>`), read at construction for multi-TFM group
    /// selection during native resolve. `None` = unknown → divergent
    /// multi-TFM sets fail closed.
    /// (Chỉ cho .NET: `<TargetFramework>` của project, đọc lúc dựng.)
    dotnet_tfm: Option<String>,
    web: Option<mgc_web_adapter::WebAdapter>,
    /// Lock v3 entries produced by the native resolve — flushed to
    /// mgc.lock during install.
    /// (Entry lock v3 sinh từ resolve native — ghi xuống mgc.lock lúc
    /// install.)
    pending_lock: std::sync::Mutex<Vec<mgc_lockfile::Package>>,
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
        root: &Path,
        registry_url: Option<String>,
        token: Option<String>,
    ) -> Result<Self> {
        Self::for_language_with_chain(language, root, registry_url, token, &[])
    }

    fn for_language_with_chain(
        language: LibLanguage,
        root: &Path,
        registry_url: Option<String>,
        token: Option<String>,
        fallbacks: &[(String, Option<String>)],
    ) -> Result<Self> {
        // Java build-manifest kind: pom.xml is natively owned, gradle
        // build scripts fail closed downstream.
        // (Loại build-manifest Java: pom.xml sở hữu native, build script
        // gradle fail-closed phía sau.)
        let java_kind = if root.join("pom.xml").is_file() {
            JavaManifestKind::Pom
        } else if root.join("build.gradle").is_file() || root.join("build.gradle.kts").is_file() {
            JavaManifestKind::Gradle
        } else {
            JavaManifestKind::None
        };
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
        Ok(Self {
            language,
            java_kind,
            dotnet_tfm: read_dotnet_target_framework(root),
            web,
            pending_lock: std::sync::Mutex::new(Vec::new()),
        })
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

/// Read the consumer target framework from the first `*.csproj` in
/// `root` (`<TargetFramework>`, or the FIRST of `<TargetFrameworks>` with
/// a loud warning — multi-target projects resolve against it). `None`
/// when absent/unreadable (divergent multi-TFM sets then fail closed).
/// Tag scan, no XML dependency (same technique as the nuspec parser).
/// (Đọc `<TargetFramework>` từ csproj đầu tiên.)
fn read_dotnet_target_framework(root: &Path) -> Option<String> {
    let mut csprojs: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "csproj"))
        .collect();
    csprojs.sort();
    let content = std::fs::read_to_string(csprojs.first()?).ok()?;
    let tag = |name: &str| -> Option<String> {
        let open = format!("<{name}>");
        let close = format!("</{name}>");
        let start = content.find(&open)? + open.len();
        let end = content[start..].find(&close)?;
        Some(content[start..start + end].trim().to_string())
    };
    if let Some(single) = tag("TargetFramework").filter(|v| !v.is_empty()) {
        return Some(single);
    }
    let multi = tag("TargetFrameworks").filter(|v| !v.is_empty())?;
    let mut frameworks = multi.split(';').map(str::trim).filter(|v| !v.is_empty());
    let first = frameworks.next()?.to_string();
    if frameworks.next().is_some() {
        eprintln!(
            "WARNING: multi-target project ({multi}) — native resolve selects dependency groups for '{first}' only"
        );
    }
    Some(first)
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

    fn manifest_owned(&self) -> bool {
        // go.mod is owned SOLELY by the go toolchain (mgc never rewrites
        // it — see write_manifest): booking an add in memory would fake a
        // mutation the tool never sees. All other lib manifests are
        // mgc-written.
        // (go.mod do toolchain go sở hữu DUY NHẤT — add phải chạy thật.)
        if self.web.is_some() {
            return true;
        }
        !matches!(self.language, LibLanguage::Go)
    }

    async fn prepare_add(
        &self,
        project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        opts: AddOptions,
    ) -> MgResult<PreparedAdd> {
        // Resolve-first (C0 FIX2): the pyproject/Cargo writers cannot
        // persist star ranges (every saved dep needs a bound), so booking
        // an unpinned dep in memory faked a mutation the disk never saw.
        // Resolve the real version natively FIRST; a resolve failure
        // errors honestly instead of fake-adding.
        // (Resolve-trước: writer không lưu được range `*`.)
        let unpinned = range.as_ref().map(|r| r.is_star()).unwrap_or(true);
        enum ResolveFirst {
            Python,
            Rust,
        }
        let resolve_first = if self.web.is_none() && unpinned {
            match self.language {
                LibLanguage::Python => Some(ResolveFirst::Python),
                LibLanguage::Rust => Some(ResolveFirst::Rust),
                _ => None,
            }
        } else {
            None
        };
        if let Some(kind) = resolve_first {
            let mut scratch = Manifest::new("scratch", Ecosystem::Lib);
            scratch.add_dep(
                DependencySpec::new(name.clone(), VersionRange::star()),
                opts.dev,
                opts.optional,
                opts.peer,
            );
            let (protocol, tag, registry): (
                Box<dyn mgc_resolver::protocols::RegistryProtocol>,
                EcosystemTag,
                &str,
            ) = match kind {
                ResolveFirst::Python => (
                    Box::new(PypiProtocol::from_env()),
                    EcosystemTag::Python,
                    "pypi://pypi.org",
                ),
                ResolveFirst::Rust => (
                    Box::new(CratesProtocol::from_env()),
                    EcosystemTag::Rust,
                    "crates://crates.io",
                ),
            };
            let resolution =
                resolve_with_protocol(protocol.as_ref(), tag, registry, &scratch).await?;
            let resolved = resolution
                .graph
                .packages
                .iter()
                .find(|p| p.id.name_str() == name.as_str())
                .ok_or_else(|| {
                    mgc_types::MgError::Other(format!(
                        "native resolve returned no entry for '{}' — refusing to book an unpinned dep",
                        name.as_str()
                    ))
                })?;
            // Python: `==` form (the pyproject writer trims it to a bare
            // version and saves `name>=version`, which round-trips).
            // Rust: bare version (the Cargo writer saves it verbatim —
            // `serde_json = "1.0.140"`, caret-implied like `cargo add`).
            // (Python dạng `==`, Rust version trần.)
            let pinned = match kind {
                ResolveFirst::Python => {
                    VersionRange::parse(&format!("=={}", resolved.id.version()))?
                }
                ResolveFirst::Rust => VersionRange::parse(&resolved.id.version().to_string())?,
            };
            *self.pending_lock.lock().expect("lib pending lock poisoned") =
                resolution.lock_packages;
            return Ok(PreparedAdd {
                id: PackageId::new(name.clone(), resolved.id.version().clone()),
                range: pinned,
            });
        }
        // Every other lane keeps the default dry-run placeholder path.
        // (Lane khác giữ path placeholder dry-run mặc định.)
        let exact = opts.exact;
        let mut dry_opts = opts;
        dry_opts.no_save = true;
        let id = self.add(project_root, name, range, dry_opts).await?;
        let saved_range = match range {
            Some(range) if exact => {
                let raw = range
                    .as_str()
                    .trim_start_matches('^')
                    .trim_start_matches('~');
                VersionRange::parse(raw)?
            }
            Some(range) => range.clone(),
            None => VersionRange::star(),
        };
        Ok(PreparedAdd {
            id,
            range: saved_range,
        })
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
            // Java (Phase 2): pom.xml projects are parsed by the SAME POM
            // parser the native Maven engine uses; gradle projects keep an
            // honest empty manifest (resolve fails closed downstream).
            // (Java (Phase 2): project pom.xml được parse bằng CÙNG parser
            // POM của engine Maven native; project gradle giữ manifest rỗng
            // trung thực (resolve fail-closed phía sau).)
            LibLanguage::Java => parse_maven_manifest(project_root),
            // .NET (Phase 2): the csproj's `<PackageReference>` entries feed
            // the native NuGet engine; packages.lock.json keeps owning audit
            // pin truth. mgc never rewrites the csproj (read-only parse).
            // (.NET (Phase 2): các `<PackageReference>` của csproj nạp cho
            // engine NuGet native; packages.lock.json vẫn giữ truth ghim cho
            // audit. mgc không bao giờ viết lại csproj (parse chỉ đọc).)
            LibLanguage::DotNet => parse_csproj_manifest(project_root),
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
                *self.pending_lock.lock().expect("lib pending lock poisoned") =
                    resolution.lock_packages;
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
                *self.pending_lock.lock().expect("lib pending lock poisoned") =
                    resolution.lock_packages;
                Ok(resolution.graph)
            }
            // Native Go module proxy engine (Phase 2): go.mod pins → @v/list
            // selection → .mod graph → ziphash/sumdb-verified install
            // (mgc-native, no `go mod download` spawn for resolve/fetch/
            // install).
            // Engine Go module proxy native (Phase 2): pin go.mod → chọn
            // @v/list → graph qua .mod → install xác minh ziphash/sumdb
            // (mgc-native, không spawn `go mod download` cho resolve/fetch/
            // install).
            LibLanguage::Go => {
                let protocol = GoModProtocol::from_env();
                let resolution = resolve_with_protocol(
                    &protocol,
                    EcosystemTag::Go,
                    "go://proxy.golang.org",
                    manifest,
                )
                .await?;
                *self.pending_lock.lock().expect("lib pending lock poisoned") =
                    resolution.lock_packages;
                Ok(resolution.graph)
            }
            // Java (Phase 2): pom.xml projects resolve natively through the
            // Maven engine (metadata → POM graph → verified jar); gradle
            // build scripts are toolchain programs — fail closed with
            // honest guidance (never an empty-graph false success).
            // (Java (Phase 2): project pom.xml resolve native qua engine
            // Maven (metadata → graph POM → jar đã xác minh); build script
            // gradle là chương trình toolchain — fail-closed kèm hướng dẫn
            // trung thực (không thành công giả graph rỗng).)
            LibLanguage::Java => match self.java_kind {
                JavaManifestKind::Pom => {
                    let protocol = MavenProtocol::from_env();
                    let resolution = resolve_with_protocol(
                        &protocol,
                        EcosystemTag::Maven,
                        "maven://repo.maven.apache.org",
                        manifest,
                    )
                    .await?;
                    *self.pending_lock.lock().expect("lib pending lock poisoned") =
                        resolution.lock_packages;
                    Ok(resolution.graph)
                }
                JavaManifestKind::Gradle | JavaManifestKind::None => {
                    Err(mgc_types::MgError::Unsupported {
                        core: "lib",
                        capability: "resolve",
                        guidance: "gradle build scripts are programs, not parseable manifests — \
                                   declare dependencies in a pom.xml for native resolution, or \
                                   let the gradle toolchain own resolution (mgc audits \
                                   gradle/verification-metadata.xml)"
                            .to_string(),
                    })
                }
            },
            // Native NuGet v3 engine (Phase 2): csproj PackageReferences →
            // flat-container versions → registration SHA-512-verified nupkgs
            // (mgc-native, no `dotnet restore` spawn for resolve/fetch/
            // install).
            // (Engine NuGet v3 native (Phase 2): PackageReference csproj →
            // version flat container → nupkg xác minh SHA-512 theo
            // registration (mgc-native, không spawn `dotnet restore` cho
            // resolve/fetch/install).)
            LibLanguage::DotNet => {
                let mut protocol = NuGetProtocol::from_env().await;
                if let Some(tfm) = self.dotnet_tfm.as_deref() {
                    protocol = protocol.with_consumer_tfm(tfm);
                }
                let resolution = resolve_with_protocol(
                    &protocol,
                    EcosystemTag::NuGet,
                    "nuget://api.nuget.org",
                    manifest,
                )
                .await?;
                *self.pending_lock.lock().expect("lib pending lock poisoned") =
                    resolution.lock_packages;
                Ok(resolution.graph)
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
                    pip_binary(),
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
                    pip_binary(),
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
                exec_tool(project_root, pip_binary(), &args)?;
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
        let mut lock_packages =
            std::mem::take(&mut *self.pending_lock.lock().expect("lib pending lock poisoned"));
        if lock_packages.is_empty() && !graph.packages.is_empty() {
            // Lock-short-circuit installs never resolved in this process,
            // so no markers were staged — read them from the project
            // lockfile (it satisfied the manifest, else the graph would
            // not come from it). Without this, hash-backed verifiers
            // (go sumdb, nuget sha512, maven sha1) see no integrity
            // source on every reinstall-from-lock.
            // (Install từ lock không resolve: đọc marker từ lockfile.)
            if let Ok(content) = std::fs::read_to_string(project_root.join("mgc.lock"))
                && let Ok(locked) = mgc_lockfile::parser::parse_lockfile(&content)
            {
                lock_packages = locked.packages;
            }
        }
        crate::install::run_install(
            self.language,
            self.web.as_ref(),
            graph,
            project_root,
            opts,
            None, // Issue #6: pass ContentStore when available
            lock_packages,
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
        root,
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
        root,
        registry_url,
        token,
        fallbacks,
    )?))
}
