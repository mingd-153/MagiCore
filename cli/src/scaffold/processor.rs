use anyhow::Result;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::scaffold::template_root::TemplateRoot;
use crate::wizard::engine::ScaffoldConfig;

struct WebFrameworkConfig {
    name: &'static str,
    sub_type: &'static str,
    base: Option<&'static str>,
}

const WEB_FRAMEWORKS: &[WebFrameworkConfig] = &[
    WebFrameworkConfig {
        name: "vanilla",
        sub_type: "frontend",
        base: None,
    },
    WebFrameworkConfig {
        name: "react-vite",
        sub_type: "frontend",
        base: None,
    },
    WebFrameworkConfig {
        name: "vue-vite",
        sub_type: "frontend",
        base: None,
    },
    WebFrameworkConfig {
        name: "nextjs",
        sub_type: "frontend",
        base: None,
    },
    WebFrameworkConfig {
        name: "sveltekit",
        sub_type: "frontend",
        base: None,
    },
    WebFrameworkConfig {
        name: "nuxt",
        sub_type: "frontend",
        base: None,
    },
    WebFrameworkConfig {
        name: "angular",
        sub_type: "frontend",
        base: None,
    },
    WebFrameworkConfig {
        name: "solidjs",
        sub_type: "frontend",
        base: None,
    },
    WebFrameworkConfig {
        name: "qwik",
        sub_type: "frontend",
        base: None,
    },
    WebFrameworkConfig {
        name: "astro",
        sub_type: "frontend",
        base: None,
    },
    WebFrameworkConfig {
        name: "express",
        sub_type: "backend",
        base: Some("node"),
    },
    WebFrameworkConfig {
        name: "fastify",
        sub_type: "backend",
        base: Some("node"),
    },
    WebFrameworkConfig {
        name: "nestjs",
        sub_type: "backend",
        base: Some("node"),
    },
    WebFrameworkConfig {
        name: "hono",
        sub_type: "backend",
        base: Some("node"),
    },
    WebFrameworkConfig {
        name: "trpc",
        sub_type: "backend",
        base: Some("node"),
    },
    WebFrameworkConfig {
        name: "laravel",
        sub_type: "backend",
        base: Some("php"),
    },
    WebFrameworkConfig {
        name: "symfony",
        sub_type: "backend",
        base: Some("php"),
    },
    WebFrameworkConfig {
        name: "spring-boot",
        sub_type: "backend",
        base: Some("java"),
    },
    WebFrameworkConfig {
        name: "quarkus",
        sub_type: "backend",
        base: Some("java"),
    },
    WebFrameworkConfig {
        name: "gin",
        sub_type: "backend",
        base: Some("go"),
    },
    WebFrameworkConfig {
        name: "echo",
        sub_type: "backend",
        base: Some("go"),
    },
    WebFrameworkConfig {
        name: "fiber",
        sub_type: "backend",
        base: Some("go"),
    },
    WebFrameworkConfig {
        name: "fastapi",
        sub_type: "backend",
        base: Some("python"),
    },
    WebFrameworkConfig {
        name: "django",
        sub_type: "backend",
        base: Some("python"),
    },
    WebFrameworkConfig {
        name: "flask",
        sub_type: "backend",
        base: Some("python"),
    },
    WebFrameworkConfig {
        name: "axum",
        sub_type: "backend",
        base: Some("rust"),
    },
    WebFrameworkConfig {
        name: "actix-web",
        sub_type: "backend",
        base: Some("rust"),
    },
    WebFrameworkConfig {
        name: "remix",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "react-fastify",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "vue-laravel",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "react-spring",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "custom",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "react-express",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "react-hono",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "react-nestjs",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "react-trpc",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "vue-express",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "vue-hono",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "vue-nestjs",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "svelte-express",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "svelte-hono",
        sub_type: "fullstack",
        base: None,
    },
    // Non-Node backend combos — no split leaf; resolver falls back to the
    // monorepo composite (apps/frontend + apps/backend).
    WebFrameworkConfig {
        name: "react-axum",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "react-actix-web",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "react-gin",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "react-echo",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "react-fiber",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "react-fastapi",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "react-django",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "react-flask",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "react-quarkus",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "react-symfony",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "vue-axum",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "vue-actix-web",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "vue-gin",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "vue-echo",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "vue-fiber",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "vue-fastapi",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "vue-django",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "vue-flask",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "vue-quarkus",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "vue-symfony",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "svelte-axum",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "svelte-gin",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "svelte-fastapi",
        sub_type: "fullstack",
        base: None,
    },
    WebFrameworkConfig {
        name: "svelte-quarkus",
        sub_type: "fullstack",
        base: None,
    },
];

pub struct Scaffolder;

