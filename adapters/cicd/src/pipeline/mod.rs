//! CI/CD pipeline generation.

use mgc_types::MgResult;
use std::path::Path;

pub async fn generate_pipeline(name: &str, dir: &Path) -> MgResult<()> {
    std::fs::create_dir_all(dir.join(".github/workflows"))?;

    let workflow = format!(
        r#"name: {}
on: [push]
jobs:
  verify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: mgc verify
"#,
        name
    );

    std::fs::write(dir.join(".github/workflows/ci.yml"), workflow)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_generate_pipeline() {
        let tmp = TempDir::new().unwrap();
        generate_pipeline("test", tmp.path()).await.unwrap();
        assert!(tmp.path().join(".github/workflows/ci.yml").exists());
    }
}
