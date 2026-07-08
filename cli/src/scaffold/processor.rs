use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::wizard::engine::ScaffoldConfig;

pub struct Scaffolder;

impl Scaffolder {
    pub fn infer_web_create_config(framework: &str, project_name: &str) -> Result<ScaffoldConfig> {
        let framework = normalize_web_framework(framework);
        let (sub_type, frameworks) = match framework.as_str() {
            "vanilla" | "react-vite" | "vue-vite" | "nextjs" | "sveltekit" | "nuxt" | "angular"
            | "solidjs" | "qwik" | "astro" => ("frontend".to_string(), vec![framework.clone()]),
            "express" | "fastify" | "nestjs" | "hono" | "trpc" => (
                "backend".to_string(),
                vec!["node".to_string(), framework.clone()],
            ),
            "laravel" | "symfony" => (
                "backend".to_string(),
                vec!["php".to_string(), framework.clone()],
            ),
            "spring-boot" | "quarkus" => (
                "backend".to_string(),
                vec!["java".to_string(), framework.clone()],
            ),
            "gin" | "echo" | "fiber" => (
                "backend".to_string(),
                vec!["go".to_string(), framework.clone()],
            ),
            "fastapi" | "django" | "flask" => (
                "backend".to_string(),
                vec!["python".to_string(), framework.clone()],
            ),
            "axum" | "actix-web" => (
                "backend".to_string(),
                vec!["rust".to_string(), framework.clone()],
            ),
            "remix" | "react-fastify" | "vue-laravel" | "react-spring" | "custom" => {
                ("fullstack".to_string(), vec![framework.clone()])
            }
            other => anyhow::bail!("Unsupported web framework '{other}'"),
        };

        Ok(ScaffoldConfig {
            core: "web".to_string(),
            sub_type,
            frameworks,
            project_name: project_name.to_string(),
            features: vec![],
            template_dir: PathBuf::new(),
        })
    }

    pub fn scaffold(config: &ScaffoldConfig) -> Result<PathBuf> {
        let target = Self::target_dir(config);
        if target.exists() {
            anyhow::bail!("Directory '{}' already exists", target.display());
        }

        std::fs::create_dir_all(&target)?;
        Self::write_common_files(&target, config)?;
        Self::write_core_files(&target, config)?;

        mg_ui::success(&format!(
            "Created {} project: {}",
            config.core,
            target.display()
        ));
        Ok(target)
    }

    pub fn display_name(project_dir: &Path) -> String {
        project_dir
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".to_string())
    }

