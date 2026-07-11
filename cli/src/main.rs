/// MegaGate CLI - Universal Package Manager
use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod context;
mod factory;
mod scaffold;
mod wizard;

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
    Install { packages: Vec<String> },
    #[cfg(all(
        feature = "web",
        any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        )
    ))]
    #[command(name = "install-web", about = "Install web dependencies")]
    InstallWeb { packages: Vec<String> },
    #[cfg(feature = "game")]
    #[command(name = "install-game", about = "Install game dependencies")]
    InstallGame { packages: Vec<String> },
    #[cfg(feature = "ai")]
    #[command(name = "install-ai", about = "Install AI dependencies")]
    InstallAi { packages: Vec<String> },
    #[cfg(feature = "clo")]
    #[command(name = "install-clo", about = "Install cloud dependencies")]
    InstallClo { packages: Vec<String> },
    #[cfg(feature = "cicd")]
    #[command(name = "install-cicd", about = "Install CI/CD dependencies")]
    InstallCicd { packages: Vec<String> },
    #[cfg(feature = "iot")]
    #[command(name = "install-iot", about = "Install IoT dependencies")]
    InstallIot { packages: Vec<String> },
    #[cfg(feature = "app")]
    #[command(name = "install-app", about = "Install app dependencies")]
    InstallApp { packages: Vec<String> },
    #[cfg(feature = "lib")]
    #[command(name = "install-lib", about = "Install library dependencies")]
    InstallLib { packages: Vec<String> },
    #[command(about = "Show package information")]
    Info { package: String },
    #[command(about = "Search for packages")]
    Search { query: String },
    #[command(about = "Check for outdated packages")]
    Outdated,
    #[command(about = "Audit packages for vulnerabilities")]
    Audit,

    // ── Single-core (bare) — reads .megagate/ ──────────────────
    #[command(about = "Add a dependency")]
    Add {
        #[arg(required = true)]
        packages: Vec<String>,
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
    Remove { package: String },
    #[command(about = "Update packages")]
    Update { packages: Vec<String> },
    #[command(about = "List installed packages")]
    List,

    // ── Per-core: create-<core> ────────────────────────────────
    #[cfg(all(
        feature = "web",
        not(feature = "game"),
        not(feature = "ai"),
        not(feature = "clo"),
        not(feature = "cicd"),
        not(feature = "iot"),
        not(feature = "app"),
        not(feature = "lib")
    ))]
    #[command(
        name = "create",
        visible_alias = "create-web",
        about = "Scaffold a new web project"
    )]
    CreateWeb {
        framework: String,
        project_name: String,
        #[arg(long)]
        ts: bool,
        #[arg(long, visible_alias = "tailwind")]
        tailwindcss: bool,
        #[arg(long)]
        monorepo: bool,
        #[arg(long)]
        backend: Option<String>,
        #[arg(long = "feature")]
        features: Vec<String>,
    },
    #[cfg(all(
        feature = "web",
        any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        )
    ))]
    #[command(name = "create-web", about = "Scaffold a new web project")]
    CreateWeb {
        framework: String,
        project_name: String,
        #[arg(long)]
        ts: bool,
        #[arg(long, visible_alias = "tailwind")]
        tailwindcss: bool,
        #[arg(long)]
        monorepo: bool,
        #[arg(long)]
        backend: Option<String>,
        #[arg(long = "feature")]
        features: Vec<String>,
    },
    #[cfg(feature = "game")]
    #[command(name = "create-game", about = "Scaffold a new game project")]
    CreateGame {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "ai")]
    #[command(name = "create-ai", about = "Scaffold a new AI project")]
    CreateAi {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "clo")]
    #[command(name = "create-clo", about = "Scaffold a new cloud project")]
    CreateClo {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "cicd")]
    #[command(name = "create-cicd", about = "Scaffold a new CI/CD project")]
    CreateCicd {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "iot")]
    #[command(name = "create-iot", about = "Scaffold a new IoT project")]
    CreateIot {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "app")]
    #[command(name = "create-app", about = "Scaffold a new app project")]
    CreateApp {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "lib")]
    #[command(name = "create-lib", about = "Scaffold a new library project")]
    CreateLib { project_name: String },

    // ── Per-core: add-<core> ───────────────────────────────────
    #[cfg(all(
        feature = "web",
        any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        )
    ))]
    #[command(name = "add-web", about = "Add web dependency")]
    AddWeb {
        #[arg(required = true)]
        packages: Vec<String>,
        #[arg(short = 'D', long)]
        dev: bool,
        #[arg(short = 'E', long)]
        exact: bool,
        #[arg(short = 'O', long)]
        optional: bool,
        #[arg(short = 'P', long)]
        peer: bool,
        #[arg(long)]
        no_save: bool,
        #[arg(short = 'g', long)]
        global: bool,
    },
    #[cfg(feature = "game")]
    #[command(name = "add-game", about = "Add game dependency")]
    AddGame {
        #[arg(required = true)]
        packages: Vec<String>,
        #[arg(short = 'D', long)]
        dev: bool,
        #[arg(short = 'E', long)]
        exact: bool,
        #[arg(short = 'O', long)]
        optional: bool,
        #[arg(short = 'P', long)]
        peer: bool,
        #[arg(long)]
        no_save: bool,
        #[arg(short = 'g', long)]
        global: bool,
    },
    #[cfg(feature = "ai")]
    #[command(name = "add-ai", about = "Add AI dependency")]
    AddAi {
        #[arg(required = true)]
        packages: Vec<String>,
        #[arg(short = 'D', long)]
        dev: bool,
        #[arg(short = 'E', long)]
        exact: bool,
        #[arg(short = 'O', long)]
        optional: bool,
        #[arg(short = 'P', long)]
        peer: bool,
        #[arg(long)]
        no_save: bool,
        #[arg(short = 'g', long)]
        global: bool,
    },
    #[cfg(feature = "clo")]
    #[command(name = "add-clo", about = "Add cloud dependency")]
    AddClo {
        #[arg(required = true)]
        packages: Vec<String>,
        #[arg(short = 'D', long)]
        dev: bool,
        #[arg(short = 'E', long)]
        exact: bool,
        #[arg(short = 'O', long)]
        optional: bool,
        #[arg(short = 'P', long)]
        peer: bool,
        #[arg(long)]
        no_save: bool,
        #[arg(short = 'g', long)]
        global: bool,
    },
    #[cfg(feature = "cicd")]
    #[command(name = "add-cicd", about = "Add CI/CD dependency")]
    AddCicd {
        #[arg(required = true)]
        packages: Vec<String>,
        #[arg(short = 'D', long)]
        dev: bool,
        #[arg(short = 'E', long)]
        exact: bool,
        #[arg(short = 'O', long)]
        optional: bool,
        #[arg(short = 'P', long)]
        peer: bool,
        #[arg(long)]
        no_save: bool,
        #[arg(short = 'g', long)]
        global: bool,
    },
    #[cfg(feature = "iot")]
    #[command(name = "add-iot", about = "Add IoT dependency")]
    AddIot {
        #[arg(required = true)]
        packages: Vec<String>,
        #[arg(short = 'D', long)]
        dev: bool,
        #[arg(short = 'E', long)]
        exact: bool,
        #[arg(short = 'O', long)]
        optional: bool,
        #[arg(short = 'P', long)]
        peer: bool,
        #[arg(long)]
        no_save: bool,
        #[arg(short = 'g', long)]
        global: bool,
    },
    #[cfg(feature = "app")]
    #[command(name = "add-app", about = "Add app dependency")]
    AddApp {
        #[arg(required = true)]
        packages: Vec<String>,
        #[arg(short = 'D', long)]
        dev: bool,
        #[arg(short = 'E', long)]
        exact: bool,
        #[arg(short = 'O', long)]
        optional: bool,
        #[arg(short = 'P', long)]
        peer: bool,
        #[arg(long)]
        no_save: bool,
        #[arg(short = 'g', long)]
        global: bool,
    },
    #[cfg(feature = "lib")]
    #[command(name = "add-lib", about = "Add library dependency")]
    AddLib {
        #[arg(required = true)]
        packages: Vec<String>,
        #[arg(short = 'D', long)]
        dev: bool,
        #[arg(short = 'E', long)]
        exact: bool,
        #[arg(short = 'O', long)]
        optional: bool,
        #[arg(short = 'P', long)]
        peer: bool,
        #[arg(long)]
        no_save: bool,
        #[arg(short = 'g', long)]
        global: bool,
    },

    // ── Per-core: remove-<core> ────────────────────────────────
    #[cfg(all(
        feature = "web",
        any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        )
    ))]
    #[command(name = "remove-web", about = "Remove web dependency")]
    RemoveWeb { package: String },
    #[cfg(feature = "game")]
    #[command(name = "remove-game", about = "Remove game dependency")]
    RemoveGame { package: String },
    #[cfg(feature = "ai")]
    #[command(name = "remove-ai", about = "Remove AI dependency")]
    RemoveAi { package: String },
    #[cfg(feature = "clo")]
    #[command(name = "remove-clo", about = "Remove cloud dependency")]
    RemoveClo { package: String },
    #[cfg(feature = "cicd")]
    #[command(name = "remove-cicd", about = "Remove CI/CD dependency")]
    RemoveCicd { package: String },
    #[cfg(feature = "iot")]
    #[command(name = "remove-iot", about = "Remove IoT dependency")]
    RemoveIot { package: String },
    #[cfg(feature = "app")]
    #[command(name = "remove-app", about = "Remove app dependency")]
    RemoveApp { package: String },
    #[cfg(feature = "lib")]
    #[command(name = "remove-lib", about = "Remove library dependency")]
    RemoveLib { package: String },

    // ── Per-core: list-<core> ──────────────────────────────────
    #[cfg(all(
        feature = "web",
        any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        )
    ))]
    #[command(name = "list-web", about = "List web packages")]
    ListWeb,
    #[cfg(feature = "game")]
    #[command(name = "list-game", about = "List game packages")]
    ListGame,
    #[cfg(feature = "ai")]
    #[command(name = "list-ai", about = "List AI packages")]
    ListAi,
    #[cfg(feature = "clo")]
    #[command(name = "list-clo", about = "List cloud packages")]
    ListClo,
    #[cfg(feature = "cicd")]
    #[command(name = "list-cicd", about = "List CI/CD packages")]
    ListCicd,
    #[cfg(feature = "iot")]
    #[command(name = "list-iot", about = "List IoT packages")]
    ListIot,
    #[cfg(feature = "app")]
    #[command(name = "list-app", about = "List app packages")]
    ListApp,
    #[cfg(feature = "lib")]
    #[command(name = "list-lib", about = "List library packages")]
    ListLib,

    // ── Per-core: update-<core> ────────────────────────────────
    #[cfg(all(
        feature = "web",
        any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        )
    ))]
    #[command(name = "update-web", about = "Update web packages")]
    UpdateWeb { packages: Vec<String> },
    #[cfg(feature = "game")]
    #[command(name = "update-game", about = "Update game packages")]
    UpdateGame { packages: Vec<String> },
    #[cfg(feature = "ai")]
    #[command(name = "update-ai", about = "Update AI packages")]
    UpdateAi { packages: Vec<String> },
    #[cfg(feature = "clo")]
    #[command(name = "update-clo", about = "Update cloud packages")]
    UpdateClo { packages: Vec<String> },
    #[cfg(feature = "cicd")]
    #[command(name = "update-cicd", about = "Update CI/CD packages")]
    UpdateCicd { packages: Vec<String> },
    #[cfg(feature = "iot")]
    #[command(name = "update-iot", about = "Update IoT packages")]
    UpdateIot { packages: Vec<String> },
    #[cfg(feature = "app")]
    #[command(name = "update-app", about = "Update app packages")]
    UpdateApp { packages: Vec<String> },
    #[cfg(feature = "lib")]
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
        #[cfg(all(
            feature = "web",
            any(
                feature = "game",
                feature = "ai",
                feature = "clo",
                feature = "cicd",
                feature = "iot",
                feature = "app",
                feature = "lib"
            )
        ))]
        Some(Commands::InstallWeb { packages }) => {
            commands::install::run(packages, Some("web")).await?;
        }
        #[cfg(feature = "game")]
        Some(Commands::InstallGame { packages }) => {
            commands::install::run(packages, Some("game")).await?;
        }
        #[cfg(feature = "ai")]
        Some(Commands::InstallAi { packages }) => {
            commands::install::run(packages, Some("ai")).await?;
        }
        #[cfg(feature = "clo")]
        Some(Commands::InstallClo { packages }) => {
            commands::install::run(packages, Some("clo")).await?;
        }
        #[cfg(feature = "cicd")]
        Some(Commands::InstallCicd { packages }) => {
            commands::install::run(packages, Some("cicd")).await?;
        }
        #[cfg(feature = "iot")]
        Some(Commands::InstallIot { packages }) => {
            commands::install::run(packages, Some("iot")).await?;
        }
        #[cfg(feature = "app")]
        Some(Commands::InstallApp { packages }) => {
            commands::install::run(packages, Some("app")).await?;
        }
        #[cfg(feature = "lib")]
        Some(Commands::InstallLib { packages }) => {
            commands::install::run(packages, Some("lib")).await?;
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
        Some(Commands::Add {
            packages,
            version,
            dev,
            global,
            exact,
            optional,
            peer,
            no_save,
        }) => {
            commands::add::run_many(
                packages, version, dev, exact, optional, peer, no_save, global, core,
            )
            .await?;
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
        Some(Commands::CreateWeb {
            framework,
            project_name,
            ts,
            tailwindcss,
            monorepo,
            backend,
            features,
        }) => {
            commands::create::run_with_options(
                "web",
                &framework,
                &project_name,
                Some(commands::create::WebCreateOptions {
                    typescript: ts,
                    tailwindcss,
                    monorepo,
                    backend,
                    features,
                }),
            )
            .await?;
        }
        #[cfg(feature = "game")]
        Some(Commands::CreateGame {
            framework,
            project_name,
        }) => {
            commands::create::run("game", &framework, &project_name).await?;
        }
        #[cfg(feature = "ai")]
        Some(Commands::CreateAi {
            framework,
            project_name,
        }) => {
            commands::create::run("ai", &framework, &project_name).await?;
        }
        #[cfg(feature = "clo")]
        Some(Commands::CreateClo {
            framework,
            project_name,
        }) => {
            commands::create::run("clo", &framework, &project_name).await?;
        }
        #[cfg(feature = "cicd")]
        Some(Commands::CreateCicd {
            framework,
            project_name,
        }) => {
            commands::create::run("cicd", &framework, &project_name).await?;
        }
        #[cfg(feature = "iot")]
        Some(Commands::CreateIot {
            framework,
            project_name,
        }) => {
            commands::create::run("iot", &framework, &project_name).await?;
        }
        #[cfg(feature = "app")]
        Some(Commands::CreateApp {
            framework,
            project_name,
        }) => {
            commands::create::run("app", &framework, &project_name).await?;
        }
        #[cfg(feature = "lib")]
        Some(Commands::CreateLib { project_name }) => {
            commands::create::run("lib", "rust", &project_name).await?;
        }

        // ── add-<core> ─────────────────────────────────────────
        #[cfg(all(
            feature = "web",
            any(
                feature = "game",
                feature = "ai",
                feature = "clo",
                feature = "cicd",
                feature = "iot",
                feature = "app",
                feature = "lib"
            )
        ))]
        Some(Commands::AddWeb {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        }) => {
            commands::add::run_many(
                packages,
                None,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                Some("web"),
            )
            .await?;
        }
        #[cfg(feature = "game")]
        Some(Commands::AddGame {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        }) => {
            commands::add::run_many(
                packages,
                None,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                Some("game"),
            )
            .await?;
        }
        #[cfg(feature = "ai")]
        Some(Commands::AddAi {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        }) => {
            commands::add::run_many(
                packages,
                None,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                Some("ai"),
            )
            .await?;
        }
        #[cfg(feature = "clo")]
        Some(Commands::AddClo {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        }) => {
            commands::add::run_many(
                packages,
                None,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                Some("clo"),
            )
            .await?;
        }
        #[cfg(feature = "cicd")]
        Some(Commands::AddCicd {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        }) => {
            commands::add::run_many(
                packages,
                None,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                Some("cicd"),
            )
            .await?;
        }
        #[cfg(feature = "iot")]
        Some(Commands::AddIot {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        }) => {
            commands::add::run_many(
                packages,
                None,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                Some("iot"),
            )
            .await?;
        }
        #[cfg(feature = "app")]
        Some(Commands::AddApp {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        }) => {
            commands::add::run_many(
                packages,
                None,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                Some("app"),
            )
            .await?;
        }
        #[cfg(feature = "lib")]
        Some(Commands::AddLib {
            packages,
            dev,
            exact,
            optional,
            peer,
            no_save,
            global,
        }) => {
            commands::add::run_many(
                packages,
                None,
                dev,
                exact,
                optional,
                peer,
                no_save,
                global,
                Some("lib"),
            )
            .await?;
        }

        // ── remove-<core> ──────────────────────────────────────
        #[cfg(all(
            feature = "web",
            any(
                feature = "game",
                feature = "ai",
                feature = "clo",
                feature = "cicd",
                feature = "iot",
                feature = "app",
                feature = "lib"
            )
        ))]
        Some(Commands::RemoveWeb { package }) => {
            commands::remove::run(package, Some("web")).await?;
        }
        #[cfg(feature = "game")]
        Some(Commands::RemoveGame { package }) => {
            commands::remove::run(package, Some("game")).await?;
        }
        #[cfg(feature = "ai")]
        Some(Commands::RemoveAi { package }) => {
            commands::remove::run(package, Some("ai")).await?;
        }
        #[cfg(feature = "clo")]
        Some(Commands::RemoveClo { package }) => {
            commands::remove::run(package, Some("clo")).await?;
        }
        #[cfg(feature = "cicd")]
        Some(Commands::RemoveCicd { package }) => {
            commands::remove::run(package, Some("cicd")).await?;
        }
        #[cfg(feature = "iot")]
        Some(Commands::RemoveIot { package }) => {
            commands::remove::run(package, Some("iot")).await?;
        }
        #[cfg(feature = "app")]
        Some(Commands::RemoveApp { package }) => {
            commands::remove::run(package, Some("app")).await?;
        }
        #[cfg(feature = "lib")]
        Some(Commands::RemoveLib { package }) => {
            commands::remove::run(package, Some("lib")).await?;
        }

        // ── list-<core> ────────────────────────────────────────
        #[cfg(all(
            feature = "web",
            any(
                feature = "game",
                feature = "ai",
                feature = "clo",
                feature = "cicd",
                feature = "iot",
                feature = "app",
                feature = "lib"
            )
        ))]
        Some(Commands::ListWeb) => commands::list::run(Some("web")).await?,
        #[cfg(feature = "game")]
        Some(Commands::ListGame) => commands::list::run(Some("game")).await?,
        #[cfg(feature = "ai")]
        Some(Commands::ListAi) => commands::list::run(Some("ai")).await?,
        #[cfg(feature = "clo")]
        Some(Commands::ListClo) => commands::list::run(Some("clo")).await?,
        #[cfg(feature = "cicd")]
        Some(Commands::ListCicd) => commands::list::run(Some("cicd")).await?,
        #[cfg(feature = "iot")]
        Some(Commands::ListIot) => commands::list::run(Some("iot")).await?,
        #[cfg(feature = "app")]
        Some(Commands::ListApp) => commands::list::run(Some("app")).await?,
        #[cfg(feature = "lib")]
        Some(Commands::ListLib) => commands::list::run(Some("lib")).await?,

        // ── update-<core> ──────────────────────────────────────
        #[cfg(all(
            feature = "web",
            any(
                feature = "game",
                feature = "ai",
                feature = "clo",
                feature = "cicd",
                feature = "iot",
                feature = "app",
                feature = "lib"
            )
        ))]
        Some(Commands::UpdateWeb { packages }) => {
            commands::update::run(packages, Some("web")).await?;
        }
        #[cfg(feature = "game")]
        Some(Commands::UpdateGame { packages }) => {
            commands::update::run(packages, Some("game")).await?;
        }
        #[cfg(feature = "ai")]
        Some(Commands::UpdateAi { packages }) => {
            commands::update::run(packages, Some("ai")).await?;
        }
        #[cfg(feature = "clo")]
        Some(Commands::UpdateClo { packages }) => {
            commands::update::run(packages, Some("clo")).await?;
        }
        #[cfg(feature = "cicd")]
        Some(Commands::UpdateCicd { packages }) => {
            commands::update::run(packages, Some("cicd")).await?;
        }
        #[cfg(feature = "iot")]
        Some(Commands::UpdateIot { packages }) => {
            commands::update::run(packages, Some("iot")).await?;
        }
        #[cfg(feature = "app")]
        Some(Commands::UpdateApp { packages }) => {
            commands::update::run(packages, Some("app")).await?;
        }
        #[cfg(feature = "lib")]
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    const IS_WEB_ONLY_BUILD: bool = cfg!(all(
        feature = "web",
        not(feature = "game"),
        not(feature = "ai"),
        not(feature = "clo"),
        not(feature = "cicd"),
        not(feature = "iot"),
        not(feature = "app"),
        not(feature = "lib")
    ));

    #[test]
    fn test_create_web_accepts_flags() {
        let command = if IS_WEB_ONLY_BUILD {
            "create"
        } else {
            "create-web"
        };
        let cli = Cli::try_parse_from([
            "mg",
            command,
            "react@latest",
            "demo-app",
            "--ts",
            "--tailwindcss",
        ])
        .unwrap();

        match cli.command.unwrap() {
            Commands::CreateWeb {
                framework,
                project_name,
                ts,
                tailwindcss,
                ..
            } => {
                assert_eq!(framework, "react@latest");
                assert_eq!(project_name, "demo-app");
                assert!(ts);
                assert!(tailwindcss);
            }
            _ => panic!("expected create-web command"),
        }
    }

    #[test]
    fn test_add_web_accepts_multiple_packages() {
        let command = if IS_WEB_ONLY_BUILD { "add" } else { "add-web" };
        let cli =
            Cli::try_parse_from(["mg", command, "zod", "lodash", "@types/node", "-D"]).unwrap();

        match cli.command.unwrap() {
            Commands::Add { packages, dev, .. } if IS_WEB_ONLY_BUILD => {
                assert_eq!(packages, vec!["zod", "lodash", "@types/node"]);
                assert!(dev);
            }
            #[cfg(all(
                feature = "web",
                any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                )
            ))]
            Commands::AddWeb { packages, dev, .. } => {
                assert_eq!(packages, vec!["zod", "lodash", "@types/node"]);
                assert!(dev);
            }
            _ => panic!("expected web add command"),
        }
    }

    #[test]
    fn test_available_cores_matches_build_shape() {
        let available = crate::factory::available_cores();

        if IS_WEB_ONLY_BUILD {
            assert_eq!(available.len(), 1);
            assert_eq!(available[0].0, "web");
        } else {
            assert!(
                available.len() >= 2,
                "expected multi-core build to expose at least 2 cores, got {available:?}"
            );
        }
    }

    #[test]
    fn test_create_alias_behavior_matches_build_shape() {
        let parsed = Cli::try_parse_from(["mg", "create", "react@latest", "demo-app", "--ts"]);

        if IS_WEB_ONLY_BUILD {
            let cli = parsed.expect("web-only build should accept `mg create ...`");
            match cli.command.unwrap() {
                Commands::CreateWeb {
                    framework,
                    project_name,
                    ts,
                    ..
                } => {
                    assert_eq!(framework, "react@latest");
                    assert_eq!(project_name, "demo-app");
                    assert!(ts);
                }
                _ => panic!("expected create alias to resolve to web scaffold"),
            }
        } else {
            assert!(
                parsed.is_err(),
                "multi-core build should require `mg create-<core> ...`"
            );
        }
    }

    #[test]
    fn test_help_surface_matches_build_shape() {
        let help = Cli::command().render_long_help().to_string();

        if IS_WEB_ONLY_BUILD {
            assert!(
                help.contains("create"),
                "web-only build should expose bare `create` command in help:\n{help}"
            );
            assert!(
                !help.contains("create-game"),
                "web-only build should not expose game commands in help:\n{help}"
            );
            assert!(
                !help.contains("create-web   Scaffold"),
                "web-only build should not expose multi-core command name in help:\n{help}"
            );
            assert!(
                !help.contains("add-game"),
                "web-only build should not expose non-web add commands in help:\n{help}"
            );
            assert!(
                !help.contains("install-web"),
                "web-only build should keep install as bare command:\n{help}"
            );
        } else {
            assert!(help.contains("install-web"));
            assert!(help.contains("create-web"));
            assert!(help.contains("create-game"));
            assert!(help.contains("add-web"));
            assert!(help.contains("add-game"));
        }
    }

    #[test]
    fn test_install_command_surface_matches_build_shape() {
        if IS_WEB_ONLY_BUILD {
            let cli = Cli::try_parse_from(["mg", "install", "react", "vite"]).unwrap();
            match cli.command.unwrap() {
                Commands::Install { packages } => {
                    assert_eq!(packages, vec!["react", "vite"]);
                }
                _ => panic!("expected bare install in web-only build"),
            }
            assert!(Cli::try_parse_from(["mg", "install-web"]).is_err());
        } else {
            #[cfg(all(
                feature = "web",
                any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                )
            ))]
            {
                let cli = Cli::try_parse_from(["mg", "install-web", "react", "vite"]).unwrap();
                match cli.command.unwrap() {
                    Commands::InstallWeb { packages } => {
                        assert_eq!(packages, vec!["react", "vite"]);
                    }
                    _ => panic!("expected install-web in multi-core build"),
                }
            }
        }
    }
}
