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
            .ok_or_else(|| crate::error::unsupported_web_framework(&framework))?;

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
            return Err(crate::error::dir_already_exists(&target));
        }

        std::fs::create_dir_all(&target)?;
        Self::write_common_files(&target, config)?;
        Self::write_core_files(&target, config)?;

        mgc_ui::success(&format!(
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
        let root = TemplateRoot::resolve("web");
        // Workspace disk may hold only a partial templates/web (e.g. shared
        // partials) while the full tree lives in the registry cache — prefer
        // the source that actually has the shared base contract.
        if root.exists("shared") {
            return root;
        }
        let cached =
            TemplateRoot::disk(crate::commands::template::templates_cache_dir().join("web"));
        if cached.exists("shared") {
            return cached;
        }
        root
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
            "game" => super::processors::game::GameProcessor::files(target, &name, &framework),
            "ai" => super::processors::ai::AiProcessor::files(target, &name, &framework),
            "clo" => super::processors::clo::CloProcessor::files(target, &name, &framework),
            "cicd" => super::processors::cicd::CicdProcessor::files(target, &name, &framework),
            "iot" => super::processors::iot::IotProcessor::files(target, &name, &framework),
            "app" => {
                if framework == "multi" {
                    super::processors::app::AppProcessor::files_multi(target, &name)
                } else {
                    super::processors::app::AppProcessor::files(target, &name, &framework)
                }
            }
            "lib" => super::processors::lib::LibProcessor::files(target, &name, &framework),
            other => Err(crate::error::unsupported_scaffold_core(other)),
        }
    }

    fn write_web_files(
        target: &Path,
        config: &ScaffoldConfig,
        name: &str,
        framework: &str,
    ) -> Result<()> {
        match Self::resolve_web_template_layers(config) {
            Ok(layers) => Self::materialize_web_templates(target, config, &layers),
            Err(err) => {
                if let Some(files) =
                    crate::scaffold::embedded_kernel::get_embedded_template("web", framework)
                {
                    Self::ensure_web_fallback_common_files(target, name, framework)?;
                    return crate::scaffold::embedded_kernel::materialize_embedded(
                        target, name, &files,
                    );
                }
                if effective_web_mode(config) == "backend" {
                    if let Some(language) = infer_backend_language(framework) {
                        Self::ensure_web_fallback_common_files(target, name, framework)?;
                        return Self::write_minimal_backend_fallback(
                            target, name, framework, language,
                        );
                    }
                }
                Err(err)
            }
        }
    }

    fn ensure_web_fallback_common_files(target: &Path, name: &str, framework: &str) -> Result<()> {
        if !target.join(".gitignore").exists() {
            Self::write_file(
                &target.join(".gitignore"),
                &common_gitignore("web", framework),
            )?;
        }
        if !target.join("README.md").exists() {
            Self::write_file(
                &target.join("README.md"),
                &common_readme(name, "web", framework, &[]),
            )?;
        }
        Ok(())
    }

    fn write_minimal_backend_fallback(
        target: &Path,
        name: &str,
        framework: &str,
        language: &str,
    ) -> Result<()> {
        match language {
            "rust" => {
                Self::write_file(
                    &target.join("Cargo.toml"),
                    &format!(
                        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
                        slugify(name)
                    ),
                )?;
                Self::write_file(
                    &target.join("src/main.rs"),
                    &format!(
                        "fn main() {{\n    println!(\"MagiCore {framework} service ready on 127.0.0.1:4315\");\n}}\n"
                    ),
                )
            }
            "go" => {
                Self::write_file(
                    &target.join("go.mod"),
                    &format!("module {}\n\ngo 1.22\n", slugify(name)),
                )?;
                Self::write_file(
                    &target.join("main.go"),
                    &format!(
                        "package main\n\nimport \"fmt\"\n\nfunc main() {{\n\tfmt.Println(\"MagiCore {framework} service ready on 127.0.0.1:4315\")\n}}\n"
                    ),
                )
            }
            "python" => {
                Self::write_file(
                    &target.join("pyproject.toml"),
                    &format!(
                        "[project]\nname = \"{}\"\nversion = \"0.1.0\"\ndependencies = []\n",
                        slugify(name)
                    ),
                )?;
                Self::write_file(
                    &target.join("main.py"),
                    &format!("print('MagiCore {framework} service ready on 127.0.0.1:4315')\n"),
                )
            }
            "php" => {
                Self::write_file(
                    &target.join("composer.json"),
                    &format!(
                        "{{\n  \"name\": \"magicore/{}\",\n  \"type\": \"project\",\n  \"require\": {{}}\n}}\n",
                        slugify(name)
                    ),
                )?;
                Self::write_file(
                    &target.join("public/index.php"),
                    &format!(
                        "<?php\necho 'MagiCore {framework} service ready on 127.0.0.1:4315';\n"
                    ),
                )
            }
            "java" => {
                Self::write_file(
                    &target.join("pom.xml"),
                    &format!(
                        "<project><modelVersion>4.0.0</modelVersion><groupId>dev.magicore</groupId><artifactId>{}</artifactId><version>0.1.0</version></project>\n",
                        slugify(name)
                    ),
                )?;
                Self::write_file(
                    &target.join("src/main/java/App.java"),
                    &format!(
                        "public class App {{\n  public static void main(String[] args) {{\n    System.out.println(\"MagiCore {framework} service ready on 127.0.0.1:4315\");\n  }}\n}}\n"
                    ),
                )
            }
            "node" => {
                Self::write_file(
                    &target.join("package.json"),
                    &format!(
                        "{{\n  \"name\": \"{}\",\n  \"private\": true,\n  \"version\": \"0.1.0\",\n  \"type\": \"module\",\n  \"scripts\": {{ \"dev\": \"node src/server.js\" }}\n}}\n",
                        slugify(name)
                    ),
                )?;
                Self::write_file(
                    &target.join("src/server.js"),
                    &format!(
                        "console.log('MagiCore {framework} service ready on 127.0.0.1:4315')\n"
                    ),
                )
            }
            _ => Err(crate::error::unsupported_scaffold_framework(framework)),
        }
    }

    /// C9 — multi-platform: shared Kotlin (KMP) + android/ios (swift+objc)/react-native/flutter.
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
            return Err(crate::error::web_template_path_missing(&dir.logical_rel()));
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
                let leaf = layers
                    .pop()
                    .ok_or_else(|| crate::error::scaffold_not_implemented("web", "monorepo"))?;
                layers = Self::monorepo_layer_stack(&root, config, vec![leaf])?;
            }
            other => return Err(crate::error::unsupported_web_mode(other)),
        }

        for layer in &layers {
            if !layer.exists("") {
                return Err(crate::error::web_template_layer_missing(
                    &layer.logical_rel(),
                ));
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
                    return Err(crate::error::unsupported_fullstack_framework(combined));
                }
                (fe, be)
            }
            _ => return Err(crate::error::web_scaffold_needs_fe_be()),
        };
        let backend_language = infer_backend_language(&backend)
            .ok_or_else(|| crate::error::unsupported_monorepo_backend(&backend))?;
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

        Err(crate::error::scaffold_not_implemented(
            label,
            &layer.logical_rel(),
        ))
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
            return Err(crate::error::template_layer_missing_manifest(
                &layer.logical_rel(),
            ));
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
                return Err(crate::error::duplicate_template_target(
                    &file.target,
                    &layer.logical_rel(),
                ));
            }
        }

        for file in active_files {
            let source_rel = format!("sources/{}", file.source);
            if !layer.exists(&source_rel) {
                return Err(crate::error::template_source_missing(
                    &file.source,
                    &layer.logical_rel(),
                ));
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
                        return Err(crate::error::binary_source_with_context(
                            &layer.label(&source_rel),
                        ));
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
                return Err(crate::error::duplicate_template_target(
                    &file.target,
                    &layer.logical_rel(),
                ));
            }
        }

        for file in active_files {
            let source_rel = format!("sources/{}", file.source);
            if !layer.exists(&source_rel) {
                return Err(crate::error::template_source_missing(
                    &file.source,
                    &layer.logical_rel(),
                ));
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
                        return Err(crate::error::binary_source_with_context(
                            &layer.label(&source_rel),
                        ));
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
                return Err(crate::error::template_token_undeclared(token, source));
            }
            if self.value(token).is_none() {
                return Err(crate::error::template_token_unsupported(token, source));
            }
        }

        for key in required_context {
            if self.value(key).is_none() {
                return Err(crate::error::template_context_unsupported(key, source));
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
                return Err(crate::error::template_token_undeclared(token, source));
            }
            if self.value(token).is_none() {
                return Err(crate::error::template_token_unsupported(token, source));
            }
        }

        for key in required_context {
            if self.value(key).is_none() {
                return Err(crate::error::template_context_unsupported(key, source));
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
            return Err(crate::error::template_manifest_missing(
                &layer.logical_rel(),
            ));
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
        "# {}\n\nGenerated by MagiCore.\n\n- Core: `{}`\n- Framework: `{}`\n",
        name, core, framework
    );
    if !features.is_empty() {
        out.push_str(&format!("- Features: `{}`\n", features.join("`, `")));
    }
    let next_step = "Run `mgc install` in this directory; MagiCore detects the core from `mgc.toml` and `.mgc.core`.";
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
        ".magicore/cache",
        ".magicore/tmp",
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
        "magicore-project".to_string()
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
#[path = "../test/processor_test.rs"]
mod tests;