    fn target_dir(config: &ScaffoldConfig) -> PathBuf {
        PathBuf::from(&config.project_name)
    }

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
    }

    fn web_templates_root() -> PathBuf {
        Self::workspace_root().join("templates").join("web")
    }

    fn framework(config: &ScaffoldConfig) -> String {
        config
            .frameworks
            .last()
            .cloned()
            .unwrap_or_else(|| Self::default_framework(&config.core).to_string())
    }

    fn default_framework(core: &str) -> &'static str {
        match core {
            "web" => "vanilla",
            "game" => "bevy",
            "ai" => "python-agent",
            "clo" => "pulumi-aws",
            "cicd" => "github-actions",
            "iot" => "esp32-rust",
            "app" => "flutter",
            "lib" => "rust",
            _ => "generic",
        }
    }

    fn write_common_files(target: &Path, config: &ScaffoldConfig) -> Result<()> {
        let name = Self::display_name(target);
        let framework = Self::framework(config);

        Self::write_file(
            &target.join(".gitignore"),
            &common_gitignore(&config.core, &framework),
        )?;
        Self::write_file(
            &target.join("README.md"),
            &common_readme(&name, &config.core, &framework, &config.features),
        )?;

        Ok(())
    }

    fn write_core_files(target: &Path, config: &ScaffoldConfig) -> Result<()> {
        let name = Self::display_name(target);
        let framework = Self::framework(config);

        match config.core.as_str() {
            "web" => Self::write_web_files(target, config, &name, &framework),
            "game" => Self::write_game_files(target, &name, &framework),
            "ai" => Self::write_ai_files(target, &name, &framework),
            "clo" => Self::write_cloud_files(target, &name, &framework),
            "cicd" => Self::write_cicd_files(target, &name, &framework),
            "iot" => Self::write_iot_files(target, &name, &framework),
            "app" => Self::write_app_files(target, &name, &framework),
            "lib" => Self::write_lib_files(target, &name, &framework),
            other => anyhow::bail!("Unsupported core '{}'", other),
        }
    }

    fn write_web_files(
        target: &Path,
        config: &ScaffoldConfig,
        name: &str,
        framework: &str,
    ) -> Result<()> {
        let template_dir = Self::resolve_web_template_dir(config)?;
        Self::write_web_manifest(target, config, &template_dir)?;
        Self::materialize_web_skeleton(target, config)?;

        match config.sub_type.as_str() {
            "frontend" => Self::write_web_frontend_placeholder(target, name, framework),
            "backend" => Self::write_web_backend_placeholder(target, name, framework),
            "fullstack" => Self::write_web_fullstack_placeholder(target, name, framework),
            "monorepo" => Self::write_web_monorepo_placeholder(target, name, config),
            _ => Self::write_web_frontend_placeholder(target, name, framework),
        }
    }

    fn write_game_files(target: &Path, name: &str, framework: &str) -> Result<()> {
        match framework {
            "unity" => {
                Self::write_file(
                    &target.join("Packages").join("manifest.json"),
                    "{\n  \"dependencies\": {}\n}\n",
                )?;
                Self::write_file(
                    &target.join("Assets").join("Scripts").join("Bootstrap.cs"),
                    "using UnityEngine;\n\npublic class Bootstrap : MonoBehaviour\n{\n    void Start()\n    {\n        Debug.Log(\"MegaGate Unity project ready\");\n    }\n}\n",
                )?;
            }
            "godot" => {
                Self::write_file(
                    &target.join("project.godot"),
                    "[application]\nconfig/name=\"MegaGate Game\"\nrun/main_scene=\"res://Main.tscn\"\n",
                )?;
                Self::write_file(
                    &target.join("Main.tscn"),
                    "[gd_scene format=3]\n\n[node name=\"Main\" type=\"Node2D\"]\n",
                )?;
            }
            "unreal" => {
                Self::write_file(
                    &target.join(format!("{name}.uproject")),
                    &format!(
                        "{{\n  \"FileVersion\": 3,\n  \"EngineAssociation\": \"5.0\",\n  \"Category\": \"Games\",\n  \"Description\": \"{}\"\n}}\n",
                        name
                    ),
                )?;
            }
            _ => {
                Self::write_file(
                    &target.join("Cargo.toml"),
                    &format!(
                        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nbevy = \"0.14\"\n",
                        slugify(name)
                    ),
                )?;
                Self::write_file(
                    &target.join("src").join("main.rs"),
                    "fn main() {\n    println!(\"MegaGate Bevy game scaffold\");\n}\n",
                )?;
            }
        }

        Ok(())
    }

    fn write_ai_files(target: &Path, name: &str, framework: &str) -> Result<()> {
        let package = slugify(name).replace('-', "_");
        Self::write_file(
            &target.join("pyproject.toml"),
            &format!(
                "[project]\nname = \"{}\"\nversion = \"0.1.0\"\ndescription = \"MegaGate AI project\"\nrequires-python = \">=3.11\"\n\n[tool.megagate]\nframework = \"{}\"\n",
                slugify(name),
                framework
            ),
        )?;

        if framework == "mcp-server" {
            Self::write_file(
                &target.join("server.py"),
                "def main() -> None:\n    print(\"MegaGate MCP server scaffold\")\n\n\nif __name__ == \"__main__\":\n    main()\n",
            )?;
        } else {
            Self::write_file(
                &target.join("src").join("agent.py"),
                &format!(
                    "def run() -> None:\n    print(\"{} agent ready\")\n\n\nif __name__ == \"__main__\":\n    run()\n",
                    package
                ),
            )?;
        }

        Ok(())
    }

    fn write_cloud_files(target: &Path, name: &str, framework: &str) -> Result<()> {
        match framework {
            "terraform" | "terraform-gcp" => Self::write_file(
                &target.join("main.tf"),
                "terraform {\n  required_version = \">= 1.5.0\"\n}\n\nprovider \"google\" {}\n",
            )?,
            "cdk" | "cdk-typescript" => {
                Self::write_file(
                    &target.join("package.json"),
                    &format!(
                        "{{\n  \"name\": \"{}\",\n  \"private\": true,\n  \"version\": \"0.1.0\",\n  \"scripts\": {{\n    \"synth\": \"cdk synth\"\n  }}\n}}\n",
                        name
                    ),
                )?;
                Self::write_file(
                    &target.join("bin").join("app.ts"),
                    "console.log('MegaGate CDK app scaffold');\n",
                )?;
            }
            "cloudflare" => Self::write_file(
                &target.join("wrangler.toml"),
                &format!("name = \"{}\"\nmain = \"src/index.ts\"\n", slugify(name)),
            )?,
            "lambda" => Self::write_file(
                &target.join("handler.ts"),
                "export const handler = async () => ({ statusCode: 200, body: 'ok' });\n",
            )?,
            _ => {
                Self::write_file(
                    &target.join("Pulumi.yaml"),
                    &format!(
                        "name: {}\nruntime: nodejs\ndescription: MegaGate cloud project\n",
                        slugify(name)
                    ),
                )?;
                Self::write_file(
                    &target.join("package.json"),
                    &format!(
                        "{{\n  \"name\": \"{}\",\n  \"private\": true,\n  \"version\": \"0.1.0\"\n}}\n",
                        name
                    ),
                )?;
                Self::write_file(
                    &target.join("index.ts"),
                    "console.log('MegaGate Pulumi scaffold');\n",
                )?;
            }
        }

        Ok(())
    }

    fn write_cicd_files(target: &Path, _name: &str, framework: &str) -> Result<()> {
        if framework == "argocd" {
            Self::write_file(
                &target.join("argocd").join("application.yaml"),
                "apiVersion: argoproj.io/v1alpha1\nkind: Application\nmetadata:\n  name: megagate-app\nspec: {}\n",
            )?;
        } else {
            Self::write_file(
                &target.join(".github").join("workflows").join("ci.yml"),
                "name: CI\n\non:\n  push:\n  pull_request:\n\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - run: echo \"MegaGate CI scaffold\"\n",
            )?;
        }

        Ok(())
    }

    fn write_iot_files(target: &Path, name: &str, framework: &str) -> Result<()> {
        match framework {
            "platformio" | "firmware" => {
                Self::write_file(
                    &target.join("platformio.ini"),
                    "[env:esp32dev]\nplatform = espressif32\nboard = esp32dev\nframework = arduino\n",
                )?;
                Self::write_file(
                    &target.join("src").join("main.cpp"),
                    "#include <Arduino.h>\n\nvoid setup() {\n}\n\nvoid loop() {\n}\n",
                )?;
            }
            "zephyr" | "zephyr-arm" => Self::write_file(
                &target.join("west.yml"),
                "manifest:\n  version: 0.13\n  projects: []\n",
            )?,
            _ => {
                Self::write_file(
                    &target.join("Cargo.toml"),
                    &format!(
                        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                        slugify(name)
                    ),
                )?;
                Self::write_file(
                    &target.join("src").join("main.rs"),
                    "#![no_std]\n#![no_main]\n\n#[no_mangle]\npub extern \"C\" fn main() -> ! {\n    loop {}\n}\n",
                )?;
            }
        }

        Ok(())
    }

    fn write_app_files(target: &Path, name: &str, framework: &str) -> Result<()> {
        match framework {
            "kotlin" => {
                Self::write_file(
                    &target.join("settings.gradle.kts"),
                    &format!("rootProject.name = \"{}\"\n", name),
                )?;
                Self::write_file(
                    &target
                        .join("app")
                        .join("src")
                        .join("main")
                        .join("kotlin")
                        .join("Main.kt"),
                    "fun main() {\n    println(\"MegaGate Kotlin app scaffold\")\n}\n",
                )?;
            }
            "swift" => {
                Self::write_file(
                    &target.join("Package.swift"),
                    &format!(
                        "// swift-tools-version: 5.9\nimport PackageDescription\n\nlet package = Package(\n    name: \"{}\",\n    targets: [.executableTarget(name: \"{}\")]\n)\n",
                        name, name
                    ),
                )?;
                Self::write_file(
                    &target.join("Sources").join(name).join("main.swift"),
                    "print(\"MegaGate Swift app scaffold\")\n",
                )?;
            }
            _ => {
                Self::write_file(
                    &target.join("pubspec.yaml"),
                    &format!(
                        "name: {}\ndescription: MegaGate Flutter app\nversion: 0.1.0\n",
                        slugify(name)
                    ),
                )?;
                Self::write_file(
                    &target.join("lib").join("main.dart"),
                    "void main() {\n  print('MegaGate Flutter app scaffold');\n}\n",
                )?;
            }
        }

        Ok(())
    }

    fn write_lib_files(target: &Path, name: &str, framework: &str) -> Result<()> {
        match framework {
            "ts" | "typescript" => {
                Self::write_file(
                    &target.join("package.json"),
                    &format!(
                        "{{\n  \"name\": \"{}\",\n  \"version\": \"0.1.0\",\n  \"type\": \"module\"\n}}\n",
                        slugify(name)
                    ),
                )?;
                Self::write_file(
                    &target.join("tsconfig.json"),
                    "{\n  \"compilerOptions\": {\n    \"target\": \"ES2022\",\n    \"module\": \"ESNext\"\n  }\n}\n",
                )?;
                Self::write_file(
                    &target.join("src").join("index.ts"),
                    "export function hello(): string {\n    return 'hello from MegaGate';\n}\n",
                )?;
            }
            "python" => {
                let package = slugify(name).replace('-', "_");
                Self::write_file(
                    &target.join("pyproject.toml"),
                    &format!(
                        "[project]\nname = \"{}\"\nversion = \"0.1.0\"\nrequires-python = \">=3.11\"\n",
                        slugify(name)
                    ),
                )?;
                Self::write_file(
                    &target.join("src").join(&package).join("__init__.py"),
                    "__all__ = []\n",
                )?;
            }
            _ => {
                Self::write_file(
                    &target.join("Cargo.toml"),
                    &format!(
                        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                        slugify(name)
                    ),
                )?;
                Self::write_file(
                    &target.join("src").join("lib.rs"),
                    "pub fn hello() -> &'static str {\n    \"hello from MegaGate\"\n}\n",
                )?;
            }
        }

        Ok(())
    }

    fn write_file(path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    fn resolve_web_template_dir(config: &ScaffoldConfig) -> Result<PathBuf> {
        if !config.template_dir.as_os_str().is_empty() {
            return Ok(config.template_dir.clone());
        }

        let root = Self::web_templates_root();
        let framework = Self::framework(config);

        let dir = match config.sub_type.as_str() {
            "frontend" => root.join("frontend").join(framework),
            "backend" => {
                let language = config.frameworks.first().cloned().unwrap_or_default();
                let backend_framework = config.frameworks.get(1).cloned().unwrap_or_default();
                root.join("backend").join(language).join(backend_framework)
            }
            "fullstack" => {
                let bucket = if is_all_in_one_fullstack(&framework) {
                    "all-in-one"
                } else {
                    "split"
                };
                root.join("fullstack").join(bucket).join(framework)
            }
            "monorepo" => root.join("monorepo").join("base"),
            _ => root.join("frontend").join(framework),
        };

        if !dir.exists() {
            anyhow::bail!("Web template path '{}' does not exist", dir.display());
        }

        Ok(dir)
    }

    fn write_web_manifest(
        target: &Path,
        config: &ScaffoldConfig,
        template_dir: &Path,
    ) -> Result<()> {
        let template_display = template_dir
            .strip_prefix(Self::workspace_root())
            .unwrap_or(template_dir)
            .display()
            .to_string();
        let manifest = format!(
            "mode = \"{}\"\nframeworks = [{}]\ntemplate = \"{}\"\nfeatures = [{}]\n",
            config.sub_type,
            quoted_list(&config.frameworks),
            template_display,
            quoted_list(&config.features),
        );
        Self::write_file(&target.join(".megagate").join("web.toml"), &manifest)
    }

    fn materialize_web_skeleton(target: &Path, config: &ScaffoldConfig) -> Result<()> {
        match config.sub_type.as_str() {
            "monorepo" => {
                std::fs::create_dir_all(target.join("apps").join("frontend"))?;
                std::fs::create_dir_all(target.join("apps").join("backend"))?;
                std::fs::create_dir_all(target.join("packages"))?;
            }
            "frontend" | "fullstack" => {
                std::fs::create_dir_all(target.join("src"))?;
            }
            "backend" => {
                std::fs::create_dir_all(target.join("src"))?;
            }
            _ => {}
        }

        Ok(())
    }

    fn write_web_frontend_placeholder(target: &Path, name: &str, framework: &str) -> Result<()> {
        let uses_next_style = matches!(framework, "nextjs" | "nuxt" | "sveltekit");
        let package = if uses_next_style {
            format!(
                "{{\n  \"name\": \"{}\",\n  \"private\": true,\n  \"version\": \"0.1.0\",\n  \"scripts\": {{\n    \"dev\": \"mg web dev\",\n    \"build\": \"mg web build\"\n  }}\n}}\n",
                name
            )
        } else {
            format!(
                "{{\n  \"name\": \"{}\",\n  \"private\": true,\n  \"version\": \"0.1.0\",\n  \"type\": \"module\",\n  \"scripts\": {{\n    \"dev\": \"mg web dev\",\n    \"build\": \"mg web build\"\n  }}\n}}\n",
                name
            )
        };
        Self::write_file(&target.join("package.json"), &package)?;
        Self::write_file(
            &target.join("src").join("main.ts"),
            &format!(
                "console.log(\"MegaGate web frontend scaffold: {}\");\n",
                framework
            ),
        )
    }

    fn write_web_backend_placeholder(target: &Path, name: &str, framework: &str) -> Result<()> {
        Self::write_file(
            &target.join("package.json"),
            &format!(
                "{{\n  \"name\": \"{}\",\n  \"private\": true,\n  \"version\": \"0.1.0\",\n  \"scripts\": {{\n    \"dev\": \"mg web dev\",\n    \"build\": \"mg web build\"\n  }}\n}}\n",
                name
            ),
        )?;
        Self::write_file(
            &target.join("src").join("server.txt"),
            &format!("MegaGate web backend scaffold: {}\n", framework),
        )
    }

    fn write_web_fullstack_placeholder(target: &Path, name: &str, framework: &str) -> Result<()> {
        Self::write_file(
            &target.join("package.json"),
            &format!(
                "{{\n  \"name\": \"{}\",\n  \"private\": true,\n  \"version\": \"0.1.0\",\n  \"scripts\": {{\n    \"dev\": \"mg web dev\",\n    \"build\": \"mg web build\"\n  }}\n}}\n",
                name
            ),
        )?;
        Self::write_file(
            &target.join("src").join("app.txt"),
            &format!("MegaGate web fullstack scaffold: {}\n", framework),
        )
    }

    fn write_web_monorepo_placeholder(
        target: &Path,
        name: &str,
        config: &ScaffoldConfig,
    ) -> Result<()> {
        let frontend = config.frameworks.first().cloned().unwrap_or_default();
        let backend = config.frameworks.get(1).cloned().unwrap_or_default();
        Self::write_file(
            &target.join("package.json"),
            &format!(
                "{{\n  \"name\": \"{}\",\n  \"private\": true,\n  \"version\": \"0.1.0\",\n  \"workspaces\": [\"apps/*\", \"packages/*\"],\n  \"scripts\": {{\n    \"dev\": \"mg web dev\",\n    \"build\": \"mg web build\"\n  }}\n}}\n",
                name
            ),
        )?;
        Self::write_file(
            &target.join("apps").join("frontend").join("README.md"),
            &format!("# Frontend\n\nFramework: `{}`\n", frontend),
        )?;
        Self::write_file(
            &target.join("apps").join("backend").join("README.md"),
            &format!("# Backend\n\nFramework: `{}`\n", backend),
        )?;
        Self::write_file(
            &target.join("packages").join("README.md"),
            "Shared packages live here.\n",
        )
    }
}

