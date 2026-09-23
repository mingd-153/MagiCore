//! PackageAdapter implementation for cloud cores.
//! Điều phối CDK/Pulumi delegate và Terraform passthrough riêng khỏi detect.
//!
//! Global Gate 1 (2026-09-16): DependencyResolver/ArtifactFetcher/
//! LockfileProvider are NOT claimed — the registry surface is real only
//! through the embedded web engine (CDK/Pulumi), so the conditional
//! delegates stay as overrides while `resolve` remains fail-closed for
//! Terraform. The terraform write_manifest silent-Ok no-op is now
//! fail-closed (hardware precedent). What IS claimed: detection,
//! scaffold, deploy, lifecycle, install (real in both branches), audit.
//! Global Gate 1: KHÔNG claim DependencyResolver/ArtifactFetcher/
//! LockfileProvider — mặt registry chỉ thật qua web engine nhúng
//! (CDK/Pulumi), delegate điều kiện giữ nguyên làm override còn `resolve`
//! vẫn fail-closed cho Terraform. No-op Ok âm thầm của write_manifest
//! terraform giờ fail-closed (tiền lệ hardware). Claim thật: detect,
//! scaffold, deploy, lifecycle, install (thật ở cả 2 nhánh), audit.

use crate::cloud_type::{CloudType, detect_type, manifest_is_cloud};
use crate::tooling::exec_tool;
use async_trait::async_trait;
use mgc_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
};
use mgc_types::capabilities::{
    ArtifactFetcher, AuditProvider, Capability, ContentStoreProvider, CoreIdent,
    DependencyResolver, DeployProvider, LifecycleRunner, LockfileProvider, ProjectDetector,
    ScaffoldProvider,
};
use mgc_types::{
    Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph, VersionRange,
};
use std::path::{Path, PathBuf};

pub struct CloudAdapter {
    cloud_type: CloudType,
    web: Option<mgc_web_adapter::WebAdapter>,
}

impl CloudAdapter {
    /// Capability manifest (Global Gate 1) — code-reality notes:
    /// - ProjectDetector: `detect_type`/`manifest_is_cloud` — real.
    /// - ScaffoldProvider: src/scaffold + `mgc create-clo` — real.
    /// - DeployProvider: src/deploy (cdk deploy / pulumi up /
    ///   terraform apply via mgc-exec, dry-run default) — real.
    /// - LifecycleRunner: install runs `terraform init`/`terraform get`;
    ///   CDK/Pulumi ride the web engine — real.
    /// - ContentStoreProvider: install is REAL in both branches
    ///   (terraform passthrough or web-engine delegate) — claimed.
    /// - AuditProvider: terraform provider-lock lane + polyglot + web
    ///   delegate — real.
    ///
    /// Bảng capability (Global Gate 1) — ghi chú theo code thật.
    pub const CAPABILITIES: &'static [Capability] = &[
        Capability::ProjectDetector,
        Capability::ScaffoldProvider,
        Capability::DeployProvider,
        Capability::LifecycleRunner,
        Capability::ContentStoreProvider,
        Capability::AuditProvider,
    ];

    pub fn cloud_type(&self) -> &'static str {
        self.cloud_type.as_str()
    }
}

// P0-4 (2026-09-15): fallible construction — WebAdapter::new() carries the
// typed registry-URL guard now, so adapter_for propagates an Err instead of
// aborting. "Not a cloud project" stays Ok(None) (an absence is not an error).
// (P0-4: việc dựng adapter có thể lỗi — WebAdapter::new() mang guard URL
// typed, adapter_for propagate Err thay vì abort. "Không phải project
// cloud" vẫn là Ok(None) — vắng mặt không phải là lỗi.)
pub fn adapter_for(root: &Path) -> anyhow::Result<Option<CloudAdapter>> {
    let Some(cloud_type) = detect_type(root) else {
        return Ok(None);
    };
    let web = if matches!(cloud_type, CloudType::Cdk | CloudType::Pulumi) {
        Some(mgc_web_adapter::WebAdapter::new()?)
    } else {
        None
    };
    Ok(Some(CloudAdapter { cloud_type, web }))
}

fn no_package_manager(cloud_type: CloudType) -> MgResult<()> {
    Err(mgc_types::MgError::Other(format!(
        "{} has no package manager — write resources in HCL directly; use `mgc dev` / `mgc deploy`",
        cloud_type.as_str()
    )))
}

impl CoreIdent for CloudAdapter {
    fn core_id(&self) -> &'static str {
        "clo"
    }

    fn name(&self) -> &str {
        "cloud"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Cloud
    }
}

impl ProjectDetector for CloudAdapter {
    fn can_handle(&self, project_root: &Path) -> bool {
        manifest_is_cloud(project_root)
    }
}

impl ScaffoldProvider for CloudAdapter {
    /// Evidence: src/scaffold (terraform/cdk/pulumi generators) + the
    /// `mgc create-clo` CLI lane.
    /// Dẫn chứng: src/scaffold (bộ sinh terraform/cdk/pulumi) + lane CLI
    /// `mgc create-clo`.
    fn probe_scaffold(&self) -> MgResult<()> {
        Ok(())
    }
}

impl DeployProvider for CloudAdapter {
    /// Evidence: src/deploy/mod.rs — real mgc-exec passthrough
    /// (`cdk deploy` / `pulumi up` / `terraform apply`, dry-run default).
    /// Dẫn chứng: src/deploy/mod.rs — passthrough mgc-exec thật
    /// (`cdk deploy` / `pulumi up` / `terraform apply`, dry-run mặc định).
    fn probe_deploy(&self) -> MgResult<()> {
        Ok(())
    }
}

