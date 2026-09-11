//! PackageAdapter implementation for cloud cores.
//! Điều phối CDK/Pulumi delegate và Terraform passthrough riêng khỏi detect.

use crate::cloud_type::{CloudType, detect_type, manifest_is_cloud};
use crate::tooling::exec_tool;
use async_trait::async_trait;
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
};
use mgc_types::{
    Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph, Version, VersionRange,
};
use std::path::{Path, PathBuf};

pub struct CloudAdapter {
    cloud_type: CloudType,
    web: Option<mgc_web_adapter::WebAdapter>,
}

pub fn adapter_for(root: &Path) -> Option<CloudAdapter> {
    let cloud_type = detect_type(root)?;
    let web = if matches!(cloud_type, CloudType::Cdk | CloudType::Pulumi) {
        Some(mgc_web_adapter::WebAdapter::new())
    } else {
        None
    };
    Some(CloudAdapter { cloud_type, web })
}

fn no_package_manager(cloud_type: CloudType) -> MgResult<()> {
    Err(mgc_types::MgError::Other(format!(
        "{} has no package manager — write resources in HCL directly; use `mgc dev` / `mgc deploy`",
        cloud_type.as_str()
    )))
}

#[async_trait]
impl PackageAdapter for CloudAdapter {
    fn name(&self) -> &str {
        "cloud"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Cloud
    }

    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_cloud(project_root)
    }

    async fn parse_manifest(&self, project_root: &Path) -> MgResult<Manifest> {
        if let Some(web) = &self.web {
            return web.parse_manifest(project_root).await;
        }
        let name = project_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "infra".to_string());
        Ok(Manifest::new(&name, Ecosystem::Cloud))
    }

    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        if let Some(web) = &self.web {
            return web.write_manifest(project_root, manifest).await;
        }
        Ok(())
    }

    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph> {
        if let Some(web) = &self.web {
            return web.resolve(manifest).await;
        }
        // Terraform/CDK modules are not resolved through a registry graph.
        // Module Terraform/CDK không resolve qua graph registry.
        Err(mgc_types::MgError::Unsupported {
            core: "cloud",
            capability: "resolve",
            guidance: "cloud dependencies are managed by terraform/CDK \
                       (`terraform init` runs during install); no registry graph"
                .to_string(),
        })
    }

    async fn fetch(&self, graph: &ResolvedGraph) -> MgResult<()> {
        if let Some(web) = &self.web {
            return web.fetch(graph).await;
        }
        Err(mgc_types::MgError::Unsupported {
            core: "cloud",
            capability: "fetch",
            guidance: "terraform modules are fetched by `terraform init` \
                       during install; there is no separate fetch step"
                .to_string(),
        })
    }

    async fn install(
        &self,
        graph: &ResolvedGraph,
        project_root: &Path,
        opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        if let Some(web) = &self.web {
            return web.install(graph, project_root, opts).await;
        }
        exec_tool(project_root, "terraform", &["init".to_string()])?;
        exec_tool(project_root, "terraform", &["get".to_string()])?;
        Ok(InstallSummary::default())
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
        no_package_manager(self.cloud_type)?;
        unreachable!()
    }

    async fn remove(&self, project_root: &Path, name: &PackageName) -> MgResult<()> {
        if let Some(web) = &self.web {
            return web.remove(project_root, name).await;
        }
        no_package_manager(self.cloud_type)
    }

    async fn update(
        &self,
        project_root: &Path,
        name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>> {
        if let Some(web) = &self.web {
            return web.update(project_root, name).await;
        }
        no_package_manager(self.cloud_type)?;
        Ok(vec![])
    }

    async fn list(&self, project_root: &Path) -> MgResult<Vec<InstalledPackage>> {
        if let Some(web) = &self.web {
            return web.list(project_root).await;
        }
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

    async fn audit(&self, project_root: &Path) -> MgResult<AuditReport> {
        if let Some(web) = &self.web {
            return web.audit(project_root).await;
        }
        // P2 2026-09-10: cloud now owns its TARGET-ecosystem lane —
        // Terraform provider-lock provenance — while sibling app
        // manifests still join through the polyglot engine.
        // Cloud giờ có lane ecosystem ĐÍCH — nguồn gốc provider-lock
        // Terraform — manifest app kề vẫn qua engine polyglot.
        let mut plan = mgc_audit::plan_for_shared_manifests(project_root)?;
        let has_terraform = project_root.join(".terraform.lock.hcl").is_file();
        if has_terraform {
            let root = project_root.to_path_buf();
            plan.add_step(mgc_audit::ScanStep {
                ecosystem: "cloud/terraform",
                scanner: "terraform-provider-lock",
                run: Box::new(move || {
                    let root = root.clone();
                    Box::pin(async move { mgc_audit::scanners::audit_terraform_lock(&root) })
                }),
            });
        }
        let manifest = self.parse_manifest(project_root).await?;
        let label = format!(
            "cloud ({} dependencies not scanned — no scanner implemented yet)",
            manifest.all_dependencies().count()
        );
        if plan.is_empty() {
            return Ok(mgc_types::adapter::AuditReport::unsupported_ecosystem(
                label,
            ));
        }
        plan.execute().await
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

impl CloudAdapter {
    pub fn cloud_type(&self) -> &'static str {
        self.cloud_type.as_str()
    }
}