impl Scaffolder {
    #[allow(dead_code)]
    pub fn infer_web_create_config(framework: &str, project_name: &str) -> Result<ScaffoldConfig> {
        let framework = normalize_web_framework(framework);
        let cfg = WEB_FRAMEWORKS
            .iter()
            .find(|f| f.name == framework)
            .ok_or_else(|| anyhow::anyhow!("Unsupported web framework '{framework}'"))?;

        let frameworks = match cfg.base {
            Some(base) => vec![base.to_string(), framework.clone()],
            None => vec![framework.clone()],
        };

        Ok(ScaffoldConfig {
            core: "web".to_string(),
            sub_type: cfg.sub_type.to_string(),
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

    fn web_templates_root() -> TemplateRoot {
        TemplateRoot::resolve("web")
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
        if config.core == "web" {
            return Ok(());
        }

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

        if config.core != "web" {
            let layer = Self::core_template_layer(&config.core, &framework);
            if Self::layer_has_contract(&layer) {
                return Self::materialize_core_template(target, &layer, config, &name, &framework);
            }
        }

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
        _name: &str,
        _framework: &str,
    ) -> Result<()> {
        let layers = Self::resolve_web_template_layers(config)?;
        Self::materialize_web_templates(target, config, &layers)
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
                    "use bevy::prelude::*;\n\nfn main() {\n    App::new()\n        .add_plugins(DefaultPlugins)\n        .add_systems(Startup, setup)\n        .add_systems(Update, frame_counter)\n        .run();\n}\n\nfn setup(mut commands: Commands) {\n    commands.spawn(Camera2d);\n    commands.spawn(Sprite {\n        color: Color::srgb(0.2, 0.8, 0.4),\n        custom_size: Some(Vec2::new(100.0, 100.0)),\n        ..default()\n    });\n}\n\nfn frame_counter(mut frames: Local<u64>) {\n    *frames += 1;\n    if *frames % 60 == 0 {\n        println!(\"MegaGate bevy game running ({} frames)\", *frames);\n    }\n}\n",
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
        match framework {
            "argocd" => Self::write_file(
                &target.join("argocd").join("application.yaml"),
                "apiVersion: argoproj.io/v1alpha1\nkind: Application\nmetadata:\n  name: megagate-app\nspec: {}\n",
            )?,
            "cloudflare" => {
                Self::write_file(
                    &target.join("wrangler.toml"),
                    "name = \"worker\"\nmain = \"src/index.js\"\ncompatibility_date = \"2026-01-01\"\n",
                )?;
                Self::write_file(
                    &target.join("src").join("index.js"),
                    "export default {\n  async fetch(request) {\n    return new Response(\"Hello from MegaGate Worker\", { status: 200 });\n  },\n};\n",
                )?;
            }
            _ => Self::write_file(
                &target.join(".github").join("workflows").join("ci.yml"),
                "name: CI\n\non:\n  push:\n  pull_request:\n\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - run: echo \"MegaGate CI scaffold\"\n",
            )?,
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

    fn write_bytes(path: &Path, content: &[u8]) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    fn resolve_web_template_dir(config: &ScaffoldConfig) -> Result<TemplateRoot> {
        if !config.template_dir.as_os_str().is_empty() {
            let dir = TemplateRoot::disk(config.template_dir.clone());
            return Ok(dir);
        }

        let root = Self::web_templates_root();
        let framework = Self::framework(config);
        let mode = effective_web_mode(config);

        let dir = match mode.as_str() {
            "frontend" => root.join("frontend").join(&framework),
            "backend" => {
                let (language, backend_framework) = if config.frameworks.len() >= 2 {
                    (
                        config.frameworks.first().cloned().unwrap_or_default(),
                        config.frameworks.get(1).cloned().unwrap_or_default(),
                    )
                } else {
                    (
                        infer_backend_language(&framework)
                            .unwrap_or_default()
                            .to_string(),
                        framework.clone(),
                    )
                };
                root.join("backend")
                    .join(&language)
                    .join(&backend_framework)
            }
            "fullstack" => {
                let bucket = if is_all_in_one_fullstack(&framework) {
                    "all-in-one"
                } else {
                    "split"
                };
                root.join("fullstack").join(bucket).join(&framework)
            }
            "monorepo" => root.join("monorepo").join("base"),
            _ => root.join("frontend").join(&framework),
        };

        if !dir.exists("") {
            anyhow::bail!("Web template path '{}' does not exist", dir.logical_rel());
        }

        Ok(dir)
    }

    fn resolve_web_template_layers(config: &ScaffoldConfig) -> Result<Vec<TemplateRoot>> {
        let root = Self::web_templates_root();
        let mode = effective_web_mode(config);
        let mut layers = vec![root.join("shared").join("partials").join("base")];
        let frontend_framework = config.frameworks.first().cloned().unwrap_or_default();

        // Fullstack split with a non-Node backend (axum/gin/fastapi/...) has
        // no dedicated split leaf; reuse the monorepo composite instead of
        // hardcoding one folder per FE×BE combo. All-in-one (nextjs/nuxt/
        // sveltekit/remix) dùng leaf riêng, không fallback.
        if mode == "fullstack"
            && !config.frameworks.is_empty()
            && !is_all_in_one_fullstack(&frontend_framework)
            && !root
                .join("fullstack")
                .join("split")
                .join(&frontend_framework)
                .exists("")
        {
            return Self::monorepo_layer_stack(&root, config, layers);
        }

        match mode.as_str() {
            "frontend" | "backend" | "fullstack" => {
                let leaf = Self::resolve_web_template_dir(config)?;
                let shared_mode_partial = root.join("shared").join("partials").join(mode.as_str());
                let frontend_foundation_partial = root
                    .join("shared")
                    .join("partials")
                    .join("frontend-foundation");
                let frontend_common_partial =
                    root.join("shared").join("partials").join("frontend-common");
                let frontend_rust_ready_partial = root
                    .join("shared")
                    .join("partials")
                    .join("frontend-rust-ready");

                if mode == "frontend" || mode == "backend" {
                    if mode == "frontend" {
                        Self::ensure_web_layer_ready(
                            &leaf,
                            &format!("frontend framework '{frontend_framework}'"),
                        )?;
                        layers.push(frontend_foundation_partial);
                        layers.push(frontend_rust_ready_partial);
                        if framework_uses_react_shell(&frontend_framework) {
                            layers.push(frontend_common_partial);
                        }
                    }
                    layers.push(shared_mode_partial);
                    layers.push(leaf);
                } else if Self::layer_has_contract(&leaf) {
                    if fullstack_uses_frontend_foundation(&frontend_framework) {
                        layers.push(frontend_foundation_partial);
                        layers.push(frontend_rust_ready_partial);
                    }
                    if fullstack_uses_react_shell(&frontend_framework) {
                        layers.push(frontend_common_partial);
                    }
                    layers.push(leaf);
                } else {
                    layers.push(shared_mode_partial);
                    layers.push(leaf);
                }
            }
            "monorepo" => {
                layers = Self::monorepo_layer_stack(&root, config, vec![layers.pop().unwrap()])?;
            }
            other => anyhow::bail!("Unsupported web mode '{other}'"),
        }

        for layer in &layers {
            if !layer.exists("") {
                anyhow::bail!(
                    "Web template layer '{}' does not exist",
                    layer.logical_rel()
                );
            }
        }

        Ok(layers)
    }

    fn layer_has_contract(layer: &TemplateRoot) -> bool {
        layer.exists("template.toml") && layer.exists("sources")
    }

    /// Composite monorepo layer stack (templates/web/monorepo/*) — shared by
    /// "monorepo" mode and as fallback for fullstack split combos without a
    /// dedicated leaf (react-axum, vue-gin, ...). `seed` = layers built so
    /// far (caller passes the shared base partial).
    fn monorepo_layer_stack(
        root: &TemplateRoot,
        config: &ScaffoldConfig,
        mut layers: Vec<TemplateRoot>,
    ) -> Result<Vec<TemplateRoot>> {
        let (frontend, backend) = match config.frameworks.as_slice() {
            // monorepo mode: explicit [fe, be]
            [fe, be, ..] => (fe.clone(), be.clone()),
            // fullstack split fallback: single combined "react-axum" arg
            [combined] => {
                let be = fullstack_backend_framework(combined).to_string();
                let fe = fullstack_frontend_framework(combined).to_string();
                if be.is_empty() || fe.is_empty() {
                    anyhow::bail!(
                        "Unsupported fullstack framework '{combined}' for composited web scaffold"
                    );
                }
                (fe, be)
            }
            _ => anyhow::bail!("Web scaffold needs a frontend and backend framework"),
        };
        let backend_language = infer_backend_language(&backend)
            .ok_or_else(|| anyhow::anyhow!("Unsupported monorepo backend '{backend}'"))?;
        let frontend_leaf = root.join("monorepo").join("frontend").join(&frontend);
        Self::ensure_web_layer_ready(
            &frontend_leaf,
            &format!("monorepo frontend framework '{frontend}'"),
        )?;

        layers.push(root.join("shared").join("partials").join("monorepo"));
        layers.push(root.join("monorepo").join("base"));
        layers.push(
            root.join("shared")
                .join("partials")
                .join("monorepo-frontend-foundation"),
        );
        layers.push(
            root.join("shared")
                .join("partials")
                .join("monorepo-frontend-rust-ready"),
        );
        if framework_uses_react_shell(&frontend) {
            layers.push(
                root.join("shared")
                    .join("partials")
                    .join("monorepo-frontend-common"),
            );
        }
        layers.push(
            root.join("shared")
                .join("partials")
                .join("monorepo-frontend"),
        );
        layers.push(frontend_leaf);
        layers.push(
            root.join("shared")
                .join("partials")
                .join("monorepo-backend"),
        );
        layers.push(
            root.join("monorepo")
                .join("backend")
                .join(backend_language)
                .join(&backend),
        );
        layers.push(
            root.join("shared")
                .join("partials")
                .join("monorepo-packages"),
        );
        Ok(layers)
    }

    fn ensure_web_layer_ready(layer: &TemplateRoot, label: &str) -> Result<()> {
        if Self::layer_has_contract(layer) {
            return Ok(());
        }

        anyhow::bail!(
            "Scaffold for {} is not implemented yet at '{}'",
            label,
            layer.logical_rel()
        )
    }

    /// Layer scaffold của core non-web: `templates/{core}/{framework}` (Q13).
    fn core_template_layer(core: &str, framework: &str) -> TemplateRoot {
        TemplateRoot::resolve(&format!("{core}/{framework}"))
    }

    /// Materialize template đơn layer cho core non-web (chung cho game/iot/cloud/cicd/app/ai/lib).
    /// Context tối thiểu: project_name/slug/package, core, framework, features (Q13).
    fn materialize_core_template(
        target: &Path,
        layer: &TemplateRoot,
        config: &ScaffoldConfig,
        name: &str,
        framework: &str,
    ) -> Result<()> {
        let Some(manifest) = TemplateManifest::load(layer)? else {
            anyhow::bail!(
                "Template layer '{}' missing template.toml",
                layer.logical_rel()
            );
        };
        let context = CoreTemplateContext::new(config, name, framework);
        let active_features: HashSet<&str> = config_feature_set(&context.features);
        let active_files = manifest
            .files
            .iter()
            .filter(|file| file.is_enabled(&active_features))
            .collect::<Vec<_>>();

        let mut seen_targets = HashSet::new();
        for file in &active_files {
            let target_path = render_core_target_path(&file.target, &context);
            if !seen_targets.insert(target_path.clone()) {
                anyhow::bail!(
                    "Duplicate template target '{}' in '{}'",
                    file.target,
                    layer.logical_rel()
                );
            }
        }

        for file in active_files {
            let source_rel = format!("sources/{}", file.source);
            if !layer.exists(&source_rel) {
                anyhow::bail!(
                    "Template source '{}' does not exist in '{}'",
                    file.source,
                    layer.logical_rel()
                );
            }
            let target_path = render_core_target_path(&file.target, &context);
            let bytes = layer.read(&source_rel)?;
            match std::str::from_utf8(&bytes) {
                Ok(contents) => {
                    let rendered = context.render(
                        contents,
                        &file.required_context,
                        &layer.label(&source_rel),
                    )?;
                    Self::write_file(&target.join(&target_path), &rendered)?;
                }
                Err(_) => {
                    if !file.required_context.is_empty() {
                        anyhow::bail!(
                            "Binary template source '{}' cannot declare template context",
                            layer.label(&source_rel)
                        );
                    }
                    Self::write_bytes(&target.join(&target_path), &bytes)?;
                }
            }
        }

        Ok(())
    }

    fn materialize_web_templates(
        target: &Path,
        config: &ScaffoldConfig,
        layers: &[TemplateRoot],
    ) -> Result<()> {
        let context = WebTemplateContext::new(config, layers);
        for layer in layers {
            Self::materialize_template_layer(target, layer, &context)?;
        }
        Ok(())
    }

    fn materialize_template_layer(
        target: &Path,
        layer: &TemplateRoot,
        context: &WebTemplateContext,
    ) -> Result<()> {
        let Some(manifest) = TemplateManifest::load(layer)? else {
            return Ok(());
        };
        let active_features: HashSet<&str> = config_feature_set(&context.features);
        let active_files = manifest
            .files
            .iter()
            .filter(|file| file.is_enabled(&active_features))
            .collect::<Vec<_>>();

        let mut seen_targets = HashSet::new();
        for file in &active_files {
            let target_path = render_target_path(&file.target, context);
            if !seen_targets.insert(target_path.clone()) {
                anyhow::bail!(
                    "Duplicate template target '{}' in '{}'",
                    file.target,
                    layer.logical_rel()
                );
            }
        }

        for file in active_files {
            let source_rel = format!("sources/{}", file.source);
            if !layer.exists(&source_rel) {
                anyhow::bail!(
                    "Template source '{}' does not exist in '{}'",
                    file.source,
                    layer.logical_rel()
                );
            }

            let target_path = render_target_path(&file.target, context);
            let bytes = layer.read(&source_rel)?;
            match std::str::from_utf8(&bytes) {
                Ok(contents) => {
                    let rendered = context.render_with_contract(
                        contents,
                        &file.required_context,
                        &layer.label(&source_rel),
                    )?;
                    Self::write_file(&target.join(&target_path), &rendered)?;
                }
                Err(_) => {
                    if !file.required_context.is_empty() {
                        anyhow::bail!(
                            "Binary template source '{}' cannot declare template context",
                            layer.label(&source_rel)
                        );
                    }
                    Self::write_bytes(&target.join(&target_path), &bytes)?;
                }
            }
        }

        Ok(())
    }
}

fn render_target_path(target: &str, context: &WebTemplateContext) -> String {
    let mut s = target.to_string();
    if let Some(v) = context.value("project_slug") {
        s = s
            .replace("{{ project_slug }}", v)
            .replace("{{project_slug}}", v);
    }
    if let Some(v) = context.value("project_name") {
        s = s
            .replace("{{ project_name }}", v)
            .replace("{{project_name}}", v);
    }
    if let Some(v) = context.value("project_package") {
        s = s
            .replace("{{ project_package }}", v)
            .replace("{{project_package}}", v);
    }
    s
}

fn framework_uses_react_shell(framework: &str) -> bool {
    matches!(framework, "react-vite" | "nextjs" | "solidjs")
}

fn fullstack_uses_frontend_foundation(framework: &str) -> bool {
    matches!(
        framework,
        "react-fastify"
            | "react-spring"
            | "react-express"
            | "react-hono"
            | "react-nestjs"
            | "react-trpc"
            | "vue-laravel"
            | "vue-express"
            | "vue-hono"
            | "vue-nestjs"
            | "svelte-express"
            | "svelte-hono"
    )
}

fn fullstack_uses_react_shell(framework: &str) -> bool {
    matches!(
        framework,
        "react-fastify"
            | "react-spring"
            | "react-express"
            | "react-hono"
            | "react-nestjs"
            | "react-trpc"
    )
}

fn fullstack_frontend_framework(framework: &str) -> &str {
    match framework {
        "react-fastify" | "react-spring" | "react-express" | "react-hono" | "react-nestjs"
        | "react-trpc" => "react-vite",
        "vue-laravel" | "vue-express" | "vue-hono" | "vue-nestjs" => "vue-vite",
        "svelte-express" | "svelte-hono" => "sveltekit",
        // Non-Node backends (Rust/Go/Python/Java/PHP)
        "react-axum" | "react-actix-web" | "react-gin" | "react-echo" | "react-fiber"
        | "react-fastapi" | "react-django" | "react-flask" | "react-quarkus" | "react-symfony" => {
            "react-vite"
        }
        "vue-axum" | "vue-actix-web" | "vue-gin" | "vue-echo" | "vue-fiber" | "vue-fastapi"
        | "vue-django" | "vue-flask" | "vue-quarkus" | "vue-symfony" => "vue-vite",
        "svelte-axum" | "svelte-gin" | "svelte-fastapi" | "svelte-quarkus" => "sveltekit",
        other => other,
    }
}

fn fullstack_backend_framework(framework: &str) -> &str {
    match framework {
        "react-fastify" => "fastify",
        "react-spring" => "spring-boot",
        "vue-laravel" => "laravel",
        "react-express" | "svelte-express" | "vue-express" => "express",
        "react-hono" | "svelte-hono" | "vue-hono" => "hono",
        "react-nestjs" | "vue-nestjs" => "nestjs",
        "react-trpc" => "trpc",
        // Non-Node backends (Rust/Go/Python/Java/PHP)
        "react-axum" | "vue-axum" | "svelte-axum" => "axum",
        "react-actix-web" | "vue-actix-web" => "actix-web",
        "react-gin" | "vue-gin" | "svelte-gin" => "gin",
        "react-echo" | "vue-echo" => "echo",
        "react-fiber" | "vue-fiber" => "fiber",
        "react-fastapi" | "vue-fastapi" | "svelte-fastapi" => "fastapi",
        "react-django" | "vue-django" => "django",
        "react-flask" | "vue-flask" => "flask",
        "react-quarkus" | "vue-quarkus" | "svelte-quarkus" => "quarkus",
        "react-symfony" | "vue-symfony" => "symfony",
        other => other,
    }
}

#[derive(Debug, Clone)]
struct WebTemplateContext {
    project_name: String,
    project_slug: String,
    project_package: String,
    mode: String,
    framework: String,
    frameworks: String,
    frontend_framework: String,
    backend_framework: String,
    backend_language: String,
    template: String,
    features: String,
    execution_architecture: String,
    execution_lane: String,
    execution_compatibility_layer: String,
    execution_native_targets: String,
}

impl WebTemplateContext {
    fn new(config: &ScaffoldConfig, layers: &[TemplateRoot]) -> Self {
        let primary_template = if config.sub_type == "monorepo" {
            layers
                .iter()
                .map(|layer| layer.logical_rel())
                .filter(|layer| !layer.starts_with("templates/web/shared/partials"))
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            Scaffolder::resolve_web_template_dir(config)
                .ok()
                .map(|dir| dir.logical_rel())
                .unwrap_or_default()
        };

        let project_name = Scaffolder::display_name(Path::new(&config.project_name));
        let project_slug = slugify(&project_name);
        let mode = effective_web_mode(config);
        let framework = Scaffolder::framework(config);
        let frontend_framework = match mode.as_str() {
            "monorepo" => config.frameworks.first().cloned().unwrap_or_default(),
            "frontend" => framework.clone(),
            "fullstack" => fullstack_frontend_framework(&framework).to_string(),
            _ => String::new(),
        };
        let backend_framework = match mode.as_str() {
            "backend" => framework.clone(),
            "monorepo" => config.frameworks.get(1).cloned().unwrap_or_default(),
            "fullstack" => fullstack_backend_framework(&framework).to_string(),
            _ => String::new(),
        };
        let backend_language = if mode == "backend" {
            if config.frameworks.len() >= 2 {
                config.frameworks.first().cloned().unwrap_or_default()
            } else {
                infer_backend_language(&framework)
                    .unwrap_or_default()
                    .to_string()
            }
        } else {
            infer_backend_language(&backend_framework)
                .unwrap_or_default()
                .to_string()
        };

        let project_package = project_slug.replace('-', "_");
        let execution_compatibility_layer = if config.features.iter().any(|feature| {
            let normalized = feature.trim().to_ascii_lowercase();
            normalized == "ts" || normalized == "typescript"
        }) {
            "ts".to_string()
        } else {
            "js".to_string()
        };
        let execution_architecture =
            if matches!(mode.as_str(), "frontend" | "fullstack" | "monorepo") {
                "rust-first".to_string()
            } else {
                "multi-runtime".to_string()
            };
        let execution_lane = if matches!(mode.as_str(), "frontend" | "fullstack" | "monorepo") {
            "compatibility-shell".to_string()
        } else {
            "runtime-native".to_string()
        };
        let execution_native_targets =
            if matches!(mode.as_str(), "frontend" | "fullstack" | "monorepo") {
                quoted_list(&[
                    "frontend-executable".to_string(),
                    "wasm-bridge".to_string(),
                    "native-module".to_string(),
                ])
            } else {
                quoted_list(&["service-binary".to_string(), "worker-binary".to_string()])
            };

        Self {
            project_name,
            project_slug,
            project_package,
            mode,
            framework,
            frameworks: quoted_list(&config.frameworks),
            frontend_framework,
            backend_framework,
            backend_language,
            template: primary_template,
            features: quoted_list(&config.features),
            execution_architecture,
            execution_lane,
            execution_compatibility_layer,
            execution_native_targets,
        }
    }

    fn value(&self, key: &str) -> Option<&str> {
        match key {
            "project_name" => Some(self.project_name.as_str()),
            "project_slug" => Some(self.project_slug.as_str()),
            "project_package" => Some(self.project_package.as_str()),
            "mode" => Some(self.mode.as_str()),
            "framework" => Some(self.framework.as_str()),
            "frameworks" => Some(self.frameworks.as_str()),
            "frontend_framework" => Some(self.frontend_framework.as_str()),
            "backend_framework" => Some(self.backend_framework.as_str()),
            "backend_language" => Some(self.backend_language.as_str()),
            "template" => Some(self.template.as_str()),
            "features" => Some(self.features.as_str()),
            "execution_architecture" => Some(self.execution_architecture.as_str()),
            "execution_lane" => Some(self.execution_lane.as_str()),
            "execution_compatibility_layer" => Some(self.execution_compatibility_layer.as_str()),
            "execution_native_targets" => Some(self.execution_native_targets.as_str()),
            _ => None,
        }
    }

    fn render_with_contract(
        &self,
        input: &str,
        required_context: &[String],
        source: &str,
    ) -> Result<String> {
        let declared: HashSet<&str> = required_context.iter().map(String::as_str).collect();
        let used = extract_template_tokens(input);

        for token in &used {
            if !declared.contains(token.as_str()) {
                anyhow::bail!(
                    "Template token '{}' in '{}' is not declared in template.toml",
                    token,
                    source
                );
            }
            if self.value(token).is_none() {
                anyhow::bail!(
                    "Template token '{}' in '{}' is not supported by the Rust compiler context",
                    token,
                    source
                );
            }
        }

        for key in required_context {
            if self.value(key).is_none() {
                anyhow::bail!(
                    "Template context '{}' required by '{}' is not supported by the Rust compiler context",
                    key,
                    source
                );
            }
        }

        let mut rendered = input.to_string();
        for key in required_context {
            if let Some(value) = self.value(key) {
                rendered = rendered
                    .replace(&format!("{{{{{key}}}}}"), value)
                    .replace(&format!("{{{{ {key} }}}}"), value);
            }
        }

        Ok(rendered)
    }
}

#[derive(Debug, Clone)]
struct CoreTemplateContext {
    project_name: String,
    project_slug: String,
    project_package: String,
    core: String,
    framework: String,
    features: String,
    board: String,
    target: String,
}

impl CoreTemplateContext {
    fn new(config: &ScaffoldConfig, name: &str, framework: &str) -> Self {
        let project_name = Scaffolder::display_name(Path::new(name));
        let project_slug = slugify(&project_name);
        let board = config.features.first().cloned().unwrap_or_default();
        let target = iot_target_for_board(&board);
        Self {
            project_name,
            project_slug: project_slug.clone(),
            project_package: project_slug.replace('-', "_"),
            core: config.core.clone(),
            framework: framework.to_string(),
            features: quoted_list(&config.features),
            board,
            target,
        }
    }

    fn value(&self, key: &str) -> Option<&str> {
        match key {
            "project_name" => Some(self.project_name.as_str()),
            "project_slug" => Some(self.project_slug.as_str()),
            "project_package" => Some(self.project_package.as_str()),
            "core" => Some(self.core.as_str()),
            "framework" => Some(self.framework.as_str()),
            "features" => Some(self.features.as_str()),
            "board" => Some(self.board.as_str()),
            "target" => Some(self.target.as_str()),
            _ => None,
        }
    }

    fn render(&self, input: &str, required_context: &[String], source: &str) -> Result<String> {
        let declared: HashSet<&str> = required_context.iter().map(String::as_str).collect();
        let used = extract_template_tokens(input);

        for token in &used {
            if !declared.contains(token.as_str()) {
                anyhow::bail!(
                    "Template token '{}' in '{}' is not declared in template.toml",
                    token,
                    source
                );
            }
            if self.value(token).is_none() {
                anyhow::bail!(
                    "Template token '{}' in '{}' is not supported by the Rust compiler context",
                    token,
                    source
                );
            }
        }

        for key in required_context {
            if self.value(key).is_none() {
                anyhow::bail!(
                    "Template context '{}' required by '{}' is not supported by the Rust compiler context",
                    key,
                    source
                );
            }
        }

        let mut rendered = input.to_string();
        for key in required_context {
            if let Some(value) = self.value(key) {
                rendered = rendered
                    .replace(&format!("{{{{{key}}}}}"), value)
                    .replace(&format!("{{{{ {key} }}}}"), value);
            }
        }

        Ok(rendered)
    }
}

fn render_core_target_path(target: &str, context: &CoreTemplateContext) -> String {
    let mut s = target.to_string();
    if let Some(v) = context.value("project_slug") {
        s = s
            .replace("{{ project_slug }}", v)
            .replace("{{project_slug}}", v);
    }
    if let Some(v) = context.value("project_name") {
        s = s
            .replace("{{ project_name }}", v)
            .replace("{{project_name}}", v);
    }
    if let Some(v) = context.value("project_package") {
        s = s
            .replace("{{ project_package }}", v)
            .replace("{{project_package}}", v);
    }
    s
}

#[derive(Debug, Deserialize)]
struct TemplateManifest {
    files: Vec<TemplateFile>,
}

impl TemplateManifest {
    fn load(layer: &TemplateRoot) -> Result<Option<Self>> {
        if !layer.exists("template.toml") {
            if !layer.exists("sources") {
                return Ok(None);
            }
            anyhow::bail!("Missing template manifest '{}'", layer.logical_rel());
        }

        let contents = String::from_utf8(layer.read("template.toml")?)?;
        Ok(Some(toml::from_str(&contents)?))
    }
}

#[derive(Debug, Deserialize)]
struct TemplateFile {
    source: String,
    target: String,
    #[serde(default)]
    required_context: Vec<String>,
    #[serde(default)]
    include_features: Vec<String>,
    #[serde(default)]
    exclude_features: Vec<String>,
}

impl TemplateFile {
    fn is_enabled(&self, active_features: &HashSet<&str>) -> bool {
        self.include_features
            .iter()
            .all(|feature| active_features.contains(feature.as_str()))
            && self
                .exclude_features
                .iter()
                .all(|feature| !active_features.contains(feature.as_str()))
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
    let next_step = if core == "web" {
        "Run `mg install-web` in this directory when the adapter for this core is ready."
    } else {
        "Run `mg install` in this directory when the adapter for this core is ready."
    };
    out.push_str(&format!("\n## Next\n\n{next_step}\n"));
    out
}

fn quoted_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("\"{}\"", value))
        .collect::<Vec<_>>()
        .join(", ")
}

fn config_feature_set(raw: &str) -> HashSet<&str> {
    raw.split(',')
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .map(|part| part.trim_matches('"'))
        .filter(|part| !part.is_empty())
        .collect()
}

fn extract_template_tokens(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut rest = input;

    while let Some(start) = rest.find("{{") {
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            break;
        };
        let token = after_start[..end].trim();
        if !token.is_empty() && !tokens.iter().any(|existing| existing == token) {
            tokens.push(token.to_string());
        }
        rest = &after_start[end + 2..];
    }

    tokens
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
        } else if matches!(ch, '-' | '_' | ' ') && !slug.ends_with('-') {
            slug.push('-');
        }
    }

    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "megagate-project".to_string()
    } else {
        trimmed.to_string()
    }
}

