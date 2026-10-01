use std::path::PathBuf;
use std::sync::Arc;

use mgc_config::project::{ProjectConfig, ProjectExecutionConfig};
use mgc_types::Ecosystem;
use mgc_types::adapter::PackageAdapter;

/// Project context: detects project, loads mgc.toml, provides right adapter.
pub struct ProjectContext {
    pub root: PathBuf,
    #[allow(dead_code)]
    pub config: ProjectConfig,
    pub adapter: Arc<dyn PackageAdapter>,
}

impl ProjectContext {
    /// Load context, optionally with explicit `--core` override.
    /// Persisted project identity is authoritative; `--core` can confirm but
    /// cannot reassign it. On an unclaimed project the flag may select a core
    /// only when it agrees with a single detected signature.
    /// Identity đã lưu là chủ sở hữu; `--core` chỉ xác nhận, không được đổi.
    pub fn load_with_core(core_override: Option<&str>) -> anyhow::Result<Self> {
        let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
        let project_root = ProjectConfig::find_project_root(&cwd);
        Self::load_at(cwd.as_path(), project_root.as_ref(), core_override)
    }

    /// Load context anchored at an explicit cwd (workspace mix core: mỗi
    /// project trong monorepo tự detect core riêng).
    pub fn load_at(
        cwd: &std::path::Path,
        project_root: Option<&PathBuf>,
        core_override: Option<&str>,
    ) -> anyhow::Result<Self> {
        let (root, config) = Self::resolve_config(cwd, project_root, core_override)?;
        let ecosystem = Ecosystem::from_str(&config.ecosystem)
            .ok_or_else(|| crate::error::unknown_ecosystem(&config.ecosystem))?;

        // Registry chain hợp nhất (ITEM 2): env override → mgc.toml [[registries]]
        // (priority) → .npmrc registry= → npmjs default. entry 0 = primary.
        let chain = mgc_config::chain::registry_chain(Some(&root), Some(&config));
        let primary = chain
            .first()
            .map(|r| r.url.clone())
            .ok_or_else(crate::error::no_registry_configured)?;
        let primary_token = chain.first().and_then(|r| r.token.clone());
        let fallbacks: Vec<(String, Option<String>)> = chain
            .iter()
            .skip(1)
            .map(|r| (r.url.clone(), r.token.clone()))
            .collect();

        let adapter = crate::factory::create_adapter_for(
            &root,
            &ecosystem,
            Some(&primary),
            primary_token.as_deref(),
            &fallbacks,
        )?;
        Ok(Self {
            root,
            config,
            adapter,
        })
    }

    /// Load context for a single workspace project (mix core detect riêng).
    pub fn load_for_dir(dir: &std::path::Path) -> anyhow::Result<Self> {
        let project_root = ProjectConfig::find_project_root(dir);
        Self::load_at(dir, project_root.as_ref(), None)
    }

