use crate::error::ScaffoldError;
use crate::installers::{write_file, InstallResult, Installer};
use crate::ScaffoldContext;
use std::path::Path;

pub struct TailwindInstaller;

impl Installer for TailwindInstaller {
    fn name(&self) -> &str {
        "tailwind"
    }

    fn description(&self) -> &str {
        "Tailwind CSS utility framework"
    }

    fn dev_dependencies(&self) -> Vec<(&str, &str)> {
        vec![("tailwindcss", "^4.0.0")]
    }

    fn install(
        &self,
        ctx: &ScaffoldContext,
        project_dir: &Path,
    ) -> Result<InstallResult, ScaffoldError> {
        let mut files = vec![];

        let content_paths = match ctx.framework.as_deref() {
            Some("next") => r#" "./pages/**/*.{ts,tsx}", "./components/**/*.{ts,tsx}""#,
            _ => r#" "./src/**/*.{ts,tsx}""#,
        };

        let tailwind_config = format!(
            "import type {{ Config }} from \"tailwindcss\";\n\
             \n\
             const config: Config = {{\n\
             content: [{content_paths}\n\
             ],\n\
             theme: {{\n\
             extend: {{}},\n\
             }},\n\
             plugins: [],\n\
             }};\n\
             \n\
             export default config;\n"
        );

        let postcss_config = r#"export default {
  plugins: {
    tailwindcss: {},
    autoprefixer: {},
  },
};
"#;

        files.push(write_file(
            project_dir,
            "tailwind.config.ts",
            &tailwind_config,
        )?);
        files.push(write_file(
            project_dir,
            "postcss.config.mjs",
            postcss_config,
        )?);

        let css_dir = match ctx.framework.as_deref() {
            Some("next") => "app",
            _ => "src",
        };
        let globals_path = format!("{}/globals.css", css_dir);
        let globals = r#"@tailwind base;
@tailwind components;
@tailwind utilities;
"#;
        files.push(write_file(project_dir, &globals_path, globals)?);

        Ok(InstallResult {
            installer_name: "tailwind".to_string(),
            files_created: files,
            dependencies_added: vec!["tailwindcss".to_string()],
        })
    }
}
