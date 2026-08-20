#![cfg_attr(test, allow(clippy::unwrap_used))]
//! mg-cloud-adapter — cloud ecosystem adapter (MegaGate)
//! (cdk/pulumi → delegate WebAdapter npm-format, KHÔNG gọi npm — policy §5.2;
//!  terraform → exec passthrough terraform init/plan/apply, allowlist §5.1)

use async_trait::async_trait;
use mg_types::adapter::{
    AddOptions, AuditReport, InstallOptions, InstallSummary, InstalledPackage, PackageAdapter,
    UpdatedPackage,
};
use mg_types::{
    Ecosystem, Manifest, MgResult, PackageId, PackageName, ResolvedGraph, Version, VersionRange,
};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudType {
    Cdk,
    Pulumi,
    Terraform,
    Cloudflare,
}

impl CloudType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CloudType::Cdk => "cdk",
            CloudType::Pulumi => "pulumi",
            CloudType::Terraform => "terraform",
            CloudType::Cloudflare => "cloudflare",
        }
    }
}

pub struct CloudAdapter {
    cloud_type: CloudType,
    web: Option<mg_web_adapter::WebAdapter>,
}

/// Detect cloud type — ưu tiên mg.toml `[cloud] type`, fallback manifest probe.
pub fn detect_type(root: &Path) -> Option<CloudType> {
    if let Ok(content) = std::fs::read_to_string(root.join("mg.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(t) = v
                .get("cloud")
                .and_then(|c| c.get("type"))
                .and_then(|t| t.as_str())
            {
                return match t {
                    "cdk" => Some(CloudType::Cdk),
                    "pulumi" => Some(CloudType::Pulumi),
                    "terraform" => Some(CloudType::Terraform),
                    "cloudflare" => Some(CloudType::Cloudflare),
                    _ => None,
                };
            }
        }
    }
    if root.join("wrangler.toml").exists() {
        return Some(CloudType::Cloudflare);
    }
    if root.join("Pulumi.yaml").exists() {
        return Some(CloudType::Pulumi);
    }
    if has_tf_files(root) {
        return Some(CloudType::Terraform);
    }
    if let Ok(content) = std::fs::read_to_string(root.join("package.json")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            let has_cdk = v
                .get("dependencies")
                .and_then(|d| d.as_object())
                .map(|deps| deps.keys().any(|k| k.starts_with("aws-cdk") || k == "cdk"))
                .unwrap_or(false);
            if has_cdk {
                return Some(CloudType::Cdk);
            }
        }
    }
    None
}

fn has_tf_files(root: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    entries
        .flatten()
        .any(|e| e.path().extension().is_some_and(|ext| ext == "tf"))
}

fn manifest_is_cloud(root: &Path) -> bool {
    if let Ok(content) = std::fs::read_to_string(root.join("mg.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(eco) = v.get("ecosystem").and_then(|e| e.as_str()) {
                if eco == "cloud" {
                    return true;
                }
            }
            if v.get("cloud").is_some() {
                return true;
            }
        }
    }
    detect_type(root).is_some()
}

pub fn adapter_for(root: &Path) -> Option<CloudAdapter> {
    let cloud_type = detect_type(root)?;
    let web = if matches!(cloud_type, CloudType::Cdk | CloudType::Pulumi) {
        Some(mg_web_adapter::WebAdapter::new())
    } else {
        None
    };
    Some(CloudAdapter { cloud_type, web })
}

fn exec_tool(root: &Path, cmd: &str, args: &[String]) -> MgResult<()> {
    let opts = mg_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".megagate").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    mg_exec::prelude::run(cmd, args, &opts).map_err(|e| mg_types::MgError::Other(e.to_string()))?;
    Ok(())
}

