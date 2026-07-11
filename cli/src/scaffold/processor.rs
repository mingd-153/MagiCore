use anyhow::Result;
use serde::Deserialize;
use std::collections::HashSet;
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
        let mode = effective_web_mode(config);

        let dir = match mode.as_str() {
            "frontend" => root.join("frontend").join(framework),
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

    fn resolve_web_template_layers(config: &ScaffoldConfig) -> Result<Vec<PathBuf>> {
        let root = Self::web_templates_root();
        let mode = effective_web_mode(config);
        let mut layers = vec![root.join("shared").join("partials").join("base")];

        match mode.as_str() {
            "frontend" | "backend" | "fullstack" => {
                let leaf = Self::resolve_web_template_dir(config)?;
                let fallback = root.join("shared").join("partials").join(mode.as_str());
                if Self::layer_has_contract(&leaf) {
                    layers.push(leaf);
                } else {
                    layers.push(fallback);
                    layers.push(leaf);
                }
            }
            "monorepo" => {
                layers.push(root.join("shared").join("partials").join("monorepo"));
                let frontend = config.frameworks.first().cloned().unwrap_or_default();
                let backend = config.frameworks.get(1).cloned().unwrap_or_default();
                let backend_language = infer_backend_language(&backend)
                    .ok_or_else(|| anyhow::anyhow!("Unsupported monorepo backend '{backend}'"))?;

                layers.push(root.join("monorepo").join("base"));
                layers.push(root.join("monorepo").join("frontend").join(frontend));
                layers.push(
                    root.join("monorepo")
                        .join("backend")
                        .join(backend_language)
                        .join(backend),
                );
                layers.push(root.join("monorepo").join("packages"));
            }
            other => anyhow::bail!("Unsupported web mode '{other}'"),
        }

        for layer in &layers {
            if !layer.exists() {
                anyhow::bail!("Web template layer '{}' does not exist", layer.display());
            }
        }

        Ok(layers)
    }

    fn layer_has_contract(layer: &Path) -> bool {
        layer.join("template.toml").exists() && layer.join("sources").exists()
    }

    fn materialize_web_templates(
        target: &Path,
        config: &ScaffoldConfig,
        layers: &[PathBuf],
    ) -> Result<()> {
        let context = WebTemplateContext::new(config, layers);
        for layer in layers {
            Self::materialize_template_layer(target, layer, &context)?;
        }
        Ok(())
    }

    fn materialize_template_layer(
        target: &Path,
        layer: &Path,
        context: &WebTemplateContext,
    ) -> Result<()> {
        let Some(manifest) = TemplateManifest::load(layer)? else {
            return Ok(());
        };
        let mut seen_targets = HashSet::new();
        for file in &manifest.files {
            if !seen_targets.insert(file.target.clone()) {
                anyhow::bail!(
                    "Duplicate template target '{}' in '{}'",
                    file.target,
                    layer.display()
                );
            }
        }

        for file in &manifest.files {
            let source_path = layer.join("sources").join(&file.source);
            if !source_path.exists() {
                anyhow::bail!(
                    "Template source '{}' does not exist in '{}'",
                    file.source,
                    layer.display()
                );
            }

            let contents = std::fs::read_to_string(&source_path)?;
            let rendered =
                context.render_with_contract(&contents, &file.required_context, &source_path)?;
            Self::write_file(&target.join(&file.target), &rendered)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct WebTemplateContext {
    project_name: String,
    project_slug: String,
    mode: String,
    framework: String,
    frameworks: String,
    frontend_framework: String,
    backend_framework: String,
    backend_language: String,
    template: String,
    features: String,
}

impl WebTemplateContext {
    fn new(config: &ScaffoldConfig, layers: &[PathBuf]) -> Self {
        let primary_template = if config.sub_type == "monorepo" {
            layers
                .iter()
                .filter_map(|layer| {
                    layer
                        .strip_prefix(Scaffolder::workspace_root())
                        .ok()
                        .map(|p| p.display().to_string())
                })
                .filter(|layer| !layer.starts_with("templates/web/shared/partials"))
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            Scaffolder::resolve_web_template_dir(config)
                .ok()
                .and_then(|dir| {
                    dir.strip_prefix(Scaffolder::workspace_root())
                        .ok()
                        .map(|p| p.display().to_string())
                })
                .unwrap_or_default()
        };

        let project_name = config.project_name.clone();
        let project_slug = slugify(&project_name);
        let mode = effective_web_mode(config);
        let framework = Scaffolder::framework(config);
        let frontend_framework = match mode.as_str() {
            "monorepo" => config.frameworks.first().cloned().unwrap_or_default(),
            "frontend" => framework.clone(),
            _ => String::new(),
        };
        let backend_framework = match mode.as_str() {
            "backend" => framework.clone(),
            "monorepo" => config.frameworks.get(1).cloned().unwrap_or_default(),
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

        Self {
            project_name,
            project_slug,
            mode,
            framework,
            frameworks: quoted_list(&config.frameworks),
            frontend_framework,
            backend_framework,
            backend_language,
            template: primary_template,
            features: quoted_list(&config.features),
        }
    }

    fn value(&self, key: &str) -> Option<&str> {
        match key {
            "project_name" => Some(self.project_name.as_str()),
            "project_slug" => Some(self.project_slug.as_str()),
            "mode" => Some(self.mode.as_str()),
            "framework" => Some(self.framework.as_str()),
            "frameworks" => Some(self.frameworks.as_str()),
            "frontend_framework" => Some(self.frontend_framework.as_str()),
            "backend_framework" => Some(self.backend_framework.as_str()),
            "backend_language" => Some(self.backend_language.as_str()),
            "template" => Some(self.template.as_str()),
            "features" => Some(self.features.as_str()),
            _ => None,
        }
    }

    fn render_with_contract(
        &self,
        input: &str,
        required_context: &[String],
        source_path: &Path,
    ) -> Result<String> {
        let declared: HashSet<&str> = required_context.iter().map(String::as_str).collect();
        let used = extract_template_tokens(input);

        for token in &used {
            if !declared.contains(token.as_str()) {
                anyhow::bail!(
                    "Template token '{}' in '{}' is not declared in template.toml",
                    token,
                    source_path.display()
                );
            }
            if self.value(token).is_none() {
                anyhow::bail!(
                    "Template token '{}' in '{}' is not supported by the Rust compiler context",
                    token,
                    source_path.display()
                );
            }
        }

        for key in required_context {
            if self.value(key).is_none() {
                anyhow::bail!(
                    "Template context '{}' required by '{}' is not supported by the Rust compiler context",
                    key,
                    source_path.display()
                );
            }
        }

        let mut rendered = input.to_string();
        for key in required_context {
            if let Some(value) = self.value(key) {
                rendered = rendered.replace(&format!("{{{{{key}}}}}"), value);
            }
        }

        Ok(rendered)
    }
}

#[derive(Debug, Deserialize)]
struct TemplateManifest {
    files: Vec<TemplateFile>,
}

impl TemplateManifest {
    fn load(layer: &Path) -> Result<Option<Self>> {
        let manifest_path = layer.join("template.toml");
        if !manifest_path.exists() {
            if !layer.join("sources").exists() {
                return Ok(None);
            }
            anyhow::bail!("Missing template manifest '{}'", manifest_path.display());
        }

        let contents = std::fs::read_to_string(&manifest_path)?;
        Ok(Some(toml::from_str(&contents)?))
    }
}

#[derive(Debug, Deserialize)]
struct TemplateFile {
    source: String,
    target: String,
    #[serde(default)]
    required_context: Vec<String>,
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
    if matches!(
        framework,
        "remix" | "react-fastify" | "vue-laravel" | "react-spring" | "custom"
    ) {
        "fullstack".to_string()
    } else if infer_backend_language(framework).is_some() {
        "backend".to_string()
    } else {
        "frontend".to_string()
    }
}

fn infer_backend_language(framework: &str) -> Option<&'static str> {
    match framework {
        "express" | "fastify" | "nestjs" | "hono" | "trpc" => Some("node"),
        "laravel" | "symfony" => Some("php"),
        "spring-boot" | "quarkus" => Some("java"),
        "gin" | "echo" | "fiber" => Some("go"),
        "fastapi" | "django" | "flask" => Some("python"),
        "axum" | "actix-web" => Some("rust"),
        _ => None,
    }
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
            if core == "web" {
                assert!(out.join("mg.lock").exists(), "web mg.lock");
                assert!(
                    out.join(".megagate").join("web.toml").exists(),
                    "web manifest"
                );
            }
        }
    }

    #[test]
    fn test_display_name_uses_last_path_segment() {
        let path = Path::new("/tmp/my-project");
        assert_eq!(Scaffolder::display_name(path), "my-project");
    }

    #[test]
    fn test_web_monorepo_uses_template_layers() {
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
        assert!(out.join(".megagate").join("web.toml").exists());
        assert!(out.join("megagate.workspace.toml").exists());
        assert!(out.join("apps").join("frontend").join("README.md").exists());
        assert!(out.join("apps").join("backend").join("README.md").exists());
        assert!(out.join("packages").join("README.md").exists());
        assert!(out
            .join("apps")
            .join("frontend")
            .join("vite.config.ts")
            .exists());
        assert!(out
            .join("apps")
            .join("backend")
            .join("src")
            .join("server.ts")
            .exists());
    }

    #[test]
    fn test_web_leaf_templates_materialize_framework_specific_files() {
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
        assert!(react_out.join("vite.config.ts").exists());
        assert!(react_out.join("index.html").exists());
        assert!(react_out.join("src").join("main.ts").exists());
        assert!(react_out.join("src").join("App.ts").exists());

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
        assert!(next_out.join("src").join("app").join("page.tsx").exists());
        assert!(!next_out.join("src").join("main.ts").exists());

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
        assert!(fastify_out.join("tsconfig.json").exists());
        assert!(fastify_out.join("src").join("server.ts").exists());
        assert!(!fastify_out.join("src").join("server.txt").exists());
    }
}