fn common_readme(name: &str, core: &str, framework: &str, features: &[String]) -> String {
    let mut out = format!(
        "# {}\n\nGenerated by MegaGate.\n\n- Core: `{}`\n- Framework: `{}`\n",
        name, core, framework
    );
    if !features.is_empty() {
        out.push_str(&format!("- Features: `{}`\n", features.join("`, `")));
    }
    out.push_str("\n## Next\n\nRun `mg install` in this directory when the adapter for this core is ready.\n");
    out
}

fn quoted_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("\"{}\"", value))
        .collect::<Vec<_>>()
        .join(", ")
}

fn common_gitignore(core: &str, framework: &str) -> String {
    let mut lines = vec![
        ".DS_Store",
        ".megagate/cache",
        ".megagate/tmp",
        ".env",
        ".env.local",
    ];

    match core {
        "web" => lines.extend(["node_modules", "dist", ".next", ".svelte-kit"]),
        "game" => {
            if framework == "unity" {
                lines.extend(["Library", "Temp", "Build"]);
            } else if framework == "godot" {
                lines.extend([".godot", ".import"]);
            } else {
                lines.push("target");
            }
        }
        "ai" => lines.extend([".venv", "__pycache__", ".pytest_cache"]),
        "clo" => lines.extend([".pulumi", ".terraform", ".terraform.lock.hcl"]),
        "cicd" => lines.extend([".artifacts", ".secrets"]),
        "iot" => lines.extend([".pio", "target", "build"]),
        "app" => lines.extend([".dart_tool", "build", ".gradle", ".build"]),
        "lib" => lines.extend(["target", "dist", "__pycache__"]),
        _ => {}
    }

    lines.join("\n") + "\n"
}

fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '-' | '_' | ' ') {
            if !slug.ends_with('-') {
                slug.push('-');
            }
        }
    }

    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "megagate-project".to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_web_framework(framework: &str) -> String {
    match framework {
        "next" | "next-app" => "nextjs".to_string(),
        other => other.to_string(),
    }
}

fn is_all_in_one_fullstack(framework: &str) -> bool {
    matches!(framework, "nextjs" | "nuxt" | "sveltekit" | "remix")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scaffold_writes_baseline_for_all_cores() {
        let root = tempfile::tempdir().unwrap();
        let cases = [
            ("web", "react-vite", "package.json"),
            ("game", "bevy", "Cargo.toml"),
            ("ai", "python-agent", "pyproject.toml"),
            ("clo", "pulumi-aws", "Pulumi.yaml"),
            ("cicd", "github-actions", ".github/workflows/ci.yml"),
            ("iot", "esp32-rust", "Cargo.toml"),
            ("app", "flutter", "pubspec.yaml"),
            ("lib", "rust", "Cargo.toml"),
        ];

        for (core, framework, expected) in cases {
            let project_dir = root.path().join(format!("{core}-{framework}"));
            let config = ScaffoldConfig {
                core: core.to_string(),
                sub_type: String::new(),
                frameworks: vec![framework.to_string()],
                project_name: project_dir.to_string_lossy().to_string(),
                features: vec![],
                template_dir: PathBuf::new(),
            };

            let out = Scaffolder::scaffold(&config).unwrap();
            assert!(out.join(expected).exists(), "{} {}", core, expected);
            assert!(out.join("README.md").exists(), "{} README", core);
        }
    }

    #[test]
    fn test_display_name_uses_last_path_segment() {
        let path = Path::new("/tmp/my-project");
        assert_eq!(Scaffolder::display_name(path), "my-project");
    }
}
