use crate::error::ScaffoldError;
use crate::installers::{write_file, InstallResult, Installer};
use crate::ScaffoldContext;
use std::path::Path;

pub struct TailwindInstaller;

impl Installer for TailwindInstaller {
    fn name(&self) -> &str {
        "tailwindcss"
    }

    fn description(&self) -> &str {
        "Tailwind CSS v4 utility framework"
    }

    fn dev_dependencies(&self) -> Vec<(&str, &str)> {
        vec![("tailwindcss", "^4.0.0"), ("@tailwindcss/vite", "^4.0.0")]
    }

    fn install(
        &self,
        ctx: &ScaffoldContext,
        project_dir: &Path,
    ) -> Result<InstallResult, ScaffoldError> {
        let mut files = vec![];

        // Tailwind v4: no tailwind.config.js, no postcss.config.js
        // Just create the globals.css with @import

        let css_dir = match ctx.framework.as_deref() {
            Some("next") => "app",
            _ => "src",
        };
        let globals_path = format!("{}/globals.css", css_dir);
        let globals = "@import \"tailwindcss\";\n";
        files.push(write_file(project_dir, &globals_path, globals)?);

        Ok(InstallResult {
            installer_name: "tailwindcss".to_string(),
            files_created: files,
            dependencies_added: vec!["tailwindcss".to_string(), "@tailwindcss/vite".to_string()],
        })
    }
}
