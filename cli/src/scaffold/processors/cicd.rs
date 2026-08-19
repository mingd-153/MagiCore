//! CICD scaffold: argocd/cloudflare/github-actions templates.

use std::path::Path;

use anyhow::Result;

use super::write_file;

pub struct CicdProcessor;

impl CicdProcessor {
    pub fn files(target: &Path, _name: &str, framework: &str) -> Result<()> {
        match framework {
            "argocd" => write_file(
                &target.join("argocd").join("application.yaml"),
                "apiVersion: argoproj.io/v1alpha1\nkind: Application\nmetadata:\n  name: megagate-app\nspec: {}\n",
            )?,
            "cloudflare" => {
                write_file(
                    &target.join("wrangler.toml"),
                    "name = \"worker\"\nmain = \"src/index.js\"\ncompatibility_date = \"2026-01-01\"\n",
                )?;
                write_file(
                    &target.join("src").join("index.js"),
                    "export default {\n  async fetch(request) {\n    return new Response(\"Hello from MegaGate Worker\", { status: 200 });\n  },\n};\n",
                )?;
            }
            _ => write_file(
                &target.join(".github").join("workflows").join("ci.yml"),
                "name: CI\n\non:\n  push:\n  pull_request:\n\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - run: echo \"MegaGate CI scaffold\"\n",
            )?,
        }

        Ok(())
    }

}
