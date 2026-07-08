use crate::factory;
use crate::scaffold::Scaffolder;
use crate::wizard::engine::{Answer, Question, QuestionKind, ScaffoldConfig, WizardEngine};
use crate::wizard::web::WebWizard;
use anyhow::Result;
use mg_config::project::ProjectConfig;
use mg_ui::{print_banner, print_next_steps, section};

/// mg init — interactive project wizard
pub async fn run(template: Option<String>) -> Result<()> {
    print_banner();

    if let Some(t) = template {
        let project_name = ask_project_name();
        let config = ScaffoldConfig {
            core: t.clone(),
            sub_type: String::new(),
            frameworks: vec![],
            project_name,
            features: vec![],
            template_dir: std::path::PathBuf::new(),
        };
        let project_dir = Scaffolder::scaffold(&config)?;
        let proj_config = ProjectConfig::new(Scaffolder::display_name(&project_dir), &config.core);
        proj_config.save(&project_dir)?;
        return Ok(());
    }

    // Step 1: Select core (auto-skip if single-core build)
    section("Choose your project type", 1, 4);
    let core = pick_core();

    // Step 2: Run core-specific wizard
    section("Configure your project", 2, 4);

    // Each core has its own decision tree
    let (mut config, features) = run_core_wizard(&core);

    // Step 3: Features (if the wizard returned any)
    section("Additional features", 3, 4);
    if !features.is_empty() {
        let feats = WizardEngine::run_question(&Question {
            prompt: "Select features:".to_string(),
            kind: QuestionKind::MultiSelect { options: features },
        });
        config.features = feats;
    }

    // Step 4: Scaffold + save .megagate config
    section("Creating project...", 4, 4);
    let project_dir = Scaffolder::scaffold(&config)?;

    let proj_config = ProjectConfig::new(Scaffolder::display_name(&project_dir), &config.core);
    proj_config.save(&project_dir)?;

    print_next_steps(&config.project_name);
    Ok(())
}

/// Pick a core from whatever is available in this build.
fn pick_core() -> String {
    let avail = factory::available_cores();
    if avail.is_empty() {
        eprintln!("error: no cores available in this build");
        std::process::exit(1);
    }
    // Single-core build: auto-select, skip menu
    if avail.len() == 1 {
        return avail[0].0.to_string();
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

/// Dispatch to the right core wizard.
/// Each core defines its own decision tree independently.
/// Non-web cores use a generic stub (just ask project name) until their wizard is built.
fn run_core_wizard(core: &str) -> (ScaffoldConfig, Vec<Answer>) {
    match core {
        "web" => {
            let mut cfg = WebWizard::run();
            cfg.project_name = ask_project_name();
            (cfg, ask_web_features())
        }
        // Future cores will have their own wizards here:
        // "game" => game::GameWizard::run() + game_features(),
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

fn ask_web_features() -> Vec<Answer> {
    let use_defaults =
        mg_ui::prompt::confirm("Use default settings for this framework?").unwrap_or(true);

    if use_defaults {
        vec![
            Answer::new("✔ TypeScript (recommended)", "ts"),
            Answer::new("✔ ESLint", "eslint"),
        ]
    } else {
        vec![
            Answer::new("☐ TypeScript", "ts"),
            Answer::new("☐ Tailwind CSS", "tailwind"),
            Answer::new("☐ ESLint", "eslint"),
            Answer::new("☐ Vitest", "vitest"),
            Answer::new("☐ Playwright", "playwright"),
            Answer::new("☐ Prisma (ORM)", "prisma"),
            Answer::new("☐ next-auth", "next-auth"),
        ]
    }
}