fn no_package_manager(cloud_type: CloudType) -> MgResult<()> {
    Err(mg_types::MgError::Other(format!(
        "{} has no package manager — write resources in HCL directly; use `mg dev` / `mg deploy`",
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
        let _ = manifest;
        Ok(())
    }

    async fn resolve(&self, manifest: &Manifest) -> MgResult<ResolvedGraph> {
        if let Some(web) = &self.web {
            return web.resolve(manifest).await;
        }
        let _ = manifest;
        Ok(ResolvedGraph::default())
    }

    async fn fetch(&self, _graph: &ResolvedGraph) -> MgResult<()> {
        Ok(())
    }

    async fn install(
        &self,
        _graph: &ResolvedGraph,
        project_root: &Path,
        _opts: InstallOptions,
    ) -> MgResult<InstallSummary> {
        if let Some(web) = &self.web {
            return web.install(_graph, project_root, _opts).await;
        }
        // ponytail: terraform passthrough — mỗi install = init + get (Q17 S3)
        exec_tool(project_root, "terraform", &["init".to_string()])?;
        exec_tool(project_root, "terraform", &["get".to_string()])?;
        Ok(InstallSummary::default())
    }

    async fn add(
        &self,
        _project_root: &Path,
        _name: &PackageName,
        _range: Option<&VersionRange>,
        _opts: AddOptions,
    ) -> MgResult<PackageId> {
        if let Some(web) = &self.web {
            return web.add(_project_root, _name, _range, _opts).await;
        }
        no_package_manager(self.cloud_type)?;
        unreachable!()
    }

    async fn remove(&self, _project_root: &Path, _name: &PackageName) -> MgResult<()> {
        if let Some(web) = &self.web {
            return web.remove(_project_root, _name).await;
        }
        no_package_manager(self.cloud_type)
    }

    async fn update(
        &self,
        _project_root: &Path,
        _name: Option<&PackageName>,
    ) -> MgResult<Vec<UpdatedPackage>> {
        if let Some(web) = &self.web {
            return web.update(_project_root, _name).await;
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
        let manifest = self.parse_manifest(project_root).await?;
        Ok(AuditReport::clean(manifest.all_dependencies().count()))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mg-cloud-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detect_cdk_via_package_json() {
        let dir = tmp_dir("cdk");
        std::fs::write(
            dir.join("package.json"),
            "{\"name\":\"infra\",\"dependencies\":{\"aws-cdk-lib\":\"^2.0.0\"}}",
        )
        .unwrap();
        assert_eq!(detect_type(&dir), Some(CloudType::Cdk));
    }

    #[test]
    fn detect_pulumi_via_yaml() {
        let dir = tmp_dir("pulumi");
        std::fs::write(dir.join("Pulumi.yaml"), "name: infra\nruntime: nodejs\n").unwrap();
        assert_eq!(detect_type(&dir), Some(CloudType::Pulumi));
    }

    #[test]
    fn detect_terraform_via_tf_files() {
        let dir = tmp_dir("tf");
        std::fs::write(dir.join("main.tf"), "provider \"aws\" {}\n").unwrap();
        assert_eq!(detect_type(&dir), Some(CloudType::Terraform));
    }

    #[test]
    fn detect_via_mg_toml_type() {
        let dir = tmp_dir("cfg");
        std::fs::write(dir.join("mg.toml"), "[cloud]\ntype = \"pulumi\"\n").unwrap();
        assert_eq!(detect_type(&dir), Some(CloudType::Pulumi));
    }

    #[test]
    fn no_manifest_detects_nothing() {
        let dir = tmp_dir("empty");
        assert!(detect_type(&dir).is_none());
    }

    #[test]
    fn terraform_add_bails_no_package_manager() {
        let dir = tmp_dir("tfadd");
        std::fs::write(dir.join("main.tf"), "provider \"aws\" {}\n").unwrap();
        let adapter = adapter_for(&dir).unwrap();
        assert_eq!(adapter.cloud_type(), "terraform");
        let name = PackageName::new("foo").unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt
            .block_on(adapter.add(&dir, &name, None, AddOptions::default()))
            .is_err());
    }
}
