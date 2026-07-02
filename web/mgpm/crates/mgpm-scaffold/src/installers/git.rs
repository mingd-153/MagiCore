use crate::error::ScaffoldError;
use crate::installers::{write_file, InstallResult, Installer};
use crate::ScaffoldContext;
use std::path::Path;

pub struct GitInstaller;

impl Installer for GitInstaller {
    fn name(&self) -> &str {
        "git"
    }

    fn description(&self) -> &str {
        "Git configuration (.gitignore, .gitattributes)"
    }

    fn install(
        &self,
        _ctx: &ScaffoldContext,
        project_dir: &Path,
    ) -> Result<InstallResult, ScaffoldError> {
        let gitignore = r#"node_modules
dist
.mgpm
.env
.env.local
.env.*.local
*.log
.DS_Store
coverage
"#;

        let gitattributes = r#"* text=auto eol=lf
*.js text eol=lf
*.ts text eol=lf
*.json text eol=lf
*.yml text eol=lf
*.yaml text eol=lf
*.md text eol=lf
"#;

        let files = vec![
            write_file(project_dir, ".gitignore", gitignore)?,
            write_file(project_dir, ".gitattributes", gitattributes)?,
        ];

        Ok(InstallResult {
            installer_name: "git".to_string(),
            files_created: files,
            dependencies_added: vec![],
        })
    }
}
