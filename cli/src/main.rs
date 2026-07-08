/// MegaGate CLI - Universal Package Manager
use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod context;
mod factory;
mod wizard;
mod scaffold;

#[derive(Parser)]
#[command(name = "mg")]
#[command(about = "MegaGate - Universal Package Manager", long_about = None)]
#[command(version)]

struct Cli {
    /// Target core (web, game, ai, clo, cicd, iot, app, lib)
    #[arg(global = true, long)]
    core: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

// ─── Per-core commands (7 cores × 5 commands = 35 variants) ───────
// Global mode:   mg add-web react, mg remove-web lodash, ...
// Single mode:   mg add react, mg remove lodash, ...

#[derive(Subcommand)]
enum Commands {
    // ── Common commands ────────────────────────────────────────
    #[command(about = "Interactive project wizard")]
    Init {
        #[arg(short, long)]
        template: Option<String>,
    },
    #[command(about = "Install all dependencies")]
    Install {
        packages: Vec<String>,
    },
    #[command(about = "Show package information")]
    Info {
        package: String,
    },
    #[command(about = "Search for packages")]
    Search {
        query: String,
    },
    #[command(about = "Check for outdated packages")]
    Outdated,
    #[command(about = "Audit packages for vulnerabilities")]
    Audit,

    // ── Single-core (bare) — reads .megagate/ ──────────────────
    #[command(about = "Add a dependency")]
    Add {
        package: String,
        #[arg(short, long)]
        version: Option<String>,
        #[arg(short = 'D', long)]
        dev: bool,
        #[arg(short = 'g', long)]
        global: bool,
        #[arg(short = 'E', long)]
        exact: bool,
        #[arg(short = 'O', long)]
        optional: bool,
        #[arg(short = 'P', long)]
        peer: bool,
        #[arg(long)]
        no_save: bool,
    },
    #[command(about = "Remove a dependency")]
    Remove {
        package: String,
    },
    #[command(about = "Update packages")]
    Update {
        packages: Vec<String>,
    },
    #[command(about = "List installed packages")]
    List,

    // ── Per-core: create-<core> ────────────────────────────────
    #[command(name = "create-web", about = "Scaffold a new web project")]
    CreateWeb { framework: String, project_name: String },
    #[command(name = "create-game", about = "Scaffold a new game project")]
    CreateGame { framework: String, project_name: String },
    #[command(name = "create-ai", about = "Scaffold a new AI project")]
    CreateAi { framework: String, project_name: String },
    #[command(name = "create-clo", about = "Scaffold a new cloud project")]
    CreateClo { framework: String, project_name: String },
    #[command(name = "create-cicd", about = "Scaffold a new CI/CD project")]
    CreateCicd { framework: String, project_name: String },
    #[command(name = "create-iot", about = "Scaffold a new IoT project")]
    CreateIot { framework: String, project_name: String },
    #[command(name = "create-app", about = "Scaffold a new app project")]
    CreateApp { framework: String, project_name: String },
    #[command(name = "create-lib", about = "Scaffold a new library project")]
    CreateLib { framework: String, project_name: String },

