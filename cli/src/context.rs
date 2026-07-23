use std::path::PathBuf;

use anyhow::Context;
use mg_config::project::ProjectConfig;
use mg_types::adapter::PackageAdapter;
use mg_types::Ecosystem;

/// Project context: detects project, loads mg.toml, provides right adapter.
pub struct ProjectContext {
    pub root: PathBuf,
    #[allow(dead_code)]
    pub config: ProjectConfig,
    pub adapter: Box<dyn PackageAdapter>,
}

impl ProjectContext {
    /// Load context, optionally with explicit `--core` override.
    /// Priority:
    ///   1. `--core` flag
    ///   2. `mg.toml` (saved by `mg init`)
    ///   3. auto_detect (package.json → web, Cargo.toml → lib, pyproject.toml → ai)
    pub fn load_with_core(core_override: Option<&str>) -> anyhow::Result<Self> {
        let cwd = std::env::current_dir()
            .context("failed to resolve current working directory")?;
        let project_root = ProjectConfig::find_project_root(&cwd);

        let (root, config) = Self::resolve_config(&cwd, project_root.as_ref(), core_override)?;
        let ecosystem = Ecosystem::from_str(&config.ecosystem)
            .ok_or_else(|| anyhow::anyhow!("Unknown ecosystem: '{}'", config.ecosystem))?;

        let adapter = crate::factory::create_adapter(&ecosystem)?;
        Ok(Self {
            root,
            config,
            adapter,
        })
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
}
