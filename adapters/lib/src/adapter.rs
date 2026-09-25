//! PackageAdapter implementation for library cores.
//! Dispatches each supported language to its native resolver/materializer lane.
//! Điều phối mỗi ngôn ngữ được hỗ trợ tới lane resolver/materializer native.

use crate::language::{LibLanguage, detect_language, manifest_is_lib};
use crate::manifest::{
    parse_cargo_manifest, parse_csproj_manifest, parse_go_mod_manifest, parse_maven_manifest,
    parse_pyproject_manifest, write_cargo_manifest, write_csproj_manifest, write_go_mod_manifest,
    write_pom_manifest, write_pyproject_manifest,
};
use crate::native::engine::resolve_with_protocol;
use anyhow::Result;
use async_trait::async_trait;
use mgc_lockfile::EcosystemTag;
use mgc_resolver::protocols::{
    CratesProtocol, GoModProtocol, MavenProtocol, NuGetProtocol, PypiProtocol,
};
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    PreparedAdd,
};
use mgc_types::capabilities::{
    ArtifactFetcher, AuditProvider, Capability, ContentStoreProvider, CoreIdent,
    DependencyResolver, LockfileProvider, ProjectDetector, ScaffoldProvider,
};
use mgc_types::{
    DependencySpec, Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph,
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
    /// Project root is retained so Python ownership can be revalidated at
    /// each operation; foreign lockfiles may appear after construction.
    /// (Giữ project root để kiểm tra lại quyền sở hữu Python mỗi thao tác.)
    project_root: PathBuf,
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

/// Native resolve-first lane for one lib language — the protocol engine
/// that owns version selection (no toolchain spawn). `None` is impossible
/// here (TS rides the embedded web delegate, not this enum).
/// (Lane resolve-first native theo ngôn ngữ lib.)
enum ResolveFirst {
    Python,
    Rust,
    Go,
    DotNet,
    JavaPom,
}

impl LibAdapter {
    /// Capability manifest (Global Gate 1) — code-reality notes:
    /// - ProjectDetector: `detect_language`/`manifest_is_lib` — real.
    /// - ScaffoldProvider: `mgc create-lib <lang>` CLI lane — real.
    /// - DependencyResolver: native CLI lanes resolve through the protocol
    ///   engine; all manifest mutations commit through the journaled CLI
    ///   gateway. Direct adapter add/remove/update methods fail closed.
    ///   TS rides the embedded web engine.
    /// - LockfileProvider: real manifest writers (cargo/pyproject/web).
    /// - ArtifactFetcher: standalone `fetch(graph)` is supported only by
    ///   TypeScript's web engine. Other native lanes fetch inside install
    ///   and do not claim the independent capability.
    /// - ContentStoreProvider: crate::install::run_install — native
    ///   download → verify → CAS import → materialize the toolchain layout
    ///   (cargo/pypi/go/maven/nuget), shared store for TS — real.
    /// - AuditProvider: per-language scanner dispatch — real.
    ///
    /// Bảng capability (Global Gate 1) — ghi chú theo code thật.
    /// Base: native resolve/manifest/store operations; fetch remains an
    /// install-internal step outside the standalone ArtifactFetcher trait.
    pub const CAPABILITIES: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
        Capability::DependencyResolver,
        Capability::LockfileProvider,
        Capability::ContentStoreProvider,
        Capability::AuditProvider,
    ];

    /// Java Gradle is a build program, not an MGC-owned dependency
    /// manifest. It may be detected and audited, but registry mutation
    /// capabilities stay unclaimed. (Gradle là chương trình build, không
    /// phải manifest MGC sở hữu.)
    pub const CAPABILITIES_JAVA_DELEGATED: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
        Capability::AuditProvider,
    ];

    /// Python projects owned by another manager retain detection/scanning,
    /// but do not claim native dependency mutation or installation.
    /// (Project Python do manager khác sở hữu chỉ claim detect/audit.)
    pub const CAPABILITIES_PYTHON_UNOWNED: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
        Capability::AuditProvider,
    ];

    /// TS lane đi trên web engine nhúng — materializer (node_modules
    /// layout) + lifecycle (scripts) THẬT qua engine đó, nên instance
    /// TS claim thêm 2 capability này; ngôn ngữ toolchain-owned
    /// (rust/python/go/...) KHÔNG claim (materialize/toolchain thuộc
    /// toolchain của chúng — trung thực).
    /// (The TS lane rides the embedded web engine — materializer +
    /// lifecycle are real THROUGH that engine, so a TS instance claims
    /// them; toolchain-owned languages do not.)
    pub const CAPABILITIES_TS: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
        Capability::DependencyResolver,
        Capability::LockfileProvider,
        Capability::ArtifactFetcher,
        Capability::ContentStoreProvider,
        Capability::Materializer,
        Capability::LifecycleRunner,
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
            project_root: root.to_path_buf(),
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

