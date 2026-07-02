use crate::error::ScaffoldError;
use crate::installers::{write_file, InstallResult, Installer};
use crate::ScaffoldContext;
use std::path::Path;

pub struct EslintInstaller;

impl Installer for EslintInstaller {
    fn name(&self) -> &str {
        "eslint"
    }

    fn description(&self) -> &str {
        "ESLint with flat config"
    }

    fn dev_dependencies(&self) -> Vec<(&str, &str)> {
        vec![
            ("eslint", "^9.0.0"),
            ("@eslint/js", "^9.0.0"),
            ("typescript-eslint", "^8.0.0"),
            ("eslint-plugin-react-hooks", "^5.0.0"),
        ]
    }

    fn install(
        &self,
        _ctx: &ScaffoldContext,
        project_dir: &Path,
    ) -> Result<InstallResult, ScaffoldError> {
        let config = r#"import eslint from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    plugins: {
      "react-hooks": reactHooks,
    },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
      "@typescript-eslint/no-unused-vars": ["warn", { argsIgnorePattern: "^_" }],
      "@typescript-eslint/no-explicit-any": "warn",
    },
  },
  {
    ignores: ["dist/**", "node_modules/**", ".mgpm/**"],
  },
);
"#;

        let files = vec![write_file(project_dir, "eslint.config.mjs", config)?];

        Ok(InstallResult {
            installer_name: "eslint".to_string(),
            files_created: files,
            dependencies_added: vec![
                "eslint".to_string(),
                "@eslint/js".to_string(),
                "typescript-eslint".to_string(),
                "eslint-plugin-react-hooks".to_string(),
            ],
        })
    }

    fn supports(&self, framework: &str) -> bool {
        matches!(framework, "react" | "next" | "vue" | "vanilla")
    }
}
