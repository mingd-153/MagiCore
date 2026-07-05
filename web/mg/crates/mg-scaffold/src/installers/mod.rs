mod ci;
mod docker;
mod eslint;
mod git;
mod husky;
mod prettier;
mod tailwind;
mod typescript;
mod vitest;

pub use ci::CIInstaller;
pub use docker::DockerInstaller;
pub use eslint::EslintInstaller;
pub use git::GitInstaller;
pub use husky::HuskyInstaller;
pub use prettier::PrettierInstaller;
pub use tailwind::TailwindInstaller;
pub use typescript::TypeScriptInstaller;
pub use vitest::VitestInstaller;

use crate::error::ScaffoldError;
use crate::ScaffoldContext;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct InstallResult {
    pub installer_name: String,
    pub files_created: Vec<PathBuf>,
    pub dependencies_added: Vec<String>,
}

pub trait Installer: Send + Sync {
    fn name(&self) -> &str;

    fn description(&self) -> &str;

    fn dependencies(&self) -> Vec<(&str, &str)> {
        vec![]
    }

    fn dev_dependencies(&self) -> Vec<(&str, &str)> {
        vec![]
    }

    fn install(
        &self,
        ctx: &ScaffoldContext,
        project_dir: &Path,
    ) -> Result<InstallResult, ScaffoldError>;

    fn supports(&self, framework: &str) -> bool {
        let _ = framework;
        true
    }
}

pub struct InstallerRegistry {
    installers: Vec<Box<dyn Installer>>,
}

impl InstallerRegistry {
    pub fn new() -> Self {
        Self { installers: vec![] }
    }

    pub fn register(&mut self, installer: Box<dyn Installer>) {
        self.installers.push(installer);
    }

    pub fn register_defaults(&mut self) {
        self.register(Box::new(TypeScriptInstaller));
        self.register(Box::new(TailwindInstaller));
        self.register(Box::new(EslintInstaller));
        self.register(Box::new(PrettierInstaller));
        self.register(Box::new(VitestInstaller));
        self.register(Box::new(DockerInstaller));
        self.register(Box::new(CIInstaller));
        self.register(Box::new(GitInstaller));
        self.register(Box::new(HuskyInstaller));
    }

    pub fn list(&self) -> &[Box<dyn Installer>] {
        &self.installers
    }

    pub fn get(&self, name: &str) -> Option<&dyn Installer> {
        self.installers
            .iter()
            .find(|i| i.name() == name)
            .map(|i| i.as_ref())
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Box<dyn Installer>> {
        self.installers.iter_mut().find(|i| i.name() == name)
    }

    pub fn install_selected(
        &self,
        names: &[String],
        ctx: &ScaffoldContext,
        project_dir: &Path,
    ) -> Result<Vec<InstallResult>, ScaffoldError> {
        let mut results = vec![];
        for name in names {
            match self.get(name) {
                Some(installer) => {
                    results.push(installer.install(ctx, project_dir)?);
                }
                None => {
                    eprintln!("[warn] Installer '{}' not found — skipping", name);
                }
            }
        }
        Ok(results)
    }

    pub fn install_all(
        &self,
        ctx: &ScaffoldContext,
        project_dir: &Path,
    ) -> Result<Vec<InstallResult>, ScaffoldError> {
        let names: Vec<String> = self
            .installers
            .iter()
            .map(|i| i.name().to_string())
            .collect();
        self.install_selected(&names, ctx, project_dir)
    }
}

impl Default for InstallerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::ScaffoldContext;
    use std::path::PathBuf;

    fn test_ctx() -> ScaffoldContext {
        ScaffoldContext::new("test-app", PathBuf::from("/tmp/test"))
    }

    #[test]
    fn test_typescript_installer() {
        let dir = tempfile::tempdir().unwrap();
        let installer = TypeScriptInstaller;
        let result = installer.install(&test_ctx(), dir.path()).unwrap();
        assert_eq!(result.installer_name, "typescript");
        assert!(dir.path().join("tsconfig.json").exists());
        assert!(dir.path().join("tsconfig.node.json").exists());
        assert_eq!(result.files_created.len(), 2);
    }

    #[test]
    fn test_tailwind_installer() {
        let dir = tempfile::tempdir().unwrap();
        let installer = TailwindInstaller;
        let ctx = test_ctx();
        let result = installer.install(&ctx, dir.path()).unwrap();
        assert_eq!(result.installer_name, "tailwindcss");
        // Tailwind v4: no config files, just globals.css
        assert!(!dir.path().join("tailwind.config.ts").exists());
        assert!(!dir.path().join("postcss.config.mjs").exists());
        assert!(dir.path().join("src/globals.css").exists());
        let css = std::fs::read_to_string(dir.path().join("src/globals.css")).unwrap();
        assert!(css.contains("@import \"tailwindcss\""));
    }

    #[test]
    fn test_tailwind_installer_nextjs() {
        let dir = tempfile::tempdir().unwrap();
        let installer = TailwindInstaller;
        let ctx = ScaffoldContext::new("app", PathBuf::from("/tmp/a")).with_framework("next");
        installer.install(&ctx, dir.path()).unwrap();
        assert!(dir.path().join("app/globals.css").exists());
    }

    #[test]
    fn test_eslint_installer() {
        let dir = tempfile::tempdir().unwrap();
        let installer = EslintInstaller;
        let result = installer.install(&test_ctx(), dir.path()).unwrap();
        assert_eq!(result.installer_name, "eslint");
        assert!(dir.path().join("eslint.config.mjs").exists());
        assert!(installer.supports("react"));
        assert!(installer.supports("next"));
        assert!(!installer.supports("angular"));
    }