fn lock_ecosystem(language: LibLanguage) -> Option<EcosystemTag> {
    match language {
        LibLanguage::Rust => Some(EcosystemTag::Rust),
        LibLanguage::Python => Some(EcosystemTag::Python),
        LibLanguage::Go => Some(EcosystemTag::Go),
        LibLanguage::Java => Some(EcosystemTag::Maven),
        LibLanguage::DotNet => Some(EcosystemTag::NuGet),
        LibLanguage::Ts => None,
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

impl LibAdapter {
    fn python_native_owned(&self) -> bool {
        self.language != LibLanguage::Python
            || crate::manifest::supports_native_python_project(&self.project_root)
    }

    fn require_python_native(&self, capability: &'static str) -> MgResult<()> {
        self.require_python_native_at(&self.project_root, capability)
    }

    fn require_python_native_at(
        &self,
        project_root: &Path,
        capability: &'static str,
    ) -> MgResult<()> {
        if self.language == LibLanguage::Python
            && !crate::manifest::supports_native_python_project(project_root)
        {
            return Err(mgc_types::capabilities::unsupported_capability(
                "lib",
                capability,
                "this Python project uses dependency sources or lockfiles not owned by the MGC native PEP 621 lane; migrate explicitly or use a separately selected compatibility manager",
            ));
        }
        Ok(())
    }

    /// Which native resolve-first lane owns this project (`Ok(None)` = the
    /// embedded web delegate owns resolution, i.e. TS). Java-gradle fails
    /// closed: build scripts are programs, not parseable manifests.
    /// (Lane native resolve-first; TS do web delegate; gradle fail-closed.)
    fn resolve_first_lane(&self) -> MgResult<Option<ResolveFirst>> {
        self.require_python_native("resolve")?;
        if self.web.is_some() {
            return Ok(None);
        }
        Ok(match self.language {
            LibLanguage::Python => Some(ResolveFirst::Python),
            LibLanguage::Rust => Some(ResolveFirst::Rust),
            LibLanguage::Go => Some(ResolveFirst::Go),
            LibLanguage::DotNet => Some(ResolveFirst::DotNet),
            LibLanguage::Java => match self.java_kind {
                JavaManifestKind::Pom => Some(ResolveFirst::JavaPom),
                // pom.xml is natively owned; gradle scripts are programs —
                // fail closed instead of booking a mutation the disk never
                // sees (write_manifest is a no-op there by design).
                JavaManifestKind::Gradle | JavaManifestKind::None => {
                    return Err(mgc_types::MgError::Other(
                        "java add needs a pom.xml — gradle build scripts are programs, not parseable manifests (declare dependencies in a pom.xml for native add)"
                            .to_string(),
                    ));
                }
            },
            LibLanguage::Ts => None,
        })
    }

    /// Prepare ONE dependency natively — the protocol engine selects the
    /// real version and stages lock entries for the install tail. No file is
    /// written here; the CLI mutation gateway commits it under journal/lock.
    /// (Resolve native một dependency — không bao giờ spawn toolchain.)
    async fn resolve_first_add(
        &self,
        name: &PackageName,
        range: Option<&VersionRange>,
        opts: &AddOptions,
    ) -> MgResult<Option<PreparedAdd>> {
        let Some(kind) = self.resolve_first_lane()? else {
            return Ok(None);
        };
        // Scratch carries the REQUESTED range (star when the user gave
        // none) — the engine selects within it; the saved range stays
        // pinned only when the request was unpinned.
        let wanted: VersionRange = range.cloned().unwrap_or_else(VersionRange::star);
        let mut scratch = Manifest::new("scratch", Ecosystem::Lib);
        scratch.add_dep(
            DependencySpec::new(name.clone(), wanted.clone()),
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
            ResolveFirst::Go => (
                Box::new(GoModProtocol::from_env()),
                EcosystemTag::Go,
                "go://proxy.golang.org",
            ),
            ResolveFirst::DotNet => {
                // Async constructor (service-index probe) — build
                // before boxing.
                let mut proto = NuGetProtocol::from_env().await;
                if let Some(tfm) = self.dotnet_tfm.as_deref() {
                    proto = proto.with_consumer_tfm(tfm);
                }
                (
                    Box::new(proto),
                    EcosystemTag::NuGet,
                    "nuget://api.nuget.org",
                )
            }
            ResolveFirst::JavaPom => (
                Box::new(MavenProtocol::from_env()),
                EcosystemTag::Maven,
                "maven://repo.maven.apache.org",
            ),
        };
        let resolution = resolve_with_protocol(protocol.as_ref(), tag, registry, &scratch).await?;
        let resolved = resolution
            .graph
            .packages
            .iter()
            .find(|p| p.id.name_str() == name.as_str())
            .ok_or_else(|| {
                mgc_types::MgError::Other(format!(
                    "native resolve returned no entry for '{}' — refusing to book an unresolved dep",
                    name.as_str()
                ))
            })?;
        // Python: `==` form (the pyproject writer trims it to a bare
        // version and saves `name>=version`, which round-trips).
        // Rust: bare version (the Cargo writer saves it verbatim —
        // `serde_json = "1.0.140"`, caret-implied like `cargo add`).
        // Go: bare version (the go.mod writer v-prefixes it —
        // `require module v1.6.0`, like `go get`).
        // (Python dạng `==`, Rust/Go version trần.)
        let unpinned = wanted.is_star();
        let pinned = match kind {
            ResolveFirst::Python if unpinned => {
                VersionRange::parse(&format!("=={}", resolved.id.version()))?
            }
            ResolveFirst::Rust
            | ResolveFirst::Go
            | ResolveFirst::DotNet
            | ResolveFirst::JavaPom
                if unpinned =>
            {
                VersionRange::parse(&resolved.id.version().to_string())?
            }
            _ => wanted,
        };
        *self.pending_lock.lock().expect("lib pending lock poisoned") = resolution.lock_packages;
        Ok(Some(PreparedAdd {
            id: PackageId::new(name.clone(), resolved.id.version().clone()),
            range: pinned,
        }))
    }
}

#[async_trait]
impl PackageAdapter for LibAdapter {
    fn capabilities(&self) -> &'static [Capability] {
        match self.language {
            LibLanguage::Ts => Self::CAPABILITIES_TS,
            LibLanguage::Python if !self.python_native_owned() => Self::CAPABILITIES_PYTHON_UNOWNED,
            LibLanguage::Java if self.java_kind != JavaManifestKind::Pom => {
                Self::CAPABILITIES_JAVA_DELEGATED
            }
            _ => Self::CAPABILITIES,
        }
    }

    fn manifest_identity(&self) -> Option<mgc_types::ManifestIdentity> {
        let (language, format, relpath) = match self.language {
            LibLanguage::Ts => ("ts", "package.json", "package.json"),
            LibLanguage::Rust => ("rust", "Cargo.toml", "Cargo.toml"),
            LibLanguage::Python => ("python", "pyproject.toml", "pyproject.toml"),
            LibLanguage::Go => ("go", "go.mod", "go.mod"),
            LibLanguage::Java => match self.java_kind {
                JavaManifestKind::Pom => ("java", "pom.xml", "pom.xml"),
                JavaManifestKind::Gradle => ("java", "build.gradle", "build.gradle"),
                JavaManifestKind::None => ("java", "pom.xml", "pom.xml"),
            },
            LibLanguage::DotNet => ("dotnet", "csproj", "*.csproj"),
        };
        Some(mgc_types::ManifestIdentity {
            core: "lib".to_string(),
            language: language.to_string(),
            format: format.to_string(),
            relpath: relpath.to_string(),
        })
    }

    fn arm_age_gate_for(&self, project_root: &std::path::Path) -> MgResult<()> {
        if let Some(web) = &self.web {
            web.arm_age_gate_for(project_root)?;
        }
        Ok(())
    }

    fn manifest_owned(&self) -> bool {
        self.python_native_owned()
    }

    fn supports_native_update(&self) -> bool {
        self.resolve_first_lane().is_ok()
    }

    async fn prepare_add(
        &self,
        project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        opts: AddOptions,
    ) -> MgResult<PreparedAdd> {
        self.require_python_native_at(project_root, "add")?;
        // Resolve-first (C0 FIX2 + native-add): the pyproject/Cargo writers
        // cannot persist star ranges (every saved dep needs a bound), so
        // booking an unpinned dep in memory faked a mutation the disk never
        // saw. Resolve the real version natively FIRST for EVERY range —
        // star or explicit — through the shared native lane below (no
        // toolchain is spawned on any path); a resolve failure errors
        // honestly instead of fake-adding.
        // (Resolve-trước mọi range qua lane native dùng chung — không
        // spawn toolchain; fail trung thực thay vì add giả.)
        if let Some(prepared) = self.resolve_first_add(name, range, &opts).await? {
            return Ok(prepared);
        }
        Err(mgc_types::capabilities::unsupported_capability(
            "lib",
            "prepare add",
            "this language has no native resolve-first add lane; no placeholder package version is returned",
        ))
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        self.require_python_native_at(project_root, "parse_manifest")?;
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
        self.require_python_native_at(project_root, "list")?;
        if let Some(web) = &self.web {
            return web.list(project_root).await;
        }
        if self.language != LibLanguage::Python {
            return Err(mgc_types::capabilities::unsupported_capability(
                "lib",
                "list installed packages",
                "this language lane does not yet maintain a verified installed-artifact inventory; manifest declarations and lock pins are not proof that packages are installed",
            ));
        }
        let manifest = self.parse_manifest(project_root).await?;
        crate::install::native_python_installed_packages(project_root, &manifest)
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
        self.require_python_native("write_manifest")?;
        if self.language == LibLanguage::Java && self.java_kind != JavaManifestKind::Pom {
            return Err(mgc_types::capabilities::unsupported_capability(
                "lib",
                "write_manifest",
                "Java Gradle build scripts are not an MGC-owned manifest",
            ));
        }
        Ok(())
    }

    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        self.require_python_native_at(project_root, "write_manifest")?;
        if let Some(web) = &self.web {
            return web.write_manifest(project_root, manifest).await;
        }
        match self.language {
            LibLanguage::Rust => write_cargo_manifest(project_root, manifest),
            LibLanguage::Python => write_pyproject_manifest(project_root, manifest),
            // mgc owns the go.mod require set (native add) — every other
            // directive is preserved verbatim by the writer.
            // (mgc sở hữu require go.mod — directive khác giữ nguyên.)
            LibLanguage::Go => write_go_mod_manifest(project_root, manifest),
            // mgc owns csproj PackageReferences (native add) — the rest
            // of the project file is preserved verbatim by the writer.
            // (mgc sở hữu PackageReference — phần còn lại giữ nguyên.)
            LibLanguage::DotNet => write_csproj_manifest(project_root, manifest),
            // mgc owns pom.xml dependencies (native add) — but ONLY for
            // pom projects; gradle build scripts are programs and stay
            // read-only: fail closed instead of reporting a phantom
            // write (a silent Ok here would fake remove success).
            // (mgc sở hữu dependency pom.xml — gradle chỉ đọc, fail rõ.)
            LibLanguage::Java => match self.java_kind {
                JavaManifestKind::Pom => write_pom_manifest(project_root, manifest),
                JavaManifestKind::Gradle | JavaManifestKind::None => Err(mgc_types::MgError::Other(
                    "refusing to write: gradle build scripts are programs, not manifests (declare dependencies in a pom.xml for mgc-managed edits)".to_string(),
                )),
            },
            LibLanguage::Ts => unreachable!("ts handled by web delegate"),
        }
    }
}

