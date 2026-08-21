use anyhow::{bail, Result};
use std::env;
use std::path::{Path, PathBuf};

/// A template directory on disk (env → workspace templates/ → registry cache).
#[derive(Debug, Clone)]
pub struct TemplateRoot {
    path: PathBuf,
}

impl TemplateRoot {
    pub fn disk(path: PathBuf) -> TemplateRoot {
        TemplateRoot { path }
    }

    /// Resolve a template rel (relative to templates/) against the source
    /// priority: MEGAGATE_TEMPLATE_DIR env → workspace disk → registry cache.
    pub fn resolve(rel: &str) -> TemplateRoot {
        if let Ok(dir) = env::var("MEGAGATE_TEMPLATE_DIR") {
            let candidate = PathBuf::from(dir).join(rel);
            if candidate.is_dir() {
                return TemplateRoot::disk(candidate);
            }
        }

        let disk = workspace_root().join("templates").join(rel);
        if disk.is_dir() && has_template_contract(&disk) {
            return TemplateRoot::disk(disk);
        }

        // Registry cache fallback: ~/.mg/templates/{rel} (mg template fetch).
        let cached = crate::commands::template::templates_cache_dir().join(rel);
        if cached.is_dir() {
            return TemplateRoot::disk(cached);
        }

        // Fallthrough to workspace disk bails with a clear error on first access.
        TemplateRoot::disk(disk)
    }

    /// Join a sub-path inside this root.
    pub fn join(&self, rel: &str) -> TemplateRoot {
        TemplateRoot::disk(self.path.join(rel))
    }

    pub fn exists(&self, rel: &str) -> bool {
        self.path.join(rel).exists()
    }

    /// Raw filesystem path of this root (publish source resolution).
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read(&self, rel: &str) -> Result<Vec<u8>> {
        let full = self.path.join(rel);
        if !full.is_file() {
            bail!("Template path '{}' does not exist", full.display());
        }
        Ok(std::fs::read(&full)?)
    }

    /// Human-readable display for a path inside this root (error messages).
    pub fn label(&self, rel: &str) -> String {
        self.path.join(rel).display().to_string()
    }

    /// Workspace-relative logical location ("templates/web/...") used as the
    /// `{{ template }}` context value.
    pub fn logical_rel(&self) -> String {
        match self.path.strip_prefix(workspace_root().join("templates")) {
            Ok(rel) => format!("templates/{}", rel.display()),
            Err(_) => self.path.display().to_string(),
        }
    }
}

pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

/// True when a template dir holds real template content (a `template.toml`
/// contract anywhere below). Workspace trees holding only doc placeholders
/// (README files, e.g. the scaffold-structure docs) must not shadow the
/// registry cache.
fn has_template_contract(dir: &Path) -> bool {
    if dir.join("template.toml").is_file() {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && has_template_contract(&path) {
            return true;
        }
    }
    false
}