    fn resolve_config(
        cwd: &std::path::Path,
        project_root: Option<&PathBuf>,
        core_override: Option<&str>,
    ) -> anyhow::Result<(PathBuf, ProjectConfig)> {
        if let Some(root) = project_root {
            // The persisted marker is an ownership record, not a hint that
            // `--core` may override. Broken/conflicting identity fails closed.
            // (Marker là owner đã lưu; identity hỏng/lệch thì dừng fail-closed.)
            let marker_core = ProjectConfig::read_core_marker(root)?;

            // An explicit selector may choose a core for an unclaimed project,
            // but must not silently reassign a project with persisted identity.
            // (Cờ chọn core chỉ chọn cho project chưa claim, không được đổi owner đã lưu.)
            if let (Some(requested), Some(owned)) = (core_override, marker_core.as_deref()) {
                Self::ensure_override_matches_identity(requested, owned)?;
            }

            if let Some(mut cfg) = ProjectConfig::load(root)? {
                if let Some(core) = core_override {
                    Self::ensure_override_matches_identity(core, &cfg.ecosystem)?;
                    let canonical = Ecosystem::from_str(core)
                        .ok_or_else(|| crate::error::unknown_ecosystem(core))?;
                    cfg.ecosystem = canonical.as_str().to_string();
                } else if let Some(marker) = &marker_core {
                    // A conflicting marker is rejected while reading above; this branch
                    // only accepts an already-validated identity.
                    // Marker mâu thuẫn đã bị từ chối khi đọc ở trên; nhánh này chỉ nhận identity hợp lệ.
                    if cfg.ecosystem != marker[..] {
                        cfg.ecosystem = marker.clone();
                    }
                }
                if marker_core.is_none() {
                    // Adopt the identity from an existing config before any
                    // core-aware command can act on the legacy project.
                    // (Claim core từ config cũ trước khi lệnh nào được thao tác.)
                    let canonical = Ecosystem::from_str(&cfg.ecosystem)
                        .ok_or_else(|| crate::error::unknown_ecosystem(&cfg.ecosystem))?;
                    match ProjectConfig::ensure_core_marker_at(root, canonical.as_str()) {
                        Ok(()) => mgc_ui::warning(&format!(
                            "No '{}' found — recording the existing mgc.toml core '{}' as project identity.",
                            ProjectConfig::CORE_MARKER_FILE,
                            canonical.as_str(),
                        )),
                        Err(error)
                            if error.downcast_ref::<std::io::Error>().is_some_and(|io| {
                                io.kind() == std::io::ErrorKind::PermissionDenied
                            }) =>
                        {
                            mgc_ui::warning(&format!(
                                "Cannot persist '{}' because the project directory is read-only; using the existing mgc.toml core '{}' as identity.",
                                ProjectConfig::CORE_MARKER_FILE,
                                canonical.as_str(),
                            ));
                        }
                        Err(error) => return Err(error),
                    }
                }
                return Ok((root.clone(), cfg));
            }

            if let Some(eco) = ProjectConfig::detect_core(root)? {
                if let Some(requested) = core_override {
                    Self::ensure_override_matches_identity(requested, &eco)?;
                }
                let eco = core_override.unwrap_or(&eco);
                // T9a: tự ghi marker khi vừa detect từ signature — lần sau
                // không còn phụ thuộc thứ tự file (cảnh báo rõ ràng).
                if marker_core.is_none() {
                    ProjectConfig::ensure_core_marker_at(root, eco)?;
                    mgc_ui::warning(&format!(
                        "No '{}' found — claiming project identity as core '{}'. Core reassignment is blocked until an explicit migration workflow is available.",
                        ProjectConfig::CORE_MARKER_FILE,
                        eco,
                    ));
                }
                let name = Self::dir_name(root);
                return Ok((root.clone(), ProjectConfig::new(name, eco)));
            }

            if let Some(core) = core_override {
                let name = Self::dir_name(root);
                return Ok((root.clone(), ProjectConfig::new(name, core)));
            }

            return Err(crate::error::cannot_detect_project_type(root));
        }

        if let Some(core) = core_override {
            return Ok((cwd.to_path_buf(), ProjectConfig::new("project", core)));
        }

        Err(crate::error::no_mgc_project_root())
    }

    fn ensure_override_matches_identity(requested: &str, owned: &str) -> anyhow::Result<()> {
        let requested_core = Ecosystem::from_str(requested)
            .ok_or_else(|| crate::error::unknown_ecosystem(requested))?;
        let owned_core =
            Ecosystem::from_str(owned).ok_or_else(|| crate::error::unknown_ecosystem(owned))?;
        if requested_core != owned_core {
            anyhow::bail!(
                "core override '{}' conflicts with existing project core '{}'; core reassignment is not supported by this version",
                requested_core.as_str(),
                owned_core.as_str(),
            );
        }
        Ok(())
    }

    fn dir_name(path: &std::path::Path) -> String {
        path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".to_string())
    }

    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    pub fn adapter(&self) -> &dyn PackageAdapter {
        self.adapter.as_ref()
    }

    pub fn execution(&self) -> &ProjectExecutionConfig {
        &self.config.execution
    }

    pub fn execution_summary(&self) -> String {
        let execution = self.execution();
        let native_targets = if execution.native_targets.is_empty() {
            "none".to_string()
        } else {
            execution.native_targets.join(", ")
        };

        format!(
            "architecture={}, lane={}, compatibility={}, native_targets={}",
            execution.architecture, execution.lane, execution.compatibility_layer, native_targets
        )
    }
}

#[cfg(test)]
#[path = "test/context.rs"]
mod tests;