impl LifecycleRunner for CloudAdapter {
    /// Evidence: install runs `terraform init` + `terraform get` (below);
    /// CDK/Pulumi ride the web engine lifecycle.
    /// Dẫn chứng: install chạy `terraform init` + `terraform get` (bên
    /// dưới); CDK/Pulumi đi trên lifecycle web engine.
    fn probe_lifecycle_runner(&self) -> MgResult<()> {
        Ok(())
    }
}

#[async_trait]
impl ContentStoreProvider for CloudAdapter {
    /// Evidence: install is REAL in both branches — terraform passthrough
    /// (exec_tool init/get) or the embedded web engine pipeline.
    /// Dẫn chứng: install THẬT ở cả hai nhánh — passthrough terraform
    /// (exec_tool init/get) hoặc pipeline web engine nhúng.
    fn probe_content_store(&self) -> MgResult<()> {
        Ok(())
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
        // DELEGATED: install is owned by terraform (`init` + `get` ran
        // for real above and fetched the modules/providers); mgc does
        // not own this lifecycle. The summary is HONEST — it only
        // counts the packages a provided graph named (usually empty for
        // terraform: resolve is fail-closed, there is no registry
        // graph). Empty graph → empty summary (truthful), never a
        // fabricated list; cache bytes stay uncounted (Delegated mode).
        // DELEGATED: install thuộc terraform (`init` + `get` đã chạy thật
        // bên trên và tải module/provider); mgc KHÔNG sở hữu lifecycle
        // này. Summary TRUNG THỰC — chỉ đếm package mà graph cung cấp nêu
        // (thường rỗng với terraform: resolve fail-closed, không có graph
        // registry). Graph rỗng → summary rỗng (trung thực), không bao
        // giờ bịa danh sách; byte cache không đếm (chế độ Delegated).
        Ok(InstallSummary {
            added: graph.packages.iter().map(|p| p.id.clone()).collect(),
            cache_mode: mgc_types::adapter::InstallCacheMode::Delegated,
            ..Default::default()
        })
    }
}

#[async_trait]
impl LockfileProvider for CloudAdapter {
    async fn write_manifest(&self, project_root: &Path, manifest: &Manifest) -> MgResult<()> {
        if let Some(web) = &self.web {
            return web.write_manifest(project_root, manifest).await;
        }
        // Fail-closed (hardware precedent): the old silent Ok no-op faked
        // a manifest write that never happened for terraform projects.
        // Fail-closed (tiền lệ hardware): Ok no-op cũ giả một lần ghi
        // manifest không bao giờ xảy ra cho project terraform.
        Err(mgc_types::MgError::Unsupported {
            core: "cloud",
            capability: "write_manifest",
            guidance: "terraform projects do not use mgc-written manifests; \
                       write resources in HCL directly or scaffold via `mgc create-clo`"
                .to_string(),
        })
    }
}

// Registry surface: REAL only through the embedded web engine (CDK/Pulumi).
// Terraform answers fail-closed. The capability is therefore NOT claimed —
// the delegates stay so mixed core flows keep working.
// Mặt registry: CHỈ thật qua web engine nhúng (CDK/Pulumi). Terraform trả
// fail-closed. Vì thế KHÔNG claim capability — delegate giữ nguyên để các
// flow mixed core vẫn chạy.
#[async_trait]
impl DependencyResolver for CloudAdapter {
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
}

#[async_trait]
impl ArtifactFetcher for CloudAdapter {
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
}

#[async_trait]
impl AuditProvider for CloudAdapter {
    /// Evidence: terraform provider-lock lane + polyglot engine + web
    /// delegate (below). Dẫn chứng: lane terraform provider-lock + engine
    /// polyglot + web delegate (bên dưới).
    fn probe_audit_provider(&self) -> MgResult<()> {
        Ok(())
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
}

#[async_trait]
impl PackageAdapter for CloudAdapter {
    fn capabilities(&self) -> &'static [Capability] {
        Self::CAPABILITIES
    }

    fn manifest_identity(&self) -> Option<mgc_types::ManifestIdentity> {
        // Source-verified manifest per cloud type (cloud_type.rs
        // detection; CDK rides the web package.json delegate).
        // (Manifest theo loại cloud.)
        let (language, format, relpath) = match self.cloud_type {
            CloudType::Cdk => ("cdk", "package.json", "package.json"),
            CloudType::Pulumi => ("pulumi", "Pulumi.yaml", "Pulumi.yaml"),
            CloudType::Terraform => ("terraform", ".terraform.lock.hcl", ".terraform.lock.hcl"),
            CloudType::Cloudflare => ("cloudflare", "wrangler.toml", "wrangler.toml"),
        };
        Some(mgc_types::ManifestIdentity {
            core: "cloud".to_string(),
            language: language.to_string(),
            format: format.to_string(),
            relpath: relpath.to_string(),
        })
    }

    /// P0/F6: forward to the embedded web engine (TS delegate resolves
    /// through it — its gate must arm from the same project).
    /// (Chuyển cho web engine nhúng.)
    fn arm_age_gate_for(&self, project_root: &std::path::Path) -> MgResult<()> {
        if let Some(web) = &self.web {
            web.arm_age_gate_for(project_root)?;
        }
        Ok(())
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
                        .unwrap_or_else(|| mgc_types::Version::new(0, 1, 0)),
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
impl mgc_types::capabilities::OptimizerProvider for CloudAdapter {}
impl mgc_types::capabilities::Materializer for CloudAdapter {}
impl mgc_types::capabilities::SimulatorProvider for CloudAdapter {}
impl mgc_types::capabilities::DeviceProvider for CloudAdapter {}
impl mgc_types::capabilities::ModelRuntimeProvider for CloudAdapter {}