#[async_trait]
impl DependencyResolver for LibAdapter {
    /// Direct adapter mutations inherit the fail-closed defaults; the CLI
    /// mutation gateway owns all dependency writes. Registry resolution is
    /// supported only by explicitly implemented native lanes.
    /// Mutation trực tiếp qua adapter fail-closed; gateway CLI sở hữu mọi
    /// lần ghi dependency. Chỉ lane native có implementation mới resolve.
    fn probe_dependency_resolver(&self) -> MgResult<()> {
        self.require_python_native("resolve")?;
        if self.language == LibLanguage::Java && self.java_kind != JavaManifestKind::Pom {
            return Err(mgc_types::capabilities::unsupported_capability(
                "lib",
                "resolve",
                "Java Gradle build scripts are not an MGC-native dependency manifest",
            ));
        }
        Ok(())
    }

    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph> {
        self.require_python_native("resolve")?;
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
        if let Some(web) = &self.web {
            return web.probe_artifact_fetcher();
        }
        Err(mgc_types::capabilities::unsupported_capability(
            "lib",
            "fetch",
            "this native language downloads and verifies artifacts inside install; standalone fetch is not exposed",
        ))
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
        self.require_python_native("install")?;
        if self.language == LibLanguage::Java && self.java_kind != JavaManifestKind::Pom {
            return Err(mgc_types::capabilities::unsupported_capability(
                "lib",
                "install",
                "this Java build lane has no MGC-native dependency graph",
            ));
        }
        Ok(())
    }

