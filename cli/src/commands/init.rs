use crate::factory;
use crate::scaffold::Scaffolder;
use crate::wizard::engine::{Answer, Question, QuestionKind, ScaffoldConfig, WizardEngine};
use crate::wizard::web::WebWizard;
use anyhow::Result;
use mg_config::project::ProjectConfig;
use mg_ui::{print_banner, print_next_steps, section};
use std::path::Path;

/// mg init — create a new project with mg.toml
pub async fn run(template: Option<String>) -> Result<()> {
    print_banner();

    if let Some(t) = template {
        let mut config = if t == "web" {
            WebWizard::run()
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
            let features = ask_web_features(&config);
            if !features.is_empty() {
                let feats = WizardEngine::run_question(&Question {
                    prompt: "Select features:".to_string(),
                    kind: QuestionKind::MultiSelect { options: features },
                });
                config.features = feats;
            }
        }
        let project_dir = Scaffolder::scaffold(&config)?;
        write_mg_toml(&project_dir, &config)?;
        return Ok(());
    }

    section("Choose your project type", 1, 4);
    let core = pick_core();

    section("Configure your project", 2, 4);

    let (mut config, features) = run_core_wizard(&core);

    section("Additional features", 3, 4);
    if !features.is_empty() {
        let feats = WizardEngine::run_question(&Question {
            prompt: "Select features:".to_string(),
            kind: QuestionKind::MultiSelect { options: features },
        });
        config.features = feats;
    }

    section("Creating project...", 4, 4);
    let project_dir = Scaffolder::scaffold(&config)?;

    write_mg_toml(&project_dir, &config)?;

    print_next_steps(&config.project_name);
    Ok(())
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

fn run_core_wizard(core: &str) -> (ScaffoldConfig, Vec<Answer>) {
    match core {
        "web" => {
            let mut cfg = WebWizard::run();
            cfg.project_name = ask_project_name();
            let features = ask_web_features(&cfg);
            (cfg, features)
        }
        _ => {
            let name = ask_project_name();
            (
                ScaffoldConfig {
                    core: core.to_string(),
                    project_name: name,
                    ..Default::default()
                },
                vec![],
            )
        }
    }
}

fn ask_project_name() -> String {
    mg_ui::prompt::input("Project name:").unwrap_or_else(|_| "my-project".to_string())
}

fn ask_web_features(config: &ScaffoldConfig) -> Vec<Answer> {
    let use_defaults =
        mg_ui::prompt::confirm("Use default settings for this framework?").unwrap_or(true);

    let options = match config.sub_type.as_str() {
        "frontend" => vec![
            Answer::new("TypeScript", "ts"),
            Answer::new("Tailwind CSS", "tailwind"),
            Answer::new("ESLint", "eslint"),
            Answer::new("Vitest", "vitest"),
            Answer::new("Playwright", "playwright"),
        ],
        "backend" => vec![
            Answer::new("TypeScript", "ts"),
            Answer::new("OpenAPI contract", "openapi"),
            Answer::new("ESLint", "eslint"),
            Answer::new("Database layer", "db"),
            Answer::new("Container baseline", "docker"),
        ],
        "fullstack" => vec![
            Answer::new("TypeScript", "ts"),
            Answer::new("Tailwind CSS", "tailwind"),
            Answer::new("Shared schema/contracts", "schema"),
            Answer::new("API client generation", "api-client"),
            Answer::new("Playwright", "playwright"),
            Answer::new("Database layer", "db"),
        ],
        "monorepo" => vec![
            Answer::new("TypeScript", "ts"),
            Answer::new("Shared schema/contracts", "schema"),
            Answer::new("Shared config layer", "shared-config"),
            Answer::new("API client generation", "api-client"),
            Answer::new("Workspace lint baseline", "eslint"),
            Answer::new("Playwright", "playwright"),
        ],
        _ => vec![],
    };

    if use_defaults {
        options.into_iter().take(2).collect()
    } else {
        options
    }
}
