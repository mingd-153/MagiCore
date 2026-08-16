use crate::commands::core::scaffold_flags::ScaffoldFlags;
use crate::commands::core::web::{enrich_web_project_manifest, parse_framework_request};
use crate::factory;
use crate::scaffold::Scaffolder;
use crate::wizard::engine::{Answer, Question, QuestionKind, ScaffoldConfig, WizardEngine};
use crate::wizard::web::WebWizard;
use anyhow::Result;
use mg_config::project::ProjectConfig;
use mg_ui::{print_banner, print_next_steps, section};
use std::path::{Path, PathBuf};

/// mg init — create a new project with mg.toml
pub async fn run(template: Option<String>) -> Result<()> {
    print_banner();

    if let Some(t) = template {
        let mut config = if t == "web" {
            WebWizard::run()
        } else if t == "hardware" {
            crate::wizard::hardware::HardwareWizard::run()
        } else if t == "lib" {
            crate::wizard::lib::LibWizard::run()
        } else if t == "game" {
            crate::wizard::game::GameWizard::run()
        } else if t == "iot" {
            crate::wizard::iot::IotWizard::run()
        } else if t == "clo" || t == "cloud" {
            crate::wizard::cloud::CloudWizard::run()
        } else if t == "cicd" {
            crate::wizard::cicd::CicdWizard::run()
        } else {
            ScaffoldConfig {
                core: t.clone(),
                sub_type: String::new(),
                frameworks: vec![],
                project_name: String::new(),
                features: vec![],
                template_dir: std::path::PathBuf::new(),
            }
        };
        config.project_name = ask_project_name();
        if t == "web" {
            let (features, show_multi) = ask_web_features(&config);
            if !features.is_empty() {
                if show_multi {
                    let feats = WizardEngine::run_question(&Question {
                        prompt: "Select features:".to_string(),
                        kind: QuestionKind::MultiSelect { options: features },
                    });
                    config.features = feats;
                } else {
                    config.features = features.into_iter().map(|a| a.value).collect();
                }
            }
        }
        if t == "hardware" {
            let project_dir = init_hardware(&config).await?;
            write_mg_toml(&project_dir, &config)?;
            return Ok(());
        }
        let project_dir = Scaffolder::scaffold(&config)?;
        write_mg_toml(&project_dir, &config)?;
        if t == "web" && !config.frameworks.is_empty() {
            seed_web_deps(&project_dir, &config).await?;
        }
        return Ok(());
    }

    section("Choose your project type", 1, 4);
    let core = pick_core();

    section("Configure your project", 2, 4);

    let (mut config, features, show_multi) = run_core_wizard(&core);

    section("Additional features", 3, 4);
    if !features.is_empty() {
        if show_multi {
            let feats = WizardEngine::run_question(&Question {
                prompt: "Select features:".to_string(),
                kind: QuestionKind::MultiSelect { options: features },
            });
            config.features = feats;
        } else {
            config.features = features.into_iter().map(|a| a.value).collect();
        }
    }

    section("Creating project...", 4, 4);
    let project_dir = if config.core == "hardware" {
        init_hardware(&config).await?
    } else {
        Scaffolder::scaffold(&config)?
    };

    write_mg_toml(&project_dir, &config)?;

    if config.core == "web" && !config.frameworks.is_empty() {
        seed_web_deps(&project_dir, &config).await?;
    }

    print_next_steps(&config.project_name);
    Ok(())
}

/// hardware core: tạo root project + materialize package vào subfolder (giống add-hardware),
/// để mg.toml nằm ở root và list-hardware detect được package.
async fn init_hardware(config: &ScaffoldConfig) -> Result<PathBuf> {
    let root = std::env::current_dir()
        .map_err(|e| anyhow::anyhow!("failed to resolve current working directory: {e}"))?
        .join(&config.project_name);
    std::fs::create_dir_all(&root)?;
    let framework = config
        .frameworks
        .first()
        .map(|s| s.as_str())
        .unwrap_or("optimizer");
    crate::commands::core::hardware::materialize_template(&root, framework).await?;
    Ok(root)
}

fn write_mg_toml(project_dir: &Path, config: &ScaffoldConfig) -> Result<()> {
    let name = Scaffolder::display_name(project_dir);
    let template = config.template_dir.to_str().unwrap_or("").to_string();
    let proj_config = ProjectConfig::from_scaffold(
        name,
        &config.core,
        &config.sub_type,
        config.frameworks.clone(),
        template,
        config.features.clone(),
    );
    proj_config.save(project_dir)?;
    Ok(())
}

fn pick_core() -> String {
    let avail = factory::available_cores();
    if avail.is_empty() {
        eprintln!("error: no cores available in this build");
        std::process::exit(1);
    }
    let options: Vec<Answer> = avail
        .iter()
        .map(|(short, label)| Answer::new(label, short))
        .collect();

    let question = Question {
        prompt: "What do you want to build?".to_string(),
        kind: QuestionKind::Select { options },
    };
    WizardEngine::run_question(&question)
        .first()
        .cloned()
        .unwrap_or_else(|| avail[0].0.to_string())
}