    async fn install(
        &self,
        graph: &ResolvedGraph,
        project_root: &Path,
        opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        self.require_python_native_at(project_root, "install")?;
        if self.language == LibLanguage::Java && self.java_kind != JavaManifestKind::Pom {
            return Err(mgc_types::capabilities::unsupported_capability(
                "lib",
                "install",
                "Java Gradle build scripts are not an MGC-native dependency manifest",
            ));
        }
        // Use new install pipeline (install/mod.rs)
        // Dùng install pipeline mới (install/mod.rs)
        let mut lock_packages =
            std::mem::take(&mut *self.pending_lock.lock().expect("lib pending lock poisoned"));
        if !graph.packages.is_empty() {
            // Delta operations (add after an existing lock) stage markers
            // only for their newly resolved subgraph. Fill the rest from
            // the existing exact lock pins; lock-short-circuit installs use
            // the same path. Never install a graph entry without its own
            // matching lock/integrity record.
            // (Delta resolve chỉ stage subgraph mới; bổ sung entry còn lại
            // từ lock hiện hữu đúng version.)
            if let Some(ecosystem) = lock_ecosystem(self.language) {
                let existing = crate::install::read_existing_lock(project_root)?;
                crate::install::complete_lock_packages_from_existing(
                    graph,
                    ecosystem,
                    &mut lock_packages,
                    &existing,
                );
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
impl mgc_types::capabilities::OptimizerProvider for LibAdapter {}
impl mgc_types::capabilities::SimulatorProvider for LibAdapter {}
impl mgc_types::capabilities::DeviceProvider for LibAdapter {}
impl mgc_types::capabilities::DeployProvider for LibAdapter {}
/// Materializer claim — TS-only: the embedded web engine materializes
/// the node_modules layout for real; toolchain-owned languages keep
/// their own materialization (cargo target / venv) — unclaimed there.
/// (Claim materializer — chỉ TS: web engine nhúng materialize layout
/// node_modules thật; ngôn ngữ toolchain-owned giữ materialize của
/// toolchain mình — không claim.)
impl mgc_types::capabilities::Materializer for LibAdapter {
    fn probe_materializer(&self) -> MgResult<()> {
        match &self.web {
            Some(web) => web.probe_materializer(),
            None => Err(mgc_types::capabilities::unsupported_capability(
                "lib",
                "materialize",
                "toolchain-owned lib languages materialize through their own toolchain; mgc owns resolve→lock→fetch→store",
            )),
        }
    }
}

/// LifecycleRunner claim — TS-only: scripts lifecycle (run/build/test)
/// executes through the embedded web engine for TS; other languages run
/// the language toolchain via the mgc-exec allowlist instead — unclaimed.
/// (Claim lifecycle — chỉ TS: scripts lifecycle chạy qua web engine nhúng;
/// ngôn ngữ khác chạy toolchain qua allowlist mgc-exec — không claim.)
impl mgc_types::capabilities::LifecycleRunner for LibAdapter {
    fn probe_lifecycle_runner(&self) -> MgResult<()> {
        match &self.web {
            Some(web) => web.probe_lifecycle_runner(),
            None => Err(mgc_types::capabilities::unsupported_capability(
                "lib",
                "lifecycle",
                "toolchain-owned lib languages run build/test through the language toolchain (mgc-exec allowlist), not a web-style scripts lifecycle",
            )),
        }
    }
}

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

/// Construct a LibAdapter for an EXPLICIT language — the ai core's PyPI
/// lane needs the lib python engine regardless of root-detect heuristics
/// (an ai project's pyproject.toml must route to Python, never guess).
/// (Dựng LibAdapter theo ngôn ngữ TƯỜNG MINH — lane PyPI của core ai cần
/// engine python của lib bất chấp heuristic detect theo root.)
pub fn adapter_for_language(
    language: LibLanguage,
    root: &Path,
    registry_url: Option<String>,
    token: Option<String>,
) -> Result<LibAdapter> {
    LibAdapter::for_language(language, root, registry_url, token)
}
