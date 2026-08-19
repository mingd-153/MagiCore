//! AI scaffold: python-agent/mcp-server templates.

use std::path::Path;

use anyhow::Result;

use super::{write_file, slugify};

pub struct AiProcessor;

impl AiProcessor {
    pub fn files(target: &Path, name: &str, framework: &str) -> Result<()> {
        let package = slugify(name).replace('-', "_");
        write_file(
            &target.join("pyproject.toml"),
            &format!(
                "[project]\nname = \"{}\"\nversion = \"0.1.0\"\ndescription = \"MegaGate AI project\"\nrequires-python = \">=3.11\"\n\n[tool.megagate]\nframework = \"{}\"\n",
                slugify(name),
                framework
            ),
        )?;

        if framework == "mcp-server" {
            write_file(
                &target.join("server.py"),
                "def main() -> None:\n    print(\"MegaGate MCP server scaffold\")\n\n\nif _name__ == \"__main__\":\n    main()\n",
            )?;
        } else {
            write_file(
                &target.join("src").join("agent.py"),
                &format!(
                    "def run() -> None:\n    print(\"{} agent ready\")\n\n\nif _name__ == \"__main__\":\n    run()\n",
                    package
                ),
            )?;
        }

        Ok(())
    }

}
