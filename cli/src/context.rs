use std::path::PathBuf;
use std::sync::Arc;

use mg_config::project::{ProjectConfig, ProjectExecutionConfig};
use mg_types::adapter::PackageAdapter;
use mg_types::Ecosystem;

/// Project context: detects project, loads mg.toml, provides right adapter.
pub struct ProjectContext {
    pub root: PathBuf,
    #[allow(dead_code)]
    pub config: ProjectConfig,
    pub adapter: Arc<dyn PackageAdapter>,
}

impl ProjectContext {
    /// Load context, optionally with explicit `--core` override.
    /// Priority:
    ///   1. `--core` flag
    ///   2. `mg.toml` (saved by `mg init`)
    ///   3. auto_detect (package.json → web, Cargo.toml → lib, pyproject.toml → ai)
    pub fn load_with_core(core_override: Option<&str>) -> anyhow::Result<Self> {
        let cwd = std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("failed to resolve current working directory: {}", e))?;
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
            .ok_or_else(|| anyhow::anyhow!("Unknown ecosystem: '{}'", config.ecosystem))?;

        // Registry override từ mg.toml [registry] (url + token) — ưu tiên env
        let registry_entry = config.registries.iter().find(|r| {
            !r.url.contains("registry.npmjs.org")
                || std::env::var("MEGAGATE_WEB_REGISTRY_URL").is_ok()
        });
        let registry_url = std::env::var("MEGAGATE_WEB_REGISTRY_URL")
            .ok()
            .or_else(|| registry_entry.map(|r| r.url.clone()));
        let token = std::env::var("MEGAGATE_WEB_REGISTRY_TOKEN")
            .ok()
            .or_else(|| registry_entry.and_then(|r| r.token.clone()));

        let adapter = crate::factory::create_adapter_for(
            &root,
            &ecosystem,
            registry_url.as_deref(),
            token.as_deref(),
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
            if let Some(mut cfg) = ProjectConfig::load(root)? {
                if let Some(core) = core_override {
                    cfg.ecosystem = core.to_string();
                }
                return Ok((root.clone(), cfg));
            }

            if let Some(eco) = ProjectConfig::auto_detect(root) {
                let eco = core_override.unwrap_or(&eco);
                let name = Self::dir_name(root);
                return Ok((root.clone(), ProjectConfig::new(name, eco)));
            }

            if let Some(core) = core_override {
                let name = Self::dir_name(root);
                return Ok((root.clone(), ProjectConfig::new(name, core)));
            }

            anyhow::bail!(
                "Cannot detect project type in '{}'. Run {} or specify {}",
                root.display(),
                mg_ui::style_cmd("mg init"),
                mg_ui::style_cmd("--core <type>"),
            );
        }

        if let Some(core) = core_override {
            return Ok((cwd.to_path_buf(), ProjectConfig::new("project", core)));
        }

        anyhow::bail!(
            "No MegaGate project found.\n\
             Run '{}' to create one, or specify {}.",
            mg_ui::style_cmd("mg init"),
            mg_ui::style_cmd("--core <type>"),
        );
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