/// ponytail: board registry tĩnh P1 — add board vào đây; P2 chuyển assets/boards/*.json
fn iot_target_for_board(board: &str) -> String {
    match board {
        "esp32" => "xtensa-esp32-none-elf".to_string(),
        "esp32s3" => "xtensa-esp32s3-none-elf".to_string(),
        "nrf52dk_nrf52832" => "thumbv7em-none-eabihf".to_string(),
        "stm32f4_disc" => "thumbv7em-none-eabihf".to_string(),
        _ => "riscv32imac-unknown-none-elf".to_string(),
    }
}

#[allow(dead_code)]
fn normalize_web_framework(framework: &str) -> String {
    match framework {
        "next" | "next-app" => "nextjs".to_string(),
        "react-express" | "react-hono" | "react-nestjs" | "react-trpc" => framework.to_string(),
        "vue-express" | "vue-hono" | "vue-nestjs" => framework.to_string(),
        "svelte-express" | "svelte-hono" => framework.to_string(),
        // Non-Node backend combos pass through unchanged
        "react-axum" | "react-actix-web" | "react-gin" | "react-echo" | "react-fiber"
        | "react-fastapi" | "react-django" | "react-flask" | "react-quarkus" | "react-symfony"
        | "vue-axum" | "vue-actix-web" | "vue-gin" | "vue-echo" | "vue-fiber" | "vue-fastapi"
        | "vue-django" | "vue-flask" | "vue-quarkus" | "vue-symfony" | "svelte-axum"
        | "svelte-gin" | "svelte-fastapi" | "svelte-quarkus" => framework.to_string(),
        other => other.to_string(),
    }
}

