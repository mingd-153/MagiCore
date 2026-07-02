use crate::error::ScaffoldError;
use crate::installers::{write_file, InstallResult, Installer};
use crate::ScaffoldContext;
use std::path::Path;

pub struct PrettierInstaller;

impl Installer for PrettierInstaller {
    fn name(&self) -> &str {
        "prettier"
    }

    fn description(&self) -> &str {
        "Prettier code formatter"
    }

    fn dev_dependencies(&self) -> Vec<(&str, &str)> {
        vec![("prettier", "^3.4.0")]
    }

    fn install(
        &self,
        _ctx: &ScaffoldContext,
        project_dir: &Path,
    ) -> Result<InstallResult, ScaffoldError> {
        let prettierrc = r#"{
  "semi": true,
  "singleQuote": false,
  "tabWidth": 2,
  "trailingComma": "all",
  "printWidth": 100,
  "bracketSpacing": true,
  "arrowParens": "always",
  "endOfLine": "lf"
}
"#;

        let prettierignore = r#"node_modules
dist
.mgpm
coverage
"#;

        let files = vec![
            write_file(project_dir, ".prettierrc", prettierrc)?,
            write_file(project_dir, ".prettierignore", prettierignore)?,
        ];

        Ok(InstallResult {
            installer_name: "prettier".to_string(),
            files_created: files,
            dependencies_added: vec!["prettier".to_string()],
        })
    }
}
