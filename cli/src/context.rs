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
    /// Priority (T9a — chữ kí chống nhầm core):
    ///   1. `--core` flag
    ///   2. `.mg.core` marker (signature file)
    ///   3. `mg.toml` (saved by `mg init` — marker đồng bộ ecosystem)
    ///   4. auto_detect signature files (package.json → web, Cargo.toml → lib,
    ///      pyproject.toml → ai) — ambiguous → Err, không đoán (RULE §9.3)
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

        // Registry chain hợp nhất (ITEM 2): env override → mg.toml [[registries]]
        // (priority) → .npmrc registry= → npmjs default. entry 0 = primary.
        let chain = mg_config::chain::registry_chain(Some(&root), Some(&config));
        let primary = chain
            .first()
            .map(|r| r.url.clone())
            .ok_or_else(|| anyhow::anyhow!("no registry configured"))?;
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
            // T9a: marker là chữ kí có quyền cao nhất (sau --core). Marker sai
            // (core không hợp lệ) → Err fail-closed, không đoán core khác.
            let marker_core = ProjectConfig::read_core_marker(root)?;

            if let Some(mut cfg) = ProjectConfig::load(root)? {
                if let Some(core) = core_override {
                    cfg.ecosystem = core.to_string();
                } else if let Some(marker) = &marker_core {
                    // Marker sửa tay đổi core → marker thắng (chữ kí dev chủ động).
                    if cfg.ecosystem != marker[..] {
                        cfg.ecosystem = marker.clone();
                    }
                }
                return Ok((root.clone(), cfg));
            }

            if let Some(eco) = ProjectConfig::detect_core(root)? {
                let eco = core_override.unwrap_or(&eco);
                // T9a: tự ghi marker khi vừa detect từ signature — lần sau
                // không còn phụ thuộc thứ tự file (cảnh báo rõ ràng).
                if marker_core.is_none() && core_override.is_none() {
                    ProjectConfig::write_core_marker_at(root, eco)?;
                    mg_ui::warning(&format!(
                        "No '{}' found — auto-marking project as core '{}'. Edit the file to change core.",
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

            anyhow::bail!(
                "Cannot detect project type in '{}'. Run {}, {}, or specify {}",
                root.display(),
                mg_ui::style_cmd("mg init --template <type>"),
                mg_ui::style_cmd(&format!(
                    "mg init --signature <core> ({} more signatures) — write {} to pin it",
                    ProjectConfig::KNOWN_CORES.len(),
                    ProjectConfig::CORE_MARKER_FILE,
                )),
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
