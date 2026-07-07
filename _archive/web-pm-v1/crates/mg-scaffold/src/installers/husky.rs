use crate::error::ScaffoldError;
use crate::installers::{write_file, InstallResult, Installer};
use crate::ScaffoldContext;
use std::path::Path;

pub struct HuskyInstaller;

impl Installer for HuskyInstaller {
    fn name(&self) -> &str {
        "husky"
    }

    fn description(&self) -> &str {
        "Husky + lint-staged"
    }

    fn dev_dependencies(&self) -> Vec<(&str, &str)> {
        vec![("husky", "^9.1.0"), ("lint-staged", "^15.3.0")]
    }

    fn install(
        &self,
        _ctx: &ScaffoldContext,
        project_dir: &Path,
    ) -> Result<InstallResult, ScaffoldError> {
        let pre_commit = r#"mg exec lint-staged
"#;

        let lintstagedrc = r#"export default {
  "*.{ts,tsx,js,jsx}": ["eslint --fix", "prettier --write"],
  "*.{json,md,yaml,yml}": ["prettier --write"],
};
"#;

        let files = vec![
            write_file(project_dir, ".husky/pre-commit", pre_commit)?,
            write_file(project_dir, ".lintstagedrc.mjs", lintstagedrc)?,
        ];

        Ok(InstallResult {
            installer_name: "husky".to_string(),
            files_created: files,
            dependencies_added: vec!["husky".to_string(), "lint-staged".to_string()],
        })
    }
}
