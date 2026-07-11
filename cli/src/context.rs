use std::path::PathBuf;

use mg_config::project::ProjectConfig;
use mg_types::adapter::PackageAdapter;
use mg_types::Ecosystem;

/// Project context: detects the project, loads config, provides the right adapter.
pub struct ProjectContext {
    pub root: PathBuf,
    pub config: ProjectConfig,
    pub adapter: Box<dyn PackageAdapter>,
}

impl ProjectContext {
    /// Load context, optionally with an explicit `--core` override.
    /// Priority:
    ///   1. `--core` flag (user says what they want)
    ///   2. `.megagate/project.toml` (saved by `mg init`)
    ///   3. auto_detect (package.json → web, Cargo.toml → lib, pyproject.toml → ai)
    ///   4. Single-core build default (only 1 core available — use it)
    pub fn load_with_core(core_override: Option<&str>) -> anyhow::Result<Self> {
        let cwd = std::env::current_dir()?;
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
        // Case 1: Project root found (has .megagate/ or package.json)
        if let Some(root) = project_root {
            // Try .megagate/project.toml first
            if let Some(mut cfg) = ProjectConfig::load(root)? {
                // --core overrides the saved ecosystem
                if let Some(core) = core_override {
                    cfg.ecosystem = core.to_string();
                }
                return Ok((root.clone(), cfg));
            }

            // Try auto_detect
            if let Some(eco) = ProjectConfig::auto_detect(root) {
                let eco = core_override.unwrap_or(&eco);
                let name = Self::dir_name(root);
                return Ok((root.clone(), ProjectConfig::new(name, eco)));
            }

            // Has project root but nothing detected
            if let Some(core) = core_override {
                let name = Self::dir_name(root);
                return Ok((root.clone(), ProjectConfig::new(name, core)));
            }

            // Single-core fallback
            if let Some(core) = Self::single_core_default() {
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

        // Case 2: No project root at all
        if let Some(core) = core_override {
            return Ok((cwd.to_path_buf(), ProjectConfig::new("project", core)));
        }

        // Single-core build: auto-default
        if let Some(core) = Self::single_core_default() {
            return Ok((cwd.to_path_buf(), ProjectConfig::new("project", core)));
        }

        anyhow::bail!(
            "No MegaGate project found.\n\
             Run '{}' to create one, or specify {}.\n\
             Tip: if you installed a single-core build, you can omit --core.",
            mg_ui::style_cmd("mg init"),
            mg_ui::style_cmd("--core <type>"),
        );
    }

    fn single_core_default() -> Option<String> {
        let avail = crate::factory::available_cores();
        if avail.len() == 1 {
            Some(avail[0].0.to_string())
        } else {
            None
        }
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