    // ── Per-core: add-<core> ───────────────────────────────────
    #[command(name = "add-web", about = "Add web dependency")]
    AddWeb { package: String, #[arg(short = 'D', long)] dev: bool, #[arg(short = 'E', long)] exact: bool, #[arg(short = 'O', long)] optional: bool, #[arg(short = 'P', long)] peer: bool, #[arg(long)] no_save: bool, #[arg(short = 'g', long)] global: bool },
    #[command(name = "add-game", about = "Add game dependency")]
    AddGame { package: String, #[arg(short = 'D', long)] dev: bool, #[arg(short = 'E', long)] exact: bool, #[arg(short = 'O', long)] optional: bool, #[arg(short = 'P', long)] peer: bool, #[arg(long)] no_save: bool, #[arg(short = 'g', long)] global: bool },
    #[command(name = "add-ai", about = "Add AI dependency")]
    AddAi { package: String, #[arg(short = 'D', long)] dev: bool, #[arg(short = 'E', long)] exact: bool, #[arg(short = 'O', long)] optional: bool, #[arg(short = 'P', long)] peer: bool, #[arg(long)] no_save: bool, #[arg(short = 'g', long)] global: bool },
    #[command(name = "add-clo", about = "Add cloud dependency")]
    AddClo { package: String, #[arg(short = 'D', long)] dev: bool, #[arg(short = 'E', long)] exact: bool, #[arg(short = 'O', long)] optional: bool, #[arg(short = 'P', long)] peer: bool, #[arg(long)] no_save: bool, #[arg(short = 'g', long)] global: bool },
    #[command(name = "add-cicd", about = "Add CI/CD dependency")]
    AddCicd { package: String, #[arg(short = 'D', long)] dev: bool, #[arg(short = 'E', long)] exact: bool, #[arg(short = 'O', long)] optional: bool, #[arg(short = 'P', long)] peer: bool, #[arg(long)] no_save: bool, #[arg(short = 'g', long)] global: bool },
    #[command(name = "add-iot", about = "Add IoT dependency")]
    AddIot { package: String, #[arg(short = 'D', long)] dev: bool, #[arg(short = 'E', long)] exact: bool, #[arg(short = 'O', long)] optional: bool, #[arg(short = 'P', long)] peer: bool, #[arg(long)] no_save: bool, #[arg(short = 'g', long)] global: bool },
    #[command(name = "add-app", about = "Add app dependency")]
    AddApp { package: String, #[arg(short = 'D', long)] dev: bool, #[arg(short = 'E', long)] exact: bool, #[arg(short = 'O', long)] optional: bool, #[arg(short = 'P', long)] peer: bool, #[arg(long)] no_save: bool, #[arg(short = 'g', long)] global: bool },
    #[command(name = "add-lib", about = "Add library dependency")]
    AddLib { package: String, #[arg(short = 'D', long)] dev: bool, #[arg(short = 'E', long)] exact: bool, #[arg(short = 'O', long)] optional: bool, #[arg(short = 'P', long)] peer: bool, #[arg(long)] no_save: bool, #[arg(short = 'g', long)] global: bool },

    // ── Per-core: remove-<core> ────────────────────────────────
    #[command(name = "remove-web", about = "Remove web dependency")]
    RemoveWeb { package: String },
    #[command(name = "remove-game", about = "Remove game dependency")]
    RemoveGame { package: String },
    #[command(name = "remove-ai", about = "Remove AI dependency")]
    RemoveAi { package: String },
    #[command(name = "remove-clo", about = "Remove cloud dependency")]
    RemoveClo { package: String },
    #[command(name = "remove-cicd", about = "Remove CI/CD dependency")]
    RemoveCicd { package: String },
    #[command(name = "remove-iot", about = "Remove IoT dependency")]
    RemoveIot { package: String },
    #[command(name = "remove-app", about = "Remove app dependency")]
    RemoveApp { package: String },
    #[command(name = "remove-lib", about = "Remove library dependency")]
    RemoveLib { package: String },

    // ── Per-core: list-<core> ──────────────────────────────────
    #[command(name = "list-web", about = "List web packages")]
    ListWeb,
    #[command(name = "list-game", about = "List game packages")]
    ListGame,
    #[command(name = "list-ai", about = "List AI packages")]
    ListAi,
    #[command(name = "list-clo", about = "List cloud packages")]
    ListClo,
    #[command(name = "list-cicd", about = "List CI/CD packages")]
    ListCicd,
    #[command(name = "list-iot", about = "List IoT packages")]
    ListIot,
    #[command(name = "list-app", about = "List app packages")]
    ListApp,
    #[command(name = "list-lib", about = "List library packages")]
    ListLib,

    // ── Per-core: update-<core> ────────────────────────────────
    #[command(name = "update-web", about = "Update web packages")]
    UpdateWeb { packages: Vec<String> },
    #[command(name = "update-game", about = "Update game packages")]
    UpdateGame { packages: Vec<String> },
    #[command(name = "update-ai", about = "Update AI packages")]
    UpdateAi { packages: Vec<String> },
    #[command(name = "update-clo", about = "Update cloud packages")]
    UpdateClo { packages: Vec<String> },
    #[command(name = "update-cicd", about = "Update CI/CD packages")]
    UpdateCicd { packages: Vec<String> },
    #[command(name = "update-iot", about = "Update IoT packages")]
    UpdateIot { packages: Vec<String> },
    #[command(name = "update-app", about = "Update app packages")]
    UpdateApp { packages: Vec<String> },
    #[command(name = "update-lib", about = "Update library packages")]
    UpdateLib { packages: Vec<String> },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let core = cli.core.as_deref();

    match cli.command {
        // ── Common ─────────────────────────────────────────────
        Some(Commands::Init { template }) => {
            commands::init::run(template).await?;
        }
        Some(Commands::Install { packages }) => {
            commands::install::run(packages, core).await?;
        }
        Some(Commands::Info { package }) => {
            commands::info::run(package).await?;
        }
        Some(Commands::Search { query }) => {
            commands::search::run(query).await?;
        }
        Some(Commands::Outdated) => {
            commands::outdated::run(core).await?;
        }
        Some(Commands::Audit) => {
            commands::audit::run(core).await?;
        }

        // ── Single-core (bare) ────────────────────────────────
        Some(Commands::Add { package, version, dev, global, exact, optional, peer, no_save }) => {
            commands::add::run(package, version, dev, exact, optional, peer, no_save, global, core).await?;
        }
        Some(Commands::Remove { package }) => {
            commands::remove::run(package, core).await?;
        }
        Some(Commands::Update { packages }) => {
            commands::update::run(packages, core).await?;
        }
        Some(Commands::List) => {
            commands::list::run(core).await?;
        }

        // ── create-<core> ──────────────────────────────────────
        Some(Commands::CreateWeb { framework, project_name }) => {
            commands::create::run("web", &framework, &project_name).await?;
        }
        Some(Commands::CreateGame { framework, project_name }) => {
            commands::create::run("game", &framework, &project_name).await?;
        }
        Some(Commands::CreateAi { framework, project_name }) => {
            commands::create::run("ai", &framework, &project_name).await?;
        }
        Some(Commands::CreateClo { framework, project_name }) => {
            commands::create::run("clo", &framework, &project_name).await?;
        }
        Some(Commands::CreateCicd { framework, project_name }) => {
            commands::create::run("cicd", &framework, &project_name).await?;
        }
        Some(Commands::CreateIot { framework, project_name }) => {
            commands::create::run("iot", &framework, &project_name).await?;
        }
        Some(Commands::CreateApp { framework, project_name }) => {
            commands::create::run("app", &framework, &project_name).await?;
        }
        Some(Commands::CreateLib { framework, project_name }) => {
            commands::create::run("lib", &framework, &project_name).await?;
        }

        // ── add-<core> ─────────────────────────────────────────
        Some(Commands::AddWeb { package, dev, exact, optional, peer, no_save, global }) => {
            commands::add::run(package, None, dev, exact, optional, peer, no_save, global, Some("web")).await?;
        }
        Some(Commands::AddGame { package, dev, exact, optional, peer, no_save, global }) => {
            commands::add::run(package, None, dev, exact, optional, peer, no_save, global, Some("game")).await?;
        }
        Some(Commands::AddAi { package, dev, exact, optional, peer, no_save, global }) => {
            commands::add::run(package, None, dev, exact, optional, peer, no_save, global, Some("ai")).await?;
        }
        Some(Commands::AddClo { package, dev, exact, optional, peer, no_save, global }) => {
            commands::add::run(package, None, dev, exact, optional, peer, no_save, global, Some("clo")).await?;
        }
        Some(Commands::AddCicd { package, dev, exact, optional, peer, no_save, global }) => {
            commands::add::run(package, None, dev, exact, optional, peer, no_save, global, Some("cicd")).await?;
        }
        Some(Commands::AddIot { package, dev, exact, optional, peer, no_save, global }) => {
            commands::add::run(package, None, dev, exact, optional, peer, no_save, global, Some("iot")).await?;
        }
        Some(Commands::AddApp { package, dev, exact, optional, peer, no_save, global }) => {
            commands::add::run(package, None, dev, exact, optional, peer, no_save, global, Some("app")).await?;
        }
        Some(Commands::AddLib { package, dev, exact, optional, peer, no_save, global }) => {
            commands::add::run(package, None, dev, exact, optional, peer, no_save, global, Some("lib")).await?;
        }

        // ── remove-<core> ──────────────────────────────────────
        Some(Commands::RemoveWeb { package }) => {
            commands::remove::run(package, Some("web")).await?;
        }
        Some(Commands::RemoveGame { package }) => {
            commands::remove::run(package, Some("game")).await?;
        }
        Some(Commands::RemoveAi { package }) => {
            commands::remove::run(package, Some("ai")).await?;
        }
        Some(Commands::RemoveClo { package }) => {
            commands::remove::run(package, Some("clo")).await?;
        }
        Some(Commands::RemoveCicd { package }) => {
            commands::remove::run(package, Some("cicd")).await?;
        }
        Some(Commands::RemoveIot { package }) => {
            commands::remove::run(package, Some("iot")).await?;
        }
        Some(Commands::RemoveApp { package }) => {
            commands::remove::run(package, Some("app")).await?;
        }
        Some(Commands::RemoveLib { package }) => {
            commands::remove::run(package, Some("lib")).await?;
        }

        // ── list-<core> ────────────────────────────────────────
        Some(Commands::ListWeb) => commands::list::run(Some("web")).await?,
        Some(Commands::ListGame) => commands::list::run(Some("game")).await?,
        Some(Commands::ListAi) => commands::list::run(Some("ai")).await?,
        Some(Commands::ListClo) => commands::list::run(Some("clo")).await?,
        Some(Commands::ListCicd) => commands::list::run(Some("cicd")).await?,
        Some(Commands::ListIot) => commands::list::run(Some("iot")).await?,
        Some(Commands::ListApp) => commands::list::run(Some("app")).await?,
        Some(Commands::ListLib) => commands::list::run(Some("lib")).await?,

        // ── update-<core> ──────────────────────────────────────
        Some(Commands::UpdateWeb { packages }) => {
            commands::update::run(packages, Some("web")).await?;
        }
        Some(Commands::UpdateGame { packages }) => {
            commands::update::run(packages, Some("game")).await?;
        }
        Some(Commands::UpdateAi { packages }) => {
            commands::update::run(packages, Some("ai")).await?;
        }
        Some(Commands::UpdateClo { packages }) => {
            commands::update::run(packages, Some("clo")).await?;
        }
        Some(Commands::UpdateCicd { packages }) => {
            commands::update::run(packages, Some("cicd")).await?;
        }
        Some(Commands::UpdateIot { packages }) => {
            commands::update::run(packages, Some("iot")).await?;
        }
        Some(Commands::UpdateApp { packages }) => {
            commands::update::run(packages, Some("app")).await?;
        }
        Some(Commands::UpdateLib { packages }) => {
            commands::update::run(packages, Some("lib")).await?;
        }

        None => {
            // Custom colored help
            mg_ui::help::print_custom_help();
        }
    }

    Ok(())
}