pub fn is_all_in_one_fullstack(framework: &str) -> bool {
    matches!(framework, "nextjs" | "nuxt" | "sveltekit" | "remix")
}

fn effective_web_mode(config: &ScaffoldConfig) -> String {
    if !config.sub_type.is_empty() {
        return config.sub_type.clone();
    }

    if config.frameworks.len() >= 2 {
        return "backend".to_string();
    }

    let framework = config
        .frameworks
        .first()
        .map(String::as_str)
        .unwrap_or("vanilla");
    if let Some(cfg) = WEB_FRAMEWORKS.iter().find(|f| f.name == framework) {
        cfg.sub_type.to_string()
    } else if infer_backend_language(framework).is_some() {
        "backend".to_string()
    } else {
        "frontend".to_string()
    }
}

pub fn infer_backend_language(framework: &str) -> Option<&'static str> {
    WEB_FRAMEWORKS
        .iter()
        .find(|f| f.name == framework)
        .and_then(|f| f.base)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Registry-first: template layer cần fetch/cache sẵn (~/.mg/templates hoặc
    /// MG_TEMPLATES_DIR). Máy sạch offline → skip test materialize.
    fn template_layer_ready(rel: &str) -> bool {
        let root = crate::scaffold::template_root::TemplateRoot::resolve(rel);
        root.exists("template.toml") && root.exists("sources")
    }

    #[test]
    fn test_disk_template_root_reads_manifest() {
        use crate::scaffold::template_root::TemplateRoot;
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("sources")).unwrap();
        std::fs::write(dir.path().join("template.toml"), "[files]\n").unwrap();
        let root = TemplateRoot::disk(dir.path().to_path_buf());
        let bytes = root.read("template.toml").unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("files"), "manifest has files");
        assert!(root.exists("sources"), "sources dir visible");
    }

    #[test]
    fn test_scaffold_writes_baseline_for_all_cores() {
        if !template_layer_ready("web/frontend/react-vite") {
            eprintln!("skipped: web/frontend/react-vite template layer not available offline (registry-first)");
            return;
        }
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
            if core == "web" {
                assert!(out.join("mg.lock").exists(), "web mg.lock");
                assert!(out.join("mg.toml").exists(), "web mg.toml");
            }
        }
    }

    #[test]
    fn test_display_name_uses_last_path_segment() {
        let path = Path::new("/tmp/my-project");
        assert_eq!(Scaffolder::display_name(path), "my-project");
    }

    #[test]
    fn test_lib_templates_materialize_all_languages() {
        if !template_layer_ready("lib/ts") {
            eprintln!("skipped: lib/ts template layer not available offline (registry-first)");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        for (language, manifest, marker) in [
            ("ts", "package.json", "\"core\""),
            ("rust", "Cargo.toml", "core = \"lib\""),
            ("python", "pyproject.toml", "core = \"lib\""),
        ] {
            let project_dir = root.path().join(format!("demo-{language}"));
            let config = ScaffoldConfig {
                core: "lib".to_string(),
                sub_type: String::new(),
                frameworks: vec![language.to_string()],
                project_name: project_dir.to_string_lossy().to_string(),
                features: vec![],
                template_dir: PathBuf::new(),
            };

            let out = Scaffolder::scaffold(&config).unwrap();
            assert_eq!(out, project_dir);
            assert!(out.join(manifest).exists(), "{language} manifest");
            let mg = std::fs::read_to_string(out.join("mg.toml")).unwrap();
            assert!(
                mg.contains("ecosystem = \"lib\""),
                "{language} mg.toml ecosystem"
            );
            assert!(
                mg.contains(&format!("language = \"{language}\"")),
                "{language} language"
            );
            let native = std::fs::read_to_string(out.join(manifest)).unwrap();
            assert!(native.contains(marker), "{language} marker");
        }
        let py_src = root
            .path()
            .join("demo-python")
            .join("src")
            .join("demo_python")
            .join("__init__.py");
        assert!(py_src.exists(), "python package source");
    }

    #[test]
    fn test_game_templates_materialize_all_engines() {
        if !template_layer_ready("game/bevy") {
            eprintln!("skipped: game/bevy template layer not available offline (registry-first)");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        for (framework, manifest) in [
            ("bevy", "Cargo.toml"),
            ("godot", "project.godot"),
            ("unity", "Packages/manifest.json"),
            ("unreal", "demo-unreal.uproject"),
        ] {
            let project_dir = root.path().join(format!("demo-{framework}"));
            let config = ScaffoldConfig {
                core: "game".to_string(),
                sub_type: String::new(),
                frameworks: vec![framework.to_string()],
                project_name: project_dir.to_string_lossy().to_string(),
                features: vec![],
                template_dir: PathBuf::new(),
            };

            let out = Scaffolder::scaffold(&config).unwrap();
            assert_eq!(out, project_dir);
            assert!(out.join(manifest).exists(), "{framework} manifest");
            let mg = std::fs::read_to_string(out.join("mg.toml")).unwrap();
            assert!(
                mg.contains("ecosystem = \"game\""),
                "{framework} mg.toml ecosystem"
            );
            assert!(
                mg.contains(&format!("engine = \"{framework}\"")),
                "{framework} engine"
            );
        }
        let bevy_src = root.path().join("demo-bevy").join("src").join("main.rs");
        assert!(bevy_src.exists(), "bevy source");
    }

    #[test]
    fn test_iot_templates_materialize_all_frameworks() {
        if !template_layer_ready("iot/esp32-rust") {
            eprintln!(
                "skipped: iot/esp32-rust template layer not available offline (registry-first)"
            );
            return;
        }
        let root = tempfile::tempdir().unwrap();
        for (framework, manifest, marker, board, fw_check) in [
            (
                "esp32-rust",
                "Cargo.toml",
                "esp32-hal",
                "esp32c3",
                "esp32-rust",
            ),
            (
                "platformio",
                "platformio.ini",
                "esp32dev",
                "esp32dev",
                "platformio",
            ),
            (
                "zephyr-arm",
                "west.yml",
                "zephyr",
                "nrf52dk_nrf52832",
                "zephyr",
            ),
        ] {
            let project_dir = root.path().join(format!("demo-{framework}"));
            let config = ScaffoldConfig {
                core: "iot".to_string(),
                sub_type: String::new(),
                frameworks: vec![framework.to_string()],
                project_name: project_dir.to_string_lossy().to_string(),
                features: vec![board.to_string()],
                template_dir: PathBuf::new(),
            };

            let out = Scaffolder::scaffold(&config).unwrap();
            assert_eq!(out, project_dir);
            assert!(out.join(manifest).exists(), "{framework} manifest");
            let mg = std::fs::read_to_string(out.join("mg.toml")).unwrap();
            assert!(
                mg.contains("ecosystem = \"iot\""),
                "{framework} mg.toml ecosystem"
            );
            assert!(
                mg.contains(&format!("framework = \"{fw_check}\"")),
                "{framework} framework"
            );
            assert!(mg.contains(board), "{framework} board");
            if framework == "esp32-rust" {
                assert!(
                    mg.contains("riscv32imac-unknown-none-elf"),
                    "{framework} target"
                );
            }
            let native = std::fs::read_to_string(out.join(manifest)).unwrap();
            assert!(native.contains(marker), "{framework} marker");
        }
        let esp32_src = root
            .path()
            .join("demo-esp32-rust")
            .join("src")
            .join("main.rs");
        assert!(esp32_src.exists(), "esp32-rust source");
    }

    #[test]
    fn test_optimizer_template_materializes() {
        if !template_layer_ready("hardware/optimizer") {
            eprintln!(
                "skipped: hardware/optimizer template layer not available offline (registry-first)"
            );
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let optimizer_dir = root.path().join("optimizer");
        let config = ScaffoldConfig {
            core: "hardware".to_string(),
            sub_type: String::new(),
            frameworks: vec!["optimizer".to_string()],
            project_name: optimizer_dir.to_string_lossy().to_string(),
            features: vec![],
            template_dir: PathBuf::new(),
        };

        let out = Scaffolder::scaffold(&config).unwrap();
        assert_eq!(out, optimizer_dir);
        assert!(out.join("Cargo.toml").exists(), "optimizer Cargo.toml");
        assert!(out.join("src").join("lib.rs").exists(), "optimizer lib.rs");
        assert!(out.join("build.rs").exists(), "optimizer build.rs");
        assert!(
            out.join("shaders").join("compute.wgsl").exists(),
            "optimizer shader"
        );
        let cargo = std::fs::read_to_string(out.join("Cargo.toml")).unwrap();
        assert!(
            cargo.contains("name = \"mg-optimizer\""),
            "fixed package name"
        );
        assert!(
            cargo.contains("[workspace]"),
            "workspace opt-out for nested crates"
        );
        let lib = std::fs::read_to_string(out.join("src").join("lib.rs")).unwrap();
        assert!(lib.contains("mg_optimizer_init"), "FFI init export");
        assert!(
            lib.contains("mg_optimizer_optimize_mesh"),
            "FFI mesh export"
        );
    }

    #[test]
    fn test_web_monorepo_uses_template_layers() {
        if !["web/frontend/react-vite", "web/backend/node/fastify"]
            .iter()
            .all(|rel| template_layer_ready(rel))
        {
            eprintln!("skipped: web template layers not available offline (registry-first)");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let project_dir = root.path().join("web-monorepo");
        let config = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "monorepo".to_string(),
            frameworks: vec!["react-vite".to_string(), "fastify".to_string()],
            project_name: project_dir.to_string_lossy().to_string(),
            features: vec!["schema".to_string()],
            template_dir: PathBuf::new(),
        };

        let out = Scaffolder::scaffold(&config).unwrap();
        assert!(out.join("mg.lock").exists());
        assert!(out.join("mg.toml").exists());
        assert!(out.join("megagate.workspace.toml").exists());
        let root_package = std::fs::read_to_string(out.join("package.json")).unwrap();
        assert!(root_package.contains("\"dev\": \"mg --core web dev\""));
        assert!(!root_package.contains("mg web build"));
        assert!(!root_package.contains("mg web check"));
        let readme = std::fs::read_to_string(out.join("README.md")).unwrap();
        assert!(readme.contains("mg install-web"));
        assert!(out.join("apps").join("frontend").join("README.md").exists());
        assert!(out.join("apps").join("backend").join("README.md").exists());
        assert!(out.join("packages").join("README.md").exists());
        assert!(out
            .join("apps")
            .join("frontend")
            .join("crates")
            .join("engine")
            .join("Cargo.toml")
            .exists());
        assert!(out
            .join("apps")
            .join("frontend")
            .join("src")
            .join("bridges")
            .join("engine.js")
            .exists());
        assert!(out
            .join("packages")
            .join("contracts")
            .join("package.json")
            .exists());
        assert!(out
            .join("apps")
            .join("frontend")
            .join("vite.config.js")
            .exists());
        assert!(out
            .join("apps")
            .join("backend")
            .join("src")
            .join("server.js")
            .exists());
    }

    #[test]
    fn test_fullstack_axum_falls_back_to_monorepo_composite() {
        if ![
            "web/frontend/react-vite",
            "web/monorepo/base",
            "web/monorepo/frontend/react-vite",
            "web/monorepo/backend/rust/axum",
        ]
        .iter()
        .all(|rel| template_layer_ready(rel))
        {
            eprintln!("skipped: web template layers not available offline (registry-first)");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let project_dir = root.path().join("react-axum-app");
        let config = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "fullstack".to_string(),
            frameworks: vec!["react-axum".to_string()],
            project_name: project_dir.to_string_lossy().to_string(),
            features: vec![],
            template_dir: PathBuf::new(),
        };

        let out = Scaffolder::scaffold(&config).unwrap();
        assert!(out.join("megagate.workspace.toml").exists());
        assert!(out
            .join("apps")
            .join("frontend")
            .join("package.json")
            .exists());
        let back_cargo =
            std::fs::read_to_string(out.join("apps").join("backend").join("Cargo.toml")).unwrap();
        assert!(
            back_cargo.contains("axum"),
            "backend Cargo.toml should pin axum, got: {back_cargo}"
        );
        assert!(out
            .join("apps")
            .join("backend")
            .join("src")
            .join("main.rs")
            .exists());
        assert!(
            !out.join("templates")
                .join("web")
                .join("fullstack")
                .join("split")
                .join("react-axum")
                .exists(),
            "no hardcoded split leaf was added"
        );
    }

    #[test]
    fn test_fullstack_gin_falls_back_to_monorepo_composite() {
        if ![
            "web/frontend/react-vite",
            "web/monorepo/base",
            "web/monorepo/frontend/react-vite",
            "web/monorepo/backend/go/gin",
        ]
        .iter()
        .all(|rel| template_layer_ready(rel))
        {
            eprintln!("skipped: web template layers not available offline (registry-first)");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let project_dir = root.path().join("react-gin-app");
        let config = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "fullstack".to_string(),
            frameworks: vec!["react-gin".to_string()],
            project_name: project_dir.to_string_lossy().to_string(),
            features: vec![],
            template_dir: PathBuf::new(),
        };

        let out = Scaffolder::scaffold(&config).unwrap();
        assert!(out.join("megagate.workspace.toml").exists());
        assert!(out.join("apps").join("backend").join("go.mod").exists());
        let go_mod =
            std::fs::read_to_string(out.join("apps").join("backend").join("go.mod")).unwrap();
        assert!(
            go_mod.contains("gin"),
            "backend go.mod should pin gin, got: {go_mod}"
        );
    }

    #[test]
    fn test_web_leaf_templates_materialize_framework_specific_files() {
        if ![
            "web/frontend/react-vite",
            "web/frontend/nextjs",
            "web/frontend/vue-vite",
            "web/frontend/vanilla",
            "web/frontend/solidjs",
            "web/fullstack/split/react-express",
        ]
        .iter()
        .all(|rel| template_layer_ready(rel))
        {
            eprintln!("skipped: web template layers not available offline (registry-first)");
            return;
        }
        let root = tempfile::tempdir().unwrap();

        let react_dir = root.path().join("react-vite-app");
        let react = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "frontend".to_string(),
            frameworks: vec!["react-vite".to_string()],
            project_name: react_dir.to_string_lossy().to_string(),
            features: vec![],
            template_dir: PathBuf::new(),
        };
        let react_out = Scaffolder::scaffold(&react).unwrap();
        assert!(react_out.join("package.json").exists());
        assert!(react_out.join("vite.config.js").exists());
        assert!(react_out.join("index.html").exists());
        assert!(react_out
            .join("crates")
            .join("engine")
            .join("Cargo.toml")
            .exists());
        assert!(react_out.join("src").join("main.jsx").exists());
        assert!(react_out.join("src").join("App.jsx").exists());
        assert!(react_out
            .join("src")
            .join("bridges")
            .join("engine.js")
            .exists());
        assert!(react_out
            .join("src")
            .join("styles")
            .join("theme.css")
            .exists());
        assert!(react_out
            .join("src")
            .join("assets")
            .join("megagate-grid.svg")
            .exists());
        assert!(!react_out.join("tsconfig.json").exists());

        let next_dir = root.path().join("next-app");
        let next = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "frontend".to_string(),
            frameworks: vec!["nextjs".to_string()],
            project_name: next_dir.to_string_lossy().to_string(),
            features: vec![],
            template_dir: PathBuf::new(),
        };
        let next_out = Scaffolder::scaffold(&next).unwrap();
        assert!(next_out.join("next.config.mjs").exists());
        assert!(next_out
            .join("crates")
            .join("engine")
            .join("Cargo.toml")
            .exists());
        assert!(next_out.join("src").join("app").join("page.jsx").exists());
        assert!(next_out
            .join("src")
            .join("bridges")
            .join("engine.js")
            .exists());
        assert!(next_out.join("jsconfig.json").exists());
        assert!(!next_out.join("src").join("main.tsx").exists());

        let vue_dir = root.path().join("vue-app");
        let vue = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "frontend".to_string(),
            frameworks: vec!["vue-vite".to_string()],
            project_name: vue_dir.to_string_lossy().to_string(),
            features: vec!["typescript".to_string()],
            template_dir: PathBuf::new(),
        };
        let vue_out = Scaffolder::scaffold(&vue).unwrap();
        assert!(vue_out.join("package.json").exists());
        assert!(vue_out.join("vite.config.ts").exists());
        assert!(vue_out.join("src").join("main.ts").exists());
        assert!(vue_out.join("src").join("App.vue").exists());
        assert!(vue_out
            .join("src")
            .join("components")
            .join("AppShell.vue")
            .exists());
        assert!(vue_out
            .join("src")
            .join("router")
            .join("AppRouter.vue")
            .exists());
        assert!(vue_out
            .join("src")
            .join("hooks")
            .join("useProjectLinks.ts")
            .exists());
        assert!(!vue_out
            .join("src")
            .join("components")
            .join("AppShell.tsx")
            .exists());

        let vanilla_dir = root.path().join("vanilla-app");
        let vanilla = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "frontend".to_string(),
            frameworks: vec!["vanilla".to_string()],
            project_name: vanilla_dir.to_string_lossy().to_string(),
            features: vec!["typescript".to_string()],
            template_dir: PathBuf::new(),
        };
        let vanilla_out = Scaffolder::scaffold(&vanilla).unwrap();
        assert!(vanilla_out.join("package.json").exists());
        assert!(vanilla_out.join("vite.config.ts").exists());
        assert!(vanilla_out.join("src").join("main.ts").exists());
        assert!(vanilla_out.join("src").join("App.ts").exists());
        assert!(vanilla_out
            .join("src")
            .join("components")
            .join("AppShell.ts")
            .exists());
        assert!(vanilla_out
            .join("src")
            .join("router")
            .join("AppRouter.ts")
            .exists());
        assert!(!vanilla_out.join("src").join("main.tsx").exists());

        let react_express_dir = root.path().join("react-express-app");
        let react_express = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "fullstack".to_string(),
            frameworks: vec!["react-express".to_string()],
            project_name: react_express_dir.to_string_lossy().to_string(),
            features: vec!["typescript".to_string()],
            template_dir: PathBuf::new(),
        };
        let react_express_out = Scaffolder::scaffold(&react_express).unwrap();
        assert!(react_express_out.join("package.json").exists());
        assert!(react_express_out.join("vite.config.ts").exists());
        assert!(react_express_out.join("src").join("main.tsx").exists());
        assert!(react_express_out
            .join("src")
            .join("styles")
            .join("theme.css")
            .exists());
        assert!(react_express_out
            .join("server")
            .join("src")
            .join("server.ts")
            .exists());

        let solid_dir = root.path().join("solid-app");
        let solid = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "frontend".to_string(),
            frameworks: vec!["solidjs".to_string()],
            project_name: solid_dir.to_string_lossy().to_string(),
            features: vec!["typescript".to_string()],
            template_dir: PathBuf::new(),
        };
        let solid_out = Scaffolder::scaffold(&solid).unwrap();
        assert!(solid_out.join("package.json").exists());
        assert!(solid_out.join("vite.config.ts").exists());
        assert!(solid_out.join("src").join("main.tsx").exists());
        assert!(solid_out.join("src").join("App.tsx").exists());
        assert!(solid_out
            .join("src")
            .join("components")
            .join("AppShell.tsx")
            .exists());
        assert!(solid_out
            .join("src")
            .join("router")
            .join("AppRouter.tsx")
            .exists());

        let fastify_dir = root.path().join("fastify-api");
        let fastify = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "backend".to_string(),
            frameworks: vec!["node".to_string(), "fastify".to_string()],
            project_name: fastify_dir.to_string_lossy().to_string(),
            features: vec![],
            template_dir: PathBuf::new(),
        };
        let fastify_out = Scaffolder::scaffold(&fastify).unwrap();
        assert!(fastify_out.join("src").join("server.js").exists());
        assert!(fastify_out
            .join("src")
            .join("config")
            .join("app.js")
            .exists());
        assert!(fastify_out
            .join("src")
            .join("routes")
            .join("health.js")
            .exists());
        assert!(fastify_out
            .join("src")
            .join("services")
            .join("status.js")
            .exists());
        assert!(!fastify_out.join("tsconfig.json").exists());
    }

    #[test]
    fn test_web_typescript_feature_switches_extensions() {
        if !template_layer_ready("web/frontend/react-vite") {
            eprintln!("skipped: web/frontend/react-vite template layer not available offline (registry-first)");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let project_dir = root.path().join("react-ts");
        let config = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "frontend".to_string(),
            frameworks: vec!["react-vite".to_string()],
            project_name: project_dir.to_string_lossy().to_string(),
            features: vec!["typescript".to_string()],
            template_dir: PathBuf::new(),
        };

        let out = Scaffolder::scaffold(&config).unwrap();
        assert!(out.join("tsconfig.json").exists());
        assert!(out.join("vite.config.ts").exists());
        assert!(out
            .join("crates")
            .join("engine")
            .join("Cargo.toml")
            .exists());
        assert!(out.join("src").join("main.tsx").exists());
        assert!(out.join("src").join("App.tsx").exists());
        assert!(out.join("src").join("bridges").join("engine.ts").exists());
        assert!(!out.join("src").join("main.jsx").exists());
    }

    #[test]
    fn test_unknown_frameworks_fail_fast() {
        let root = tempfile::tempdir().unwrap();

        let unsupported_dir = root.path().join("ember-app");
        let unsupported = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "frontend".to_string(),
            frameworks: vec!["ember".to_string()],
            project_name: unsupported_dir.to_string_lossy().to_string(),
            features: vec!["typescript".to_string()],
            template_dir: PathBuf::new(),
        };
        let unsupported_err = Scaffolder::scaffold(&unsupported).unwrap_err();
        assert!(unsupported_err.to_string().contains("Web template path"));

        let broken_mono_dir = root.path().join("broken-mono");
        let broken_mono = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "monorepo".to_string(),
            frameworks: vec!["ember".to_string(), "fastify".to_string()],
            project_name: broken_mono_dir.to_string_lossy().to_string(),
            features: vec!["typescript".to_string()],
            template_dir: PathBuf::new(),
        };
        let broken_mono_err = Scaffolder::scaffold(&broken_mono).unwrap_err();
        assert!(broken_mono_err
            .to_string()
            .contains("Scaffold for monorepo frontend framework 'ember' is not implemented yet"));
    }

    #[test]
    fn test_web_feature_gated_templates_materialize_only_when_active() {
        if !template_layer_ready("web/frontend/nextjs") {
            eprintln!("skipped: web/frontend/nextjs template layer not available offline (registry-first)");
            return;
        }
        let root = tempfile::tempdir().unwrap();

        // Next.js with prisma + tailwindcss + eslint + prettier + vitest
        let with_features = root.path().join("next-features");
        let config = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "frontend".to_string(),
            frameworks: vec!["nextjs".to_string()],
            project_name: with_features.to_string_lossy().to_string(),
            features: vec![
                "typescript".to_string(),
                "prisma".to_string(),
                "tailwindcss".to_string(),
                "daisyui".to_string(),
                "eslint".to_string(),
                "prettier".to_string(),
                "vitest".to_string(),
            ],
            template_dir: PathBuf::new(),
        };
        let out = Scaffolder::scaffold(&config).unwrap();
        assert!(out.join("prisma").join("schema.prisma").exists());
        assert!(out.join("tailwind.config.ts").exists());
        assert!(out.join("postcss.config.mjs").exists());
        assert!(out.join(".eslintrc.json").exists());
        assert!(out.join(".prettierrc").exists());
        assert!(out.join("vitest.config.ts").exists());

        // Next.js without features — feature files must NOT exist
        let no_features = root.path().join("next-bare");
        let config_bare = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "frontend".to_string(),
            frameworks: vec!["nextjs".to_string()],
            project_name: no_features.to_string_lossy().to_string(),
            features: vec!["typescript".to_string()],
            template_dir: PathBuf::new(),
        };
        let out_bare = Scaffolder::scaffold(&config_bare).unwrap();
        assert!(!out_bare.join("prisma").join("schema.prisma").exists());
        assert!(!out_bare.join("tailwind.config.ts").exists());
        assert!(!out_bare.join(".eslintrc.json").exists());
        assert!(!out_bare.join(".prettierrc").exists());
        assert!(!out_bare.join("vitest.config.ts").exists());
    }

    #[test]
    fn test_web_docker_templates_materialize_in_base_layer() {
        if !template_layer_ready("web/frontend/nextjs") {
            eprintln!("skipped: web/frontend/nextjs template layer not available offline (registry-first)");
            return;
        }
        let root = tempfile::tempdir().unwrap();

        // Frontend with docker feature
        let docker_dir = root.path().join("docker-app");
        let config = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "frontend".to_string(),
            frameworks: vec!["nextjs".to_string()],
            project_name: docker_dir.to_string_lossy().to_string(),
            features: vec!["docker".to_string()],
            template_dir: PathBuf::new(),
        };
        let out = Scaffolder::scaffold(&config).unwrap();
        assert!(out.join("Dockerfile").exists());
        assert!(out.join("docker-compose.yml").exists());
        assert!(out.join(".dockerignore").exists());

        // Without docker — no docker files
        let no_docker = root.path().join("no-docker");
        let config_no = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "frontend".to_string(),
            frameworks: vec!["nextjs".to_string()],
            project_name: no_docker.to_string_lossy().to_string(),
            features: vec!["typescript".to_string()],
            template_dir: PathBuf::new(),
        };
        let out_no = Scaffolder::scaffold(&config_no).unwrap();
        assert!(!out_no.join("Dockerfile").exists());
        assert!(!out_no.join("docker-compose.yml").exists());
    }

    #[test]
    fn test_web_postgres_env_template_materializes_with_feature() {
        if !template_layer_ready("web/frontend/nextjs") {
            eprintln!("skipped: web/frontend/nextjs template layer not available offline (registry-first)");
            return;
        }
        let root = tempfile::tempdir().unwrap();

        let pg_dir = root.path().join("pg-app");
        let config = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "frontend".to_string(),
            frameworks: vec!["nextjs".to_string()],
            project_name: pg_dir.to_string_lossy().to_string(),
            features: vec!["postgres".to_string()],
            template_dir: PathBuf::new(),
        };
        let out = Scaffolder::scaffold(&config).unwrap();
        assert!(out.join(".env").exists());

        let no_pg = root.path().join("no-pg");
        let config_no = ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "frontend".to_string(),
            frameworks: vec!["nextjs".to_string()],
            project_name: no_pg.to_string_lossy().to_string(),
            features: vec![],
            template_dir: PathBuf::new(),
        };
        let out_no = Scaffolder::scaffold(&config_no).unwrap();
        assert!(!out_no.join(".env").exists());
    }
}
