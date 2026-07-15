/// MegaGate CLI - Universal Package Manager
use anyhow::Result;
use clap::{Parser, Subcommand};

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

    #[command(subcommand)]
    command: Option<Commands>,
}

// ─── Per-core commands (8 cores × 5 commands = 40 variants) ───────
// Global mode:   mg add-web react, mg remove-web lodash, ...
// Single mode:   mg add react, mg remove lodash, ...

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Commands {
    // ── Common commands ────────────────────────────────────────
    #[command(about = "Interactive project wizard")]
    Init {
        #[arg(short, long)]
        template: Option<String>,
    },
    #[command(about = "Start the local development server", alias = "dev-web")]
    Dev {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
    },
    #[cfg(all(
        feature = "web",
        not(any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        ))
    ))]
    #[command(about = "Install all dependencies", alias = "install-web")]
    Install {
        packages: Vec<String>,
        #[arg(long, help = "Fail if mg.lock is missing or outdated (CI mode)")]
        frozen: bool,
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
    #[command(name = "install-web", about = "Install web dependencies")]
    InstallWeb {
        packages: Vec<String>,
        #[arg(long, help = "Fail if mg.lock is missing or outdated (CI mode)")]
        frozen: bool,
    },
    #[cfg(feature = "game")]
    #[command(
        name = "install-game",
        about = "Install game dependencies",
        hide = true
    )]
    InstallGame { packages: Vec<String> },
    #[cfg(feature = "ai")]
    #[command(name = "install-ai", about = "Install AI dependencies", hide = true)]
    InstallAi { packages: Vec<String> },
    #[cfg(feature = "clo")]
    #[command(
        name = "install-clo",
        about = "Install cloud dependencies",
        hide = true
    )]
    InstallClo { packages: Vec<String> },
    #[cfg(feature = "cicd")]
    #[command(
        name = "install-cicd",
        about = "Install CI/CD dependencies",
        hide = true
    )]
    InstallCicd { packages: Vec<String> },
    #[cfg(feature = "iot")]
    #[command(name = "install-iot", about = "Install IoT dependencies", hide = true)]
    InstallIot { packages: Vec<String> },
    #[cfg(feature = "app")]
    #[command(name = "install-app", about = "Install app dependencies", hide = true)]
    InstallApp { packages: Vec<String> },
    #[cfg(feature = "lib")]
    #[command(
        name = "install-lib",
        about = "Install library dependencies",
        hide = true
    )]
    InstallLib { packages: Vec<String> },
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

    // ── Single-core (bare) — only in single-core builds ────────
    #[cfg(all(
        feature = "web",
        not(any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        ))
    ))]
    #[command(about = "Add a dependency", alias = "add-web")]
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
    #[cfg(all(
        feature = "web",
        not(any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        ))
    ))]
    #[command(about = "Remove a dependency", alias = "remove-web")]
    Remove { package: String },
    #[cfg(all(
        feature = "web",
        not(any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        ))
    ))]
    #[command(about = "Update packages", alias = "update-web")]
    Update {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[cfg(all(
        feature = "web",
        not(any(
            feature = "game",
            feature = "ai",
            feature = "clo",
            feature = "cicd",
            feature = "iot",
            feature = "app",
            feature = "lib"
        ))
    ))]
    #[command(about = "List installed packages", alias = "list-web")]
    List,

    // ── Bare (multi-core auto-detect from .megagate/) ──────────
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
    #[command(about = "Install dependencies (auto-detect core)")]
    Install {
        packages: Vec<String>,
        #[arg(long, help = "Fail if mg.lock is missing or outdated (CI mode)")]
        frozen: bool,
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
    #[command(about = "Remove a dependency (auto-detect core)")]
    Remove { package: String },
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
    #[command(about = "Update packages (auto-detect core)")]
    Update {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
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
    #[command(about = "List installed packages (auto-detect core)")]
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
        about = "Scaffold a new web project",
        alias = "create-web"
    )]
    CreateWeb {
        framework: String,
        project_name: String,
        #[command(flatten)]
        flags: crate::commands::core::scaffold_flags::ScaffoldFlags,
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
        #[command(flatten)]
        flags: crate::commands::core::scaffold_flags::ScaffoldFlags,
    },
    #[cfg(feature = "game")]
    #[command(
        name = "create-game",
        about = "Scaffold a new game project",
        hide = true
    )]
    CreateGame {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "ai")]
    #[command(name = "create-ai", about = "Scaffold a new AI project", hide = true)]
    CreateAi {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "clo")]
    #[command(
        name = "create-clo",
        about = "Scaffold a new cloud project",
        hide = true
    )]
    CreateClo {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "cicd")]
    #[command(
        name = "create-cicd",
        about = "Scaffold a new CI/CD project",
        hide = true
    )]
    CreateCicd {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "iot")]
    #[command(name = "create-iot", about = "Scaffold a new IoT project", hide = true)]
    CreateIot {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "app")]
    #[command(name = "create-app", about = "Scaffold a new app project", hide = true)]
    CreateApp {
        framework: String,
        project_name: String,
    },
    #[cfg(feature = "lib")]
    #[command(
        name = "create-lib",
        about = "Scaffold a new library project",
        hide = true
    )]
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
    #[command(name = "add-game", about = "Add game dependency", hide = true)]
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
    #[command(name = "add-ai", about = "Add AI dependency", hide = true)]
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
    #[command(name = "add-clo", about = "Add cloud dependency", hide = true)]
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
    #[command(name = "add-cicd", about = "Add CI/CD dependency", hide = true)]
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
    #[command(name = "add-iot", about = "Add IoT dependency", hide = true)]
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
    #[command(name = "add-app", about = "Add app dependency", hide = true)]
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
    #[command(name = "add-lib", about = "Add library dependency", hide = true)]
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
    #[command(name = "remove-game", about = "Remove game dependency", hide = true)]
    RemoveGame { package: String },
    #[cfg(feature = "ai")]
    #[command(name = "remove-ai", about = "Remove AI dependency", hide = true)]
    RemoveAi { package: String },
    #[cfg(feature = "clo")]
    #[command(name = "remove-clo", about = "Remove cloud dependency", hide = true)]
    RemoveClo { package: String },
    #[cfg(feature = "cicd")]
    #[command(name = "remove-cicd", about = "Remove CI/CD dependency", hide = true)]
    RemoveCicd { package: String },
    #[cfg(feature = "iot")]
    #[command(name = "remove-iot", about = "Remove IoT dependency", hide = true)]
    RemoveIot { package: String },
    #[cfg(feature = "app")]
    #[command(name = "remove-app", about = "Remove app dependency", hide = true)]
    RemoveApp { package: String },
    #[cfg(feature = "lib")]
    #[command(name = "remove-lib", about = "Remove library dependency", hide = true)]
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
    #[command(name = "list-game", about = "List game packages", hide = true)]
    ListGame,
    #[cfg(feature = "ai")]
    #[command(name = "list-ai", about = "List AI packages", hide = true)]
    ListAi,
    #[cfg(feature = "clo")]
    #[command(name = "list-clo", about = "List cloud packages", hide = true)]
    ListClo,
    #[cfg(feature = "cicd")]
    #[command(name = "list-cicd", about = "List CI/CD packages", hide = true)]
    ListCicd,
    #[cfg(feature = "iot")]
    #[command(name = "list-iot", about = "List IoT packages", hide = true)]
    ListIot,
    #[cfg(feature = "app")]
    #[command(name = "list-app", about = "List app packages", hide = true)]
    ListApp,
    #[cfg(feature = "lib")]
    #[command(name = "list-lib", about = "List library packages", hide = true)]
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
    UpdateWeb {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[cfg(feature = "game")]
    #[command(name = "update-game", about = "Update game packages", hide = true)]
    UpdateGame {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[cfg(feature = "ai")]
    #[command(name = "update-ai", about = "Update AI packages", hide = true)]
    UpdateAi {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[cfg(feature = "clo")]
    #[command(name = "update-clo", about = "Update cloud packages", hide = true)]
    UpdateClo {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[cfg(feature = "cicd")]
    #[command(name = "update-cicd", about = "Update CI/CD packages", hide = true)]
    UpdateCicd {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[cfg(feature = "iot")]
    #[command(name = "update-iot", about = "Update IoT packages", hide = true)]
    UpdateIot {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[cfg(feature = "app")]
    #[command(name = "update-app", about = "Update app packages", hide = true)]
    UpdateApp {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[cfg(feature = "lib")]
    #[command(name = "update-lib", about = "Update library packages", hide = true)]
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
        let command = if IS_WEB_ONLY_BUILD { "add" } else { "add-web" };
        let cli =
            Cli::try_parse_from(["mg", command, "zod", "lodash", "@types/node", "-D"]).unwrap();

        match cli.command.unwrap() {
            #[cfg(all(
                feature = "web",
                not(any(
                    feature = "game",
                    feature = "ai",
                    feature = "clo",
                    feature = "cicd",
                    feature = "iot",
                    feature = "app",
                    feature = "lib"
                ))
            ))]
            Commands::Add { packages, dev, .. } => {
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
        if IS_WEB_ONLY_BUILD {
            let cli = Cli::try_parse_from(["mg", "create", "react@latest", "demo-app", "--ts"])
                .expect("web-only build should accept `mg create ...`");
            match cli.command.unwrap() {
                Commands::CreateWeb {
                    framework,
                    project_name,
                    flags,
                } => {
                    assert_eq!(framework, "react@latest");
                    assert_eq!(project_name, "demo-app");
                    assert!(flags.ts);
                }
                _ => panic!("expected create alias to resolve to web scaffold"),
            }
            let cli = Cli::try_parse_from(["mg", "create-web", "react@latest", "demo-app", "--ts"])
                .expect(
                    "web-only build should also accept `mg create-web ...` as compatibility alias",
                );
            match cli.command.unwrap() {
                Commands::CreateWeb {
                    framework,
                    project_name,
                    flags,
                } => {
                    assert_eq!(framework, "react@latest");
                    assert_eq!(project_name, "demo-app");
                    assert!(flags.ts);
                }
                _ => panic!("expected create-web alias to resolve to web scaffold"),
            }
        } else {
            assert!(
                Cli::try_parse_from(["mg", "create", "react@latest", "demo-app", "--ts"]).is_err(),
                "multi-core build should reject bare `mg create`"
            );
            // But create-web works
            let cli = Cli::try_parse_from(["mg", "create-web", "react@latest", "demo-app", "--ts"])
                .expect("multi-core build should accept `mg create-web ...`");
            match cli.command.unwrap() {
                Commands::CreateWeb {
                    framework,
                    project_name,
                    flags,
                } => {
                    assert_eq!(framework, "react@latest");
                    assert_eq!(project_name, "demo-app");
                    assert!(flags.ts);
                }
                _ => panic!("expected create-web in multi-core build"),
            }
        }
    }

    #[test]
    fn test_help_surface_matches_build_shape() {
        let help = Cli::command().render_long_help().to_string();

        assert!(help.contains("dev"));

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
            assert!(help.contains("add-web"));
            assert!(!help.contains("create-game"));
            assert!(!help.contains("add-game"));
            assert!(
                !help.contains("create "),
                "multi-core should not have bare create"
            );
        }
    }

    #[test]
    fn test_install_command_surface_matches_build_shape() {
        #[cfg(all(
            feature = "web",
            not(any(
                feature = "game",
                feature = "ai",
                feature = "clo",
                feature = "cicd",
                feature = "iot",
                feature = "app",
                feature = "lib"
            ))
        ))]
        {
            let cli = Cli::try_parse_from(["mg", "install", "react", "vite"]).unwrap();
            match cli.command.unwrap() {
                Commands::Install {
                    packages, frozen, ..
                } => {
                    assert_eq!(packages, vec!["react", "vite"]);
                    assert!(!frozen);
                }
                _ => panic!("expected bare install in web-only build"),
            }
            let cli = Cli::try_parse_from(["mg", "install-web"]).unwrap();
            match cli.command.unwrap() {
                Commands::Install {
                    packages, frozen, ..
                } => {
                    assert!(packages.is_empty());
                    assert!(!frozen);
                }
                _ => panic!("expected install-web alias in web-only build"),
            }
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
        {
            let cli = Cli::try_parse_from(["mg", "install", "react_vite"]).unwrap();
            match cli.command.unwrap() {
                Commands::Install {
                    packages, frozen, ..
                } => {
                    assert_eq!(packages, vec!["react_vite"]);
                    assert!(!frozen);
                }
                _ => panic!("expected auto-detect install in multi-core build"),
            }
            let cli = Cli::try_parse_from(["mg", "install-web", "react", "vite"]).unwrap();
            match cli.command.unwrap() {
                Commands::InstallWeb {
                    packages, frozen, ..
                } => {
                    assert_eq!(packages, vec!["react", "vite"]);
                    assert!(!frozen);
                }
                _ => panic!("expected install-web in multi-core build"),
            }
        }
    }

    #[test]
    fn test_dev_command_accepts_host_and_port() {
        let cli =
            Cli::try_parse_from(["mg", "dev", "--host", "127.0.0.1", "--port", "4315"]).unwrap();

        match cli.command.unwrap() {
            Commands::Dev { host, port } => {
                assert_eq!(host.as_deref(), Some("127.0.0.1"));
                assert_eq!(port, Some(4315));
            }
            _ => panic!("expected dev command"),
        }

        let alias_cli =
            Cli::try_parse_from(["mg", "dev-web", "--host", "127.0.0.1", "--port", "4316"])
                .unwrap();

        match alias_cli.command.unwrap() {
            Commands::Dev { host, port } => {
                assert_eq!(host.as_deref(), Some("127.0.0.1"));
                assert_eq!(port, Some(4316));
            }
            _ => panic!("expected dev alias command"),
        }
    }
}
