use crate::error::ScaffoldError;
use crate::installers::{write_file, InstallResult, Installer};
use crate::ScaffoldContext;
use std::path::Path;

pub struct VitestInstaller;

impl Installer for VitestInstaller {
    fn name(&self) -> &str {
        "vitest"
    }

    fn description(&self) -> &str {
        "Vitest test framework"
    }

    fn dev_dependencies(&self) -> Vec<(&str, &str)> {
        vec![("vitest", "^3.0.0")]
    }

    fn install(
        &self,
        _ctx: &ScaffoldContext,
        project_dir: &Path,
    ) -> Result<InstallResult, ScaffoldError> {
        let vitest_config = r#"import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    globals: true,
    environment: "node",
    include: ["src/**/*.test.ts", "src/**/*.spec.ts"],
    coverage: {
      provider: "v8",
      reporter: ["text", "json", "html"],
      include: ["src/**/*.ts"],
      exclude: ["src/**/*.test.ts", "src/**/*.spec.ts"],
    },
  },
});
"#;

        let example_test = r#"import { describe, it, expect } from "vitest";

describe("example", () => {
  it("should pass", () => {
    expect(1 + 1).toBe(2);
  });
});
"#;

        let files = vec![
            write_file(project_dir, "vitest.config.ts", vitest_config)?,
            write_file(project_dir, "src/test/setup.ts", "")?,
            write_file(project_dir, "src/test/example.test.ts", example_test)?,
        ];

        Ok(InstallResult {
            installer_name: "vitest".to_string(),
            files_created: files,
            dependencies_added: vec!["vitest".to_string()],
        })
    }
}
