//! AI scaffold for Python agents and MCP servers.
//! Scaffold AI tối thiểu, chạy được và không tuyên bố khả năng chưa được kiểm chứng.

use super::{slugify, write_file};
use anyhow::Result;
use std::path::Path;

pub struct AiProcessor;

impl AiProcessor {
    pub fn files(target: &Path, name: &str, framework: &str) -> Result<()> {
        let package = slugify(name).replace('-', "_");
        let slug = slugify(name);

        write_file(
            &target.join("pyproject.toml"),
            &format!(
                "[build-system]\n\
                 requires = [\"setuptools>=80\"]\n\
                 build-backend = \"setuptools.build_meta\"\n\n\
                 [project]\n\
                 name = \"{slug}\"\n\
                 version = \"0.1.0\"\n\
                 description = \"MagiCore AI Project\"\n\
                 requires-python = \">=3.11\"\n\
                 dependencies = []\n\n\
                 [project.optional-dependencies]\n\
                 model = [\n\
                     \"torch>=2.4.0\",\n\
                     \"transformers>=4.44.0\",\n\
                     \"accelerate>=0.33.0\",\n\
                     \"safetensors>=0.4.4\",\n\
                 ]\n\n\
                 [tool.magicore]\n\
                 framework = \"{framework}\"\n\
                 core = \"ai\"\n\n\
                 [tool.setuptools]\n\
                 py-modules = [\"agent\", \"compression\"]\n\
                 package-dir = {{\"\" = \"src\"}}\n"
            ),
        )?;

        // Defaults are inert until a runtime explicitly consumes them.
        // Giá trị mặc định không tuyên bố tối ưu khi chưa có runtime áp dụng.
        write_file(
            &target.join("configs").join("model_config.json"),
            &format!(
                "{{\n\
                  \"model_name\": \"{slug}\",\n\
                  \"quantization\": null,\n\
                  \"sharding\": {{\n\
                    \"enabled\": false,\n\
                    \"max_memory_per_gpu\": null,\n\
                    \"offload_to_cpu\": false\n\
                  }}\n\
                }}\n"
            ),
        )?;

        write_file(
            &target.join("src").join("compression.py"),
            "\"\"\"Runtime configuration for optional model optimization.\"\"\"\n\nfrom typing import Optional\n\n\nclass ModelRuntimeConfig:\n    \"\"\"Describe requested optimization without mutating a model.\"\"\"\n\n    def __init__(self, target_bits: Optional[int] = None):\n        self.target_bits = target_bits\n",
        )?;

        if framework == "mcp-server" {
            write_file(
                &target.join("src").join("server.py"),
                &format!(
                    r#"'''MagiCore MCP server entry point.'''

from compression import ModelRuntimeConfig


def main() -> None:
    config = ModelRuntimeConfig()
    print("MagiCore MCP Server ({package}) initialized!", config.target_bits)


if __name__ == "__main__":
    main()
"#
                ),
            )?;
        } else {
            write_file(
                &target.join("src").join("agent.py"),
                r#"'''MagiCore AI agent entry point.'''

from compression import ModelRuntimeConfig


class AIAgent:
    def __init__(self) -> None:
        self.runtime = ModelRuntimeConfig()

    def run(self, prompt: str) -> str:
        print(f"Executing agent with prompt: {prompt[:50]}...")
        return "Agent processed successfully!"


if __name__ == "__main__":
    agent = AIAgent()
    print(agent.run("Hello MagiCore AI"))
"#,
            )?;
        }

        let entrypoint = if framework == "mcp-server" {
            "src/server.py"
        } else {
            "src/agent.py"
        };
        write_file(
            &target.join("scripts").join("run_dev.sh"),
            &format!(
                "#!/usr/bin/env bash\nset -euo pipefail\nif [[ -f .mgc-optimizer/ai_runtime.env ]]; then\n  source .mgc-optimizer/ai_runtime.env\nfi\npython3 {entrypoint}\n"
            ),
        )?;

        write_file(
            &target.join("models").join("README.md"),
            "# AI Models Directory\n\nManage model artifacts with `mgc model pull hf://org/repo`.\n",
        )?;

        Ok(())
    }
}
