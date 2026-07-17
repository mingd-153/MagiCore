/// MegaGate CLI - Universal Package Manager
use anyhow::Result;
use clap::{Parser, Subcommand};

mod bundler;
mod commands;
mod context;
mod dispatch;
mod factory;
mod scaffold;
mod wizard;

#[derive(Parser)]
#[command(name = "mg")]
#[command(about = "MegaGate - Universal Package Manager", long_about = None)]
#[command(version)]
pub(crate) struct Cli {
    /// Target core (web, game, ai, clo, cicd, iot, app, lib)
    #[arg(global = true, long)]
    core: Option<String>,

    /// Fail installations if packages are under quarantine (published < 24h)
    #[arg(global = true, long)]
    audit_strict: bool,

    /// Run the command for each project in the workspace
    #[arg(global = true, short = 'r', long)]
    recursive: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Clone)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Commands {
    // ── Common / Global commands ────────────────────────────────────────
    #[command(about = "Interactive project wizard")]
    Init {
        #[arg(short, long)]
        template: Option<String>,
    },
    #[command(about = "Show package information")]
    Info {
        package: String,
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "Search for packages")]
    Search {
        query: String,
        #[arg(long, help = "Output as JSON")]
        json: bool,
        #[arg(long, help = "Exact match search")]
        exact: bool,
        #[arg(long, help = "Page number (20 results per page)")]
        page: Option<u32>,
    },
    #[command(about = "Check for outdated packages")]
    Outdated {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "Audit packages for vulnerabilities")]
    Audit,
    #[command(about = "Update MegaGate CLI to the latest version")]
    SelfUpdate,

    // ── Engine Commands (In-project, auto-detect core) ───────────────
    #[command(about = "Start the local development server", alias = "dev-web")]
    Dev {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long, help = "Clear terminal on each reload")]
        clear: bool,
    },
    #[command(about = "Run a script defined in package.json")]
    Run {
        #[arg(required = true)]
        script: String,
        #[arg(last = true)]
        args: Vec<String>,
    },
    #[command(about = "Build the project")]
    Build,
    #[command(about = "Start the production server")]
    Start,
    #[command(about = "Execute a shell command in scope of a project")]
    Exec {
        #[arg(required = true)]
        command: String,
        #[arg(last = true)]
        args: Vec<String>,
    },
    #[command(about = "Download and execute a package without permanently installing it")]
    Dlx {
        #[arg(required = true)]
        package: String,
        #[arg(last = true)]
        args: Vec<String>,
    },

    // ── Bare Commands (In-project, auto-detect core from signature) ──
    #[command(about = "Install dependencies (auto-detect core)", alias = "i")]
    Install {
        packages: Vec<String>,
        #[arg(long, help = "Fail if mg.lock is missing or outdated (CI mode)")]
        frozen: bool,
        #[arg(long, help = "Skip running lifecycle scripts")]
        ignore_scripts: bool,
    },
    #[command(about = "Add a dependency (auto-detect core)")]
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
    #[command(about = "Remove a dependency (auto-detect core)", alias = "rm")]
    Remove { package: String },
    #[command(about = "Update packages (auto-detect core)", alias = "up")]
    Update {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(about = "List installed packages (auto-detect core)", alias = "ls")]
    List,
    #[command(about = "Connect the local project to another one", alias = "ln")]
    Link { package: Option<String> },
    #[command(about = "Unlinks a package")]
    Unlink { package: Option<String> },
    #[command(about = "Shows all packages that depend on the specified package")]
    Why { package: String },

    // ── Per-core: create-<core> ────────────────────────────────
    #[command(name = "create-web", about = "Scaffold a new web project")]
    CreateWeb {
        framework: String,
        project_name: String,
        #[command(flatten)]
        flags: crate::commands::core::scaffold_flags::ScaffoldFlags,
    },
    #[command(name = "create-game", about = "Scaffold a new game project")]
    CreateGame {
        framework: String,
        project_name: String,
    },
    #[command(name = "create-ai", about = "Scaffold a new AI project")]
    CreateAi {
        framework: String,
        project_name: String,
    },
    #[command(name = "create-clo", about = "Scaffold a new cloud project")]
    CreateClo {
        framework: String,
        project_name: String,
    },
    #[command(name = "create-cicd", about = "Scaffold a new CI/CD project")]
    CreateCicd {
        framework: String,
        project_name: String,
    },
    #[command(name = "create-iot", about = "Scaffold a new IoT project")]
    CreateIot {
        framework: String,
        project_name: String,
    },
    #[command(name = "create-app", about = "Scaffold a new app project")]
    CreateApp {
        framework: String,
        project_name: String,
    },
    #[command(name = "create-lib", about = "Scaffold a new library project")]
    CreateLib { project_name: String },

    // ── Per-core: install-<core> ───────────────────────────────────
    #[command(name = "install-web", about = "Install web dependencies")]
    InstallWeb {
        packages: Vec<String>,
        #[arg(long, help = "Fail if mg.lock is missing or outdated (CI mode)")]
        frozen: bool,
        #[arg(long, help = "Skip running lifecycle scripts")]
        ignore_scripts: bool,
    },
    #[command(name = "install-game", about = "Install game dependencies")]
    InstallGame { packages: Vec<String> },
    #[command(name = "install-ai", about = "Install AI dependencies")]
    InstallAi { packages: Vec<String> },
    #[command(name = "install-clo", about = "Install cloud dependencies")]
    InstallClo { packages: Vec<String> },
    #[command(name = "install-cicd", about = "Install CI/CD dependencies")]
    InstallCicd { packages: Vec<String> },
    #[command(name = "install-iot", about = "Install IoT dependencies")]
    InstallIot { packages: Vec<String> },
    #[command(name = "install-app", about = "Install app dependencies")]
    InstallApp { packages: Vec<String> },
    #[command(name = "install-lib", about = "Install library dependencies")]
    InstallLib { packages: Vec<String> },

    // ── Per-core: add-<core> ───────────────────────────────────
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
    UpdateWeb {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(name = "update-game", about = "Update game packages")]
    UpdateGame {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(name = "update-ai", about = "Update AI packages")]
    UpdateAi {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(name = "update-clo", about = "Update cloud packages")]
    UpdateClo {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(name = "update-cicd", about = "Update CI/CD packages")]
    UpdateCicd {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(name = "update-iot", about = "Update IoT packages")]
    UpdateIot {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(name = "update-app", about = "Update app packages")]
    UpdateApp {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(name = "update-lib", about = "Update library packages")]
    UpdateLib {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dispatch::run(Cli::parse()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_create_web_accepts_flags() {
        let cli = Cli::try_parse_from([
            "mg",
            "create-web",
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
                flags,
            } => {
                assert_eq!(framework, "react@latest");
                assert_eq!(project_name, "demo-app");
                assert!(flags.ts);
                assert!(flags.tailwindcss);
            }
            _ => panic!("expected create-web command"),
        }
    }

    #[test]
    fn test_add_web_accepts_multiple_packages() {
        let cli =
            Cli::try_parse_from(["mg", "add-web", "zod", "lodash", "@types/node", "-D"]).unwrap();

        match cli.command.unwrap() {
            Commands::AddWeb { packages, dev, .. } => {
                assert_eq!(packages, vec!["zod", "lodash", "@types/node"]);
                assert!(dev);
            }
            _ => panic!("expected add-web command"),
        }
    }

    #[test]
    fn test_available_cores_matches_build_shape() {
        let available = crate::factory::available_cores();
        assert!(
            available.len() >= 8,
            "expected full multi-core build to expose 8 cores, got {available:?}"
        );
    }

    #[test]
    fn test_help_surface_matches_build_shape() {
        let help = Cli::command().render_long_help().to_string();

        assert!(help.contains("dev"));
        assert!(help.contains("install-web"));
        assert!(help.contains("create-web"));
        assert!(help.contains("add-web"));
        assert!(help.contains("create-game"));
        assert!(help.contains("add-game"));

        // Bare aliases for single-core no longer exist, ensure they're missing or handled gracefully
        assert!(!help.contains("create   "));
    }

    #[test]
    fn test_dev_command_accepts_host_and_port() {
        let cli =
            Cli::try_parse_from(["mg", "dev", "--host", "127.0.0.1", "--port", "4315"]).unwrap();

        match cli.command.unwrap() {
            Commands::Dev {
                host,
                port,
                clear: _,
            } => {
                assert_eq!(host.as_deref(), Some("127.0.0.1"));
                assert_eq!(port, Some(4315));
            }
            _ => panic!("expected dev command"),
        }
    }
}
