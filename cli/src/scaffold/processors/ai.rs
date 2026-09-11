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

        // The scaffold ships its OWN tests (P0 finding, 2026-09-12):
        // `mgc test` auto-detects pytest via pyproject.toml — a
        // template without tests makes every honest lifecycle run fail
        // with pytest exit 5 (nothing collected), and CI lanes used to
        // paper over it by printf-ing tests into the project at run
        // time. The template carries the test; nobody fabricates it.
        // Template TỰ mang test (P0 finding, 2026-09-12): `mgc test`
        // auto-detect pytest qua pyproject.toml — template thiếu test
        // khiến mọi lần chạy lifecycle trung thực fail với pytest exit
        // 5 (không có gì để chạy), và lane CI trước đây phải độn test
        // vào project lúc chạy. Template mang test; không ai bịa test.
        if framework == "mcp-server" {
            write_file(
                &target.join("tests").join("test_server.py"),
                r#"'''Scaffold smoke test: the MCP server entry point imports.'''

import importlib.util
import pathlib
import sys

SRC = pathlib.Path(__file__).resolve().parents[1] / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))


def test_server_module_imports() -> None:
    path = SRC / "server.py"
    spec = importlib.util.spec_from_file_location("server", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    assert hasattr(module, "main")
"#,
            )?;
        } else {
            write_file(
                &target.join("tests").join("test_agent.py"),
                r#"'''Scaffold smoke test: the agent runs end-to-end.'''

import importlib.util
import pathlib
import sys

SRC = pathlib.Path(__file__).resolve().parents[1] / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))


def _load_agent():
    path = SRC / "agent.py"
    spec = importlib.util.spec_from_file_location("agent", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_agent_runs() -> None:
    agent = _load_agent().AIAgent()
    assert agent.run("CI") == "Agent processed successfully!"


def test_runtime_config_inert() -> None:
    agent = _load_agent().AIAgent()
    assert agent.runtime.target_bits is None
"#,
            )?;
        }

        write_file(
            &target.join("models").join("README.md"),
            "# AI Models Directory\n\nManage model artifacts with `mgc model pull hf://org/repo`.\n",
        )?;

        Ok(())
    }
}
