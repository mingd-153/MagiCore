use crate::error::ScaffoldError;
use crate::installers::{write_file, InstallResult, Installer};
use crate::ScaffoldContext;
use std::path::Path;

pub struct CIInstaller;

impl Installer for CIInstaller {
    fn name(&self) -> &str {
        "ci"
    }

    fn description(&self) -> &str {
        "GitHub Actions CI workflow"
    }

    fn install(
        &self,
        _ctx: &ScaffoldContext,
        project_dir: &Path,
    ) -> Result<InstallResult, ScaffoldError> {
        let ci = r#"name: CI

on:
  push:
    branches: [main, development]
  pull_request:
    branches: [main]

jobs:
  quality:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        node-version: [18, 20, 22]

    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-node@v4
        with:
          node-version: ${{ matrix.node-version }}

      - name: Cache mg store
        uses: actions/cache@v4
        with:
          path: ~/.mg
          key: ${{ runner.os }}-mg-${{ hashFiles('**/package.json') }}
          restore-keys: |
            ${{ runner.os }}-mg-

      - run: mg install
      - run: mg lint
      - run: mg typecheck
      - run: mg test
      - run: mg build
"#;

        let files = vec![write_file(project_dir, ".github/workflows/ci.yml", ci)?];

        Ok(InstallResult {
            installer_name: "ci".to_string(),
            files_created: files,
            dependencies_added: vec![],
        })
    }
}
