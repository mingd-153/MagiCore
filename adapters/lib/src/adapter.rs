//! PackageAdapter implementation for library cores.
//! Điều phối TS/Rust/Python lib mà không nhồi mọi logic vào lib.rs.

use crate::language::{detect_language, manifest_is_lib, LibLanguage};
use crate::manifest::{
    parse_cargo_manifest, parse_pyproject_manifest, write_cargo_manifest, write_pyproject_manifest,
};
use crate::tooling::{
    cargo_lock_versions, check_pip_allowed, dist_info_versions, exec_tool, placeholder_id,
    version_from_manifest,
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
        let manifest = self.parse_manifest(project_root).await?;
        let count = manifest.all_dependencies().count();
        // P0.6 FIX: Return unavailable instead of fake clean
        Ok(AuditReport::unavailable(format!(
            "No audit scanner available for lib core ({} dependencies not scanned)",
            count
        )))
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