    #[test]
    fn test_prettier_installer() {
        let dir = tempfile::tempdir().unwrap();
        let installer = PrettierInstaller;
        let result = installer.install(&test_ctx(), dir.path()).unwrap();
        assert_eq!(result.installer_name, "prettier");
        assert!(dir.path().join(".prettierrc").exists());
        assert!(dir.path().join(".prettierignore").exists());
    }

    #[test]
    fn test_vitest_installer() {
        let dir = tempfile::tempdir().unwrap();
        let installer = VitestInstaller;
        let result = installer.install(&test_ctx(), dir.path()).unwrap();
        assert_eq!(result.installer_name, "vitest");
        assert!(dir.path().join("vitest.config.ts").exists());
        assert!(dir.path().join("src/test/example.test.ts").exists());
    }

    #[test]
    fn test_docker_installer() {
        let dir = tempfile::tempdir().unwrap();
        let installer = DockerInstaller;
        let result = installer.install(&test_ctx(), dir.path()).unwrap();
        assert_eq!(result.installer_name, "docker");
        assert!(dir.path().join("Dockerfile").exists());
        assert!(dir.path().join(".dockerignore").exists());
        assert!(dir.path().join("docker-compose.yml").exists());
    }

    #[test]
    fn test_ci_installer() {
        let dir = tempfile::tempdir().unwrap();
        let installer = CIInstaller;
        let result = installer.install(&test_ctx(), dir.path()).unwrap();
        assert_eq!(result.installer_name, "ci");
        assert!(dir.path().join(".github/workflows/ci.yml").exists());
    }

    #[test]
    fn test_git_installer() {
        let dir = tempfile::tempdir().unwrap();
        let installer = GitInstaller;
        let result = installer.install(&test_ctx(), dir.path()).unwrap();
        assert_eq!(result.installer_name, "git");
        assert!(dir.path().join(".gitignore").exists());
        assert!(dir.path().join(".gitattributes").exists());
    }

    #[test]
    fn test_husky_installer() {
        let dir = tempfile::tempdir().unwrap();
        let installer = HuskyInstaller;
        let result = installer.install(&test_ctx(), dir.path()).unwrap();
        assert_eq!(result.installer_name, "husky");
        assert!(dir.path().join(".husky/pre-commit").exists());
        assert!(dir.path().join(".lintstagedrc.mjs").exists());
    }

    #[test]
    fn test_installer_registry() {
        let mut registry = InstallerRegistry::new();
        assert!(registry.list().is_empty());

        registry.register(Box::new(TypeScriptInstaller));
        registry.register(Box::new(TailwindInstaller));
        assert_eq!(registry.list().len(), 2);

        let ts = registry.get("typescript");
        assert!(ts.is_some());
        assert_eq!(ts.unwrap().name(), "typescript");

        let missing = registry.get("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_register_defaults() {
        let mut registry = InstallerRegistry::new();
        registry.register_defaults();
        assert_eq!(registry.list().len(), 9);
        assert!(registry.get("typescript").is_some());
        assert!(registry.get("husky").is_some());
    }

    #[test]
    fn test_install_selected() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = InstallerRegistry::new();
        registry.register_defaults();

        let names = vec!["typescript".to_string(), "git".to_string()];
        let results = registry
            .install_selected(&names, &test_ctx(), dir.path())
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(dir.path().join("tsconfig.json").exists());
        assert!(dir.path().join(".gitignore").exists());
    }

    #[test]
    fn test_empty_selection() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = InstallerRegistry::new();
        registry.register_defaults();

        let results = registry
            .install_selected(&[], &test_ctx(), dir.path())
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_all_installers() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = InstallerRegistry::new();
        registry.register_defaults();

        let results = registry.install_all(&test_ctx(), dir.path()).unwrap();
        assert_eq!(results.len(), 9);
        assert!(dir.path().join("tsconfig.json").exists());
        // Tailwind v4: no config files, just globals.css
        assert!(dir.path().join("src/globals.css").exists());
        assert!(dir.path().join("eslint.config.mjs").exists());
        assert!(dir.path().join(".prettierrc").exists());
        assert!(dir.path().join("vitest.config.ts").exists());
        assert!(dir.path().join("Dockerfile").exists());
        assert!(dir.path().join(".github/workflows/ci.yml").exists());
        assert!(dir.path().join(".gitignore").exists());
        assert!(dir.path().join(".husky/pre-commit").exists());
    }

    #[test]
    fn test_unknown_installer_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let registry = InstallerRegistry::new();
        let names = vec!["unknown".to_string()];
        let results = registry
            .install_selected(&names, &test_ctx(), dir.path())
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_installer_dependencies() {
        let ts = TypeScriptInstaller;
        let deps = ts.dev_dependencies();
        assert!(deps.iter().any(|(n, _)| *n == "typescript"));
    }
}

fn write_file(project_dir: &Path, relative: &str, content: &str) -> Result<PathBuf, ScaffoldError> {
    let path = project_dir.join(relative);
    if !path.starts_with(project_dir) {
        return Err(ScaffoldError::IoError {
            context: format!("path escape: {} resolves outside project dir", relative),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "path traversal"),
        });
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ScaffoldError::IoError {
            context: format!("Failed to create directory for {}", relative),
            source: e,
        })?;
    }
    std::fs::write(&path, content).map_err(|e| ScaffoldError::IoError {
        context: format!("Failed to write {}", relative),
        source: e,
    })?;
    Ok(path)
}