fn run_core_wizard(core: &str) -> (ScaffoldConfig, Vec<Answer>, bool) {
    match core {
        "web" => {
            let mut cfg = WebWizard::run();
            cfg.project_name = ask_project_name();
            let (features, show_multi) = ask_web_features(&cfg);
            (cfg, features, show_multi)
        }
        "hardware" => {
            let mut cfg = crate::wizard::hardware::HardwareWizard::run();
            cfg.project_name = ask_project_name();
            (cfg, Vec::new(), false)
        }
        "lib" => {
            let mut cfg = crate::wizard::lib::LibWizard::run();
            cfg.project_name = ask_project_name();
            (cfg, Vec::new(), false)
        }
        "game" => {
            let mut cfg = crate::wizard::game::GameWizard::run();
            cfg.project_name = ask_project_name();
            (cfg, Vec::new(), false)
        }
        "iot" => {
            let mut cfg = crate::wizard::iot::IotWizard::run();
            cfg.project_name = ask_project_name();
            (cfg, Vec::new(), false)
        }
        "clo" | "cloud" => {
            let mut cfg = crate::wizard::cloud::CloudWizard::run();
            cfg.project_name = ask_project_name();
            (cfg, Vec::new(), false)
        }
        "cicd" => {
            let mut cfg = crate::wizard::cicd::CicdWizard::run();
            cfg.project_name = ask_project_name();
            (cfg, Vec::new(), false)
        }
        _ => {
            let name = ask_project_name();
            (
                ScaffoldConfig {
                    core: core.to_string(),
                    project_name: name,
                    ..Default::default()
                },
                Vec::new(),
                false,
            )
        }
    }
}

fn ask_project_name() -> String {
    mg_ui::prompt::input("Project name:").unwrap_or_else(|_| "my-project".to_string())
}

async fn seed_web_deps(project_dir: &Path, config: &ScaffoldConfig) -> Result<()> {
    let mut flags = ScaffoldFlags::default();
    for feat in &config.features {
        match feat.as_str() {
            "typescript" | "ts" => flags.ts = true,
            "tailwindcss" | "tailwind" => flags.tailwindcss = true,
            "eslint" => flags.eslint = true,
            "vitest" => flags.vitest = true,
            "playwright" => flags.playwright = true,
            _ => {}
        }
    }
    if config.sub_type == "monorepo" {
        flags.monorepo = true;
    }
    let fe = &config.frameworks[0];
    let frontend = parse_framework_request(fe);
    match config.sub_type.as_str() {
        "fullstack" => {
            let backend = if config.frameworks.len() > 1 {
                Some(parse_framework_request(&config.frameworks[1]))
            } else {
                None
            };
            enrich_web_project_manifest(project_dir, &frontend, backend.as_ref(), &flags).await
        }
        "monorepo" => {
            let backend = config.frameworks.get(1).map(|b| parse_framework_request(b));
            enrich_web_project_manifest(project_dir, &frontend, backend.as_ref(), &flags).await
        }
        _ => enrich_web_project_manifest(project_dir, &frontend, None, &flags).await,
    }
}

fn ask_web_features(config: &ScaffoldConfig) -> (Vec<Answer>, bool) {
    let use_defaults =
        mg_ui::prompt::confirm("Use default settings for this framework?").unwrap_or(true);

    let options = match config.sub_type.as_str() {
        "frontend" => vec![
            Answer::new("TypeScript", "typescript"),
            Answer::new("Tailwind CSS", "tailwindcss"),
            Answer::new("ESLint", "eslint"),
            Answer::new("Vitest", "vitest"),
            Answer::new("Playwright", "playwright"),
        ],
        "backend" => vec![
            Answer::new("TypeScript", "typescript"),
            Answer::new("OpenAPI contract", "openapi"),
            Answer::new("ESLint", "eslint"),
            Answer::new("Database layer", "db"),
            Answer::new("Container baseline", "docker"),
        ],
        "fullstack" => vec![
            Answer::new("TypeScript", "typescript"),
            Answer::new("Tailwind CSS", "tailwindcss"),
            Answer::new("Shared schema/contracts", "schema"),
            Answer::new("API client generation", "api-client"),
            Answer::new("Playwright", "playwright"),
            Answer::new("Database layer", "db"),
        ],
        "monorepo" => vec![
            Answer::new("TypeScript", "typescript"),
            Answer::new("Shared schema/contracts", "schema"),
            Answer::new("Shared config layer", "shared-config"),
            Answer::new("API client generation", "api-client"),
            Answer::new("Workspace lint baseline", "eslint"),
            Answer::new("Playwright", "playwright"),
        ],
        _ => vec![],
    };

    if use_defaults {
        (options.into_iter().take(2).collect(), false)
    } else {
        (options, true)
    }
}
