//! PackageAdapter implementation for library cores.
//! Điều phối TS/Rust/Python lib mà không nhồi mọi logic vào lib.rs.

use crate::language::{LibLanguage, detect_language, manifest_is_lib};
use crate::manifest::{
    parse_cargo_manifest, parse_go_mod_manifest, parse_pyproject_manifest, write_cargo_manifest,
    write_pyproject_manifest,
};
use crate::tooling::{
    cargo_lock_versions, check_pip_allowed, dist_info_versions, exec_tool, go_module_path,
    placeholder_id, version_from_manifest,
};
use async_trait::async_trait;
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
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
    fn for_language(
        language: LibLanguage,
        registry_url: Option<String>,
        token: Option<String>,
    ) -> Self {
        Self::for_language_with_chain(language, registry_url, token, &[])
    }

    fn for_language_with_chain(
        language: LibLanguage,
        registry_url: Option<String>,
        token: Option<String>,
        fallbacks: &[(String, Option<String>)],
    ) -> Self {
        let web = if language == LibLanguage::Ts {
            Some(match (registry_url, token) {
                (Some(url), token) => {
                    mgc_web_adapter::WebAdapter::with_registry_chain(url, token, fallbacks.to_vec())
                }
                (None, _) => mgc_web_adapter::WebAdapter::new(),
            })
        } else {
            None
        };
        Self { language, web }
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

#[async_trait]
impl PackageAdapter for LibAdapter {
    fn name(&self) -> &str {
        "lib"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Lib
    }

    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_lib(project_root)
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

    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph> {
        if let Some(web) = &self.web {
            return web.resolve(manifest).await;
        }
        Ok(ResolvedGraph::default())
    }

    async fn fetch(&self, _graph: &ResolvedGraph) -> MgResult<()> {
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

    async fn add(
        &self,
        project_root: &Path,
        name: &PackageName,
        range: Option<&VersionRange>,
        opts: AddOptions,
    ) -> MgResult<PackageId> {
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

pub fn adapter_for(
    root: &Path,
    registry_url: Option<String>,
    token: Option<String>,
) -> Option<LibAdapter> {
    let language = detect_language(root)?;
    Some(LibAdapter::for_language(language, registry_url, token))
}

pub fn adapter_for_with_chain(
    root: &Path,
    registry_url: Option<String>,
    token: Option<String>,
    fallbacks: &[(String, Option<String>)],
) -> Option<LibAdapter> {
    let language = detect_language(root)?;
    Some(LibAdapter::for_language_with_chain(
        language,
        registry_url,
        token,
        fallbacks,
    ))
}
