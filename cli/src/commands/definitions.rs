//! Clap command definitions — tách từ main.rs (Phase 7 v5 — user chốt 2026-08-19).
//! main.rs chỉ parse + dispatch; enum ~80 lệnh ở đây (định nghĩa lệnh, không logic).

use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand, Clone)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Commands {
    // ── Common / Global commands ────────────────────────────────────────
    #[command(about = "Interactive project wizard")]
    Init {
        #[arg(short, long)]
        template: Option<String>,
        #[arg(
            long,
            help = "Write core signature marker (.mg.core) for the current project, no wizard"
        )]
        signature: Option<String>,
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
    Audit {
        #[arg(
            long,
            help = "Bump vulnerable packages and rewrite lockfile on success"
        )]
        fix: bool,
    },
    #[command(about = "Update MegaGate CLI to the latest version")]
    SelfUpdate,
    #[command(about = "Read/write configuration (.npmrc)", alias = "c")]
    Config {
        #[command(subcommand)]
        cmd: crate::commands::config::ConfigCmd,
        #[arg(
            long,
            global = true,
            help = "read/write project .npmrc instead of ~/.npmrc"
        )]
        local: bool,
    },
    #[command(about = "Stage the package (pack + verify, no upload)")]
    Stage {
        #[arg(long, help = "stage a different project directory")]
        dir: Option<std::path::PathBuf>,
    },
    #[command(
        about = "Import legacy lockfile (package-lock.json, pnpm-lock.yaml, yarn.lock, bun.lock) to mg.lock"
    )]
    Import {
        #[arg(long, help = "Target project directory to import")]
        dir: Option<std::path::PathBuf>,
    },

    // ── W6: SBOM Export ──────────────────────────────────────────────
    #[command(about = "Generate Software Bill of Materials (SBOM) from lockfile")]
    Sbom {
        #[arg(long, help = "Output format (cyclonedx-json, cyclonedx-xml, spdx-json)")]
        format: Option<String>,
        #[arg(long, help = "Output file path (default: stdout)")]
        output: Option<std::path::PathBuf>,
        #[arg(long, help = "Component name (default: project name)")]
        name: Option<String>,
        #[arg(long, help = "Component version (default: project version)")]
        version: Option<String>,
        #[arg(long, help = "Target project directory")]
        dir: Option<std::path::PathBuf>,
    },

    // ── Common: Publish ──────────────────────────────────────────────
    #[command(about = "Publish package to registry")]
    Publish {
        #[arg(long, help = "dist-tag (default: latest)")]
        tag: Option<String>,
        #[arg(long, help = "access level: public|restricted")]
        access: Option<String>,
        #[arg(long, help = "pack + verify, do not publish")]
        dry_run: bool,
        #[arg(long, help = "output JSON")]
        json: bool,
        #[arg(long, help = "OTP code for 2FA")]
        otp: Option<String>,
        #[arg(long, help = "delete existing version then republish (409)")]
        force: bool,
        #[arg(long, help = "skip lifecycle scripts")]
        ignore_scripts: bool,
        #[arg(long, help = "skip git checks")]
        no_git_checks: bool,
        #[arg(
            long,
            help = "publish branch (default: current branch tracking remote)"
        )]
        publish_branch: Option<String>,
        #[arg(long, help = "all in one PUT (Phase 3)")]
        batch: bool,
        #[arg(long, help = "write JSON report file")]
        report_summary: bool,
        #[arg(long, help = "version bump patch")]
        patch: bool,
        #[arg(long, help = "version bump minor")]
        minor: bool,
        #[arg(long, help = "version bump major")]
        major: bool,
        #[arg(long, help = "override registry URL")]
        registry: Option<String>,
        #[arg(long, help = "override token (env MG_NPM_TOKEN recommended)")]
        token: Option<String>,
    },

    // ── Common: Patch & Dedupe ───────────────────────────────────────
    #[command(about = "Manage package patches (add/rm/ls/verify)")]
    Patch {
        #[command(subcommand)]
        cmd: crate::commands::patch::PatchCmd,
    },
    #[command(about = "Deduplicate packages in lockfile (merge duplicates)")]
    Dedupe {
        #[arg(long, help = "report only, do not apply changes")]
        dry_run: bool,
        #[arg(long, help = "prefer latest version over existing instances")]
        prefer_latest: bool,
        #[arg(long, help = "output JSON")]
        json: bool,
    },

    // ── Registry (Phase 3) ──────────────────────────────────────────
    #[command(about = "Login to a registry (adduser flow, save token to .npmrc)")]
    Login {
        #[arg(long, help = "registry URL (default: config or npmjs)")]
        registry: Option<String>,
        #[arg(long, help = "username (prompt if omitted)")]
        username: Option<String>,
        #[arg(long, help = "password (prompt if omitted)")]
        password: Option<String>,
        #[arg(long, help = "write token to project .npmrc instead of ~/.npmrc")]
        local: bool,
    },
    #[command(about = "Manage the private registry (serve, user add/rm)")]
    Registry {
        #[command(subcommand)]
        cmd: crate::commands::registry::RegistryCmd,
    },
    #[command(about = "Push/pull/list AI models qua OCI registry")]
    Model {
        #[command(subcommand)]
        cmd: crate::commands::model::ModelCmd,
    },
    #[command(about = "Start native Model Context Protocol (MCP) server for AI coding agents")]
    Mcp,

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
    Build {
        #[arg(long, help = "Build target (e.g., native, browser, server)")]
        target: Option<String>,
    },
    #[command(about = "Flash firmware to a device (IoT esp32)")]
    Flash {
        #[arg(long, help = "Board override (e.g., esp32c3, esp32s3)")]
        board: Option<String>,
        #[arg(long, help = "Skip cargo build, flash existing binary")]
        skip_build: bool,
    },
    #[command(about = "Deploy cloud infrastructure (dry-run default)")]
    Deploy {
        #[arg(long, help = "Actually run the deploy (default is dry-run print-only)")]
        run: bool,
    },
    #[command(about = "Generate CI pipeline (github-actions workflows)")]
    CiGenerate,
    #[command(about = "Run verify chain (audit → test → build) per project core")]
    Verify,
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
    #[command(about = "Inspect or clean MegaGate caches")]
    Cache {
        #[arg(value_parser = ["status", "clean", "prune"])]
        action: String,
        #[arg(long, default_value = "all", value_parser = ["all", "shared", "project", "build"])]
        target: String,
        #[arg(long, help = "Required for cache clean")]
        yes: bool,
        #[arg(long, help = "Preview cache prune without deleting files")]
        dry_run: bool,
    },

    // ── Bare Commands (In-project, auto-detect core from signature) ──
    #[command(about = "Install dependencies (auto-detect core)", alias = "i")]
    Install {
        packages: Vec<String>,
        #[arg(long, help = "Fail if mg.lock is missing or outdated (CI mode)")]
        frozen: bool,
        #[arg(long, help = "Skip running lifecycle scripts")]
        ignore_scripts: bool,
        #[arg(long, help = "Allow dependency lifecycle scripts")]
        allow_scripts: bool,
        #[arg(
            long,
            help = "Prefer reusing installed versions instead of latest (dedupe, opt-in)"
        )]
        prefer_dedupe: bool,
        #[arg(long, help = "Re-link dangling symlinks in node_modules")]
        repair: bool,
        #[arg(long, help = "Print the commands that would run (cloud terraform)")]
        dry_run: bool,
        #[arg(long, help = "Offline mode: install from cache only, no network (T4.1)")]
        offline: bool,
    },
    #[command(about = "Manage the local store (prune unreferenced packages)")]
    Store {
        #[command(subcommand)]
        cmd: crate::commands::store::StoreCmd,
    },
    #[command(about = "Benchmark install (phases + wall-time, C3)")]
    Bench {
        #[command(flatten)]
        args: crate::commands::bench::BenchArgs,
    },
    #[command(about = "Approve/deny lifecycle scripts per package (T5 trust gate)")]
    Trust {
        #[command(subcommand)]
        cmd: crate::commands::trust::TrustCmd,
    },
    #[command(about = "Run user-defined pre/post scripts (mg.hooks.toml)")]
    Hooks {
        #[command(subcommand)]
        cmd: crate::commands::hooks::HooksCmd,
    },
    #[command(about = "Auto-generate CLI docs from clap schema")]
    Docs {
        #[arg(short, long, help = "write docs to file (default: stdout)")]
        output: Option<std::path::PathBuf>,
    },
    #[command(about = "Telemetry opt-in status/log (default OFF — sends nothing)")]
    Telemetry {
        #[command(subcommand)]
        cmd: crate::commands::telemetry::TelemetryCmd,
    },
    #[command(about = "Network transparency — list EVERY outbound connection + reachability")]
    Network {
        #[command(subcommand)]
        cmd: crate::commands::network::NetworkCmd,
    },
    #[command(about = "Environment diagnostic (toolchain, store, disk, network)")]
    Doctor {
        #[command(subcommand)]
        cmd: crate::commands::doctor::DoctorCmd,
    },
    #[command(about = "Manage kernel templates (publish/fetch — registry-backed)")]
    Template {
        #[command(subcommand)]
        cmd: crate::commands::template::TemplateCmd,
    },
    #[command(about = "Show workspace graph (nodes + workspace:* edges)")]
    Workspace {
        #[command(subcommand)]
        cmd: crate::commands::workspace::WorkspaceCmd,
    },
    #[command(about = "Add dependencies (auto-detect core)")]
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
        #[arg(long, help = "Only update manifest, do not install dependencies")]
        no_install: bool,
    },
    #[command(about = "Remove dependencies (auto-detect core)", alias = "rm")]
    Remove {
        #[arg(required = true)]
        packages: Vec<String>,
        #[arg(long, help = "Only update manifest, do not reinstall dependencies")]
        no_install: bool,
    },
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
    #[cfg_attr(not(feature = "web"), command(hide = true))]
    #[cfg_attr(
        all(
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
        ),
        command(visible_alias = "create")
    )]
    #[command(name = "create-web", about = "Scaffold a new web project", visible_alias = "cre-w")]
    CreateWeb {
        /// Framework with optional version (e.g., react@latest, nextjs@14.0.0)
        #[arg(value_name = "FRAMEWORK[@VERSION]")]
        framework: String,
        /// Project directory name
        #[arg(value_name = "PROJECT")]
        project_name: String,
        #[command(flatten)]
        flags: crate::commands::core::scaffold_flags::ScaffoldFlags,
    },
    #[command(
        name = "create-game",
        about = "Scaffold a new game project",
        visible_alias = "cre-g",
        hide = true
    )]
    CreateGame {
        /// Framework with optional version
        #[arg(value_name = "FRAMEWORK[@VERSION]")]
        framework: String,
        /// Project directory name
        #[arg(value_name = "PROJECT")]
        project_name: String,
    },
    #[command(name = "create-ai", about = "Scaffold a new AI project", visible_alias = "cre-ai", hide = true)]
    CreateAi {
        /// Framework with optional version
        #[arg(value_name = "FRAMEWORK[@VERSION]")]
        framework: String,
        /// Project directory name
        #[arg(value_name = "PROJECT")]
        project_name: String,
    },
    #[command(
        name = "create-clo",
        about = "Scaffold a new cloud project",
        visible_alias = "cre-c",
        hide = true
    )]
    CreateClo {
        /// Framework with optional version
        #[arg(value_name = "FRAMEWORK[@VERSION]")]
        framework: String,
        /// Project directory name
        #[arg(value_name = "PROJECT")]
        project_name: String,
    },
    #[command(
        name = "create-cicd",
        about = "Scaffold a new CI/CD project",
        visible_alias = "cre-ci",
        hide = true
    )]
    CreateCicd {
        /// Framework with optional version
        #[arg(value_name = "FRAMEWORK[@VERSION]")]
        framework: String,
        /// Project directory name
        #[arg(value_name = "PROJECT")]
        project_name: String,
    },
    #[command(name = "create-iot", about = "Scaffold a new IoT project", visible_alias = "cre-i", hide = true)]
    CreateIot {
        /// Framework with optional version
        #[arg(value_name = "FRAMEWORK[@VERSION]")]
        framework: String,
        /// Project directory name
        #[arg(value_name = "PROJECT")]
        project_name: String,
    },
    #[command(name = "create-app", about = "Scaffold a new app project", visible_alias = "cre-a", hide = true)]
    CreateApp {
        /// Framework with optional version
        #[arg(value_name = "FRAMEWORK[@VERSION]")]
        framework: String,
        /// Project directory name
        #[arg(value_name = "PROJECT")]
        project_name: String,
    },
    #[command(
        name = "create-lib",
        about = "Scaffold a new library project",
        visible_alias = "cre-l",
        hide = true
    )]
    CreateLib {
        /// Project directory name
        #[arg(value_name = "PROJECT")]
        project_name: String
    },
    #[command(
        name = "create-hardware",
        about = "Scaffold hardware packages (optimizer/bench)",
        visible_alias = "cre-h",
        hide = true
    )]
    CreateHardware {
        /// Framework with optional version
        #[arg(value_name = "FRAMEWORK[@VERSION]")]
        framework: String,
        /// Project directory name
        #[arg(value_name = "PROJECT")]
        project_name: String,
    },

    // ── Per-core: install-<core> ───────────────────────────────────
    #[cfg_attr(not(feature = "web"), command(hide = true))]
    #[command(
        name = "install-web",
        alias = "i-web",
        about = "Install web dependencies"
    )]
    InstallWeb {
        packages: Vec<String>,
        #[arg(long, help = "Fail if mg.lock is missing or outdated (CI mode)")]
        frozen: bool,
        #[arg(long, help = "Skip running lifecycle scripts")]
        ignore_scripts: bool,
        #[arg(long, help = "Allow dependency lifecycle scripts")]
        allow_scripts: bool,
        #[arg(
            long,
            help = "Prefer reusing installed versions instead of latest (dedupe, opt-in)"
        )]
        prefer_dedupe: bool,
        #[arg(long, help = "Re-link dangling symlinks in node_modules")]
        repair: bool,
        #[arg(long, help = "Use only cached packages, no network requests")]
        offline: bool,
    },
    #[command(
        name = "install-game",
        alias = "i-game",
        about = "Install game dependencies",
        hide = true
    )]
    InstallGame { packages: Vec<String> },
    #[command(
        name = "install-ai",
        alias = "i-ai",
        about = "Install AI dependencies",
        hide = true
    )]
    InstallAi {
        packages: Vec<String>,
        #[arg(long)]
        dry_run: bool,
    },
    #[command(
        name = "install-clo",
        alias = "i-clo",
        about = "Install cloud dependencies",
        hide = true
    )]
    InstallClo { packages: Vec<String> },
    #[command(
        name = "install-cicd",
        alias = "i-cicd",
        about = "Install CI/CD dependencies",
        hide = true
    )]
    InstallCicd { packages: Vec<String> },
    #[command(
        name = "install-iot",
        alias = "i-iot",
        about = "Install IoT dependencies",
        hide = true
    )]
    InstallIot { packages: Vec<String> },
    #[command(
        name = "install-app",
        alias = "i-app",
        about = "Install app dependencies",
        hide = true
    )]
    InstallApp { packages: Vec<String> },
    #[command(
        name = "install-lib",
        alias = "i-lib",
        about = "Install library dependencies",
        hide = true
    )]
    InstallLib { packages: Vec<String> },
    #[command(
        name = "install-hardware",
        alias = "i-hardware",
        about = "Install hardware packages (optimizer/bench)",
        hide = true
    )]
    InstallHardware { packages: Vec<String> },

    // ── Per-core: add-<core> ───────────────────────────────────
    #[cfg_attr(not(feature = "web"), command(hide = true))]
    #[command(name = "add-web", alias = "a-web", about = "Add web dependencies")]
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
        #[arg(long, help = "Only update manifest, do not install dependencies")]
        no_install: bool,
        #[arg(short = 'g', long)]
        global: bool,
    },
    #[command(
        name = "add-game",
        alias = "a-game",
        about = "Add game dependencies",
        hide = true
    )]
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
    #[command(
        name = "add-ai",
        alias = "a-ai",
        about = "Add AI dependencies",
        hide = true
    )]
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
    #[command(
        name = "add-clo",
        alias = "a-clo",
        about = "Add cloud dependencies",
        hide = true
    )]
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
    #[command(
        name = "add-cicd",
        alias = "a-cicd",
        about = "Add CI/CD dependencies",
        hide = true
    )]
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
    #[command(
        name = "add-iot",
        alias = "a-iot",
        about = "Add IoT dependencies",
        hide = true
    )]
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
    #[command(
        name = "add-app",
        alias = "a-app",
        about = "Add app dependencies",
        hide = true
    )]
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
    #[command(
        name = "add-lib",
        alias = "a-lib",
        about = "Add library dependencies",
        hide = true
    )]
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
    #[command(
        name = "add-hardware",
        alias = "a-hardware",
        about = "Add hardware packages (optimizer/bench)",
        hide = true
    )]
    AddHardware {
        #[arg(required = true)]
        packages: Vec<String>,
    },

    // ── Per-core: remove-<core> ────────────────────────────────
    #[cfg_attr(not(feature = "web"), command(hide = true))]
    #[command(
        name = "remove-web",
        alias = "rm-web",
        about = "Remove web dependencies"
    )]
    RemoveWeb {
        #[arg(required = true)]
        packages: Vec<String>,
        #[arg(long, help = "Only update manifest, do not reinstall dependencies")]
        no_install: bool,
    },
    #[command(
        name = "remove-game",
        alias = "rm-game",
        about = "Remove game dependencies",
        hide = true
    )]
    RemoveGame { packages: Vec<String> },
    #[command(
        name = "remove-ai",
        alias = "rm-ai",
        about = "Remove AI dependencies",
        hide = true
    )]
    RemoveAi { packages: Vec<String> },
    #[command(
        name = "remove-clo",
        alias = "rm-clo",
        about = "Remove cloud dependencies",
        hide = true
    )]
    RemoveClo { packages: Vec<String> },
    #[command(
        name = "remove-cicd",
        alias = "rm-cicd",
        about = "Remove CI/CD dependencies",
        hide = true
    )]
    RemoveCicd { packages: Vec<String> },
    #[command(
        name = "remove-iot",
        alias = "rm-iot",
        about = "Remove IoT dependencies",
        hide = true
    )]
    RemoveIot { packages: Vec<String> },
    #[command(
        name = "remove-app",
        alias = "rm-app",
        about = "Remove app dependencies",
        hide = true
    )]
    RemoveApp { packages: Vec<String> },
    #[command(
        name = "remove-lib",
        alias = "rm-lib",
        about = "Remove library dependencies",
        hide = true
    )]
    RemoveLib { packages: Vec<String> },

    // ── Per-core: list-<core> ──────────────────────────────────
    #[cfg_attr(not(feature = "web"), command(hide = true))]
    #[command(name = "list-web", alias = "ls-web", about = "List web packages")]
    ListWeb,
    #[command(
        name = "list-game",
        alias = "ls-game",
        about = "List game packages",
        hide = true
    )]
    ListGame,
    #[command(
        name = "list-ai",
        alias = "ls-ai",
        about = "List AI packages",
        hide = true
    )]
    ListAi,
    #[command(
        name = "list-clo",
        alias = "ls-clo",
        about = "List cloud packages",
        hide = true
    )]
    ListClo,
    #[command(
        name = "list-cicd",
        alias = "ls-cicd",
        about = "List CI/CD packages",
        hide = true
    )]
    ListCicd,
    #[command(
        name = "list-iot",
        alias = "ls-iot",
        about = "List IoT packages",
        hide = true
    )]
    ListIot,
    #[command(
        name = "list-app",
        alias = "ls-app",
        about = "List app packages",
        hide = true
    )]
    ListApp,
    #[command(
        name = "list-lib",
        alias = "ls-lib",
        about = "List library packages",
        hide = true
    )]
    ListLib,
    #[command(
        name = "list-hardware",
        alias = "ls-hardware",
        about = "List hardware packages",
        hide = true
    )]
    ListHardware,

    // ── Per-core: update-<core> ────────────────────────────────
    #[cfg_attr(not(feature = "web"), command(hide = true))]
    #[command(name = "update-web", alias = "up-web", about = "Update web packages")]
    UpdateWeb {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(
        name = "update-game",
        alias = "up-game",
        about = "Update game packages",
        hide = true
    )]
    UpdateGame {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(
        name = "update-ai",
        alias = "up-ai",
        about = "Update AI packages",
        hide = true
    )]
    UpdateAi {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(
        name = "update-clo",
        alias = "up-clo",
        about = "Update cloud packages",
        hide = true
    )]
    UpdateClo {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(
        name = "update-cicd",
        alias = "up-cicd",
        about = "Update CI/CD packages",
        hide = true
    )]
    UpdateCicd {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(
        name = "update-iot",
        alias = "up-iot",
        about = "Update IoT packages",
        hide = true
    )]
    UpdateIot {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(
        name = "update-app",
        alias = "up-app",
        about = "Update app packages",
        hide = true
    )]
    UpdateApp {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(
        name = "update-lib",
        alias = "up-lib",
        about = "Update library packages",
        hide = true
    )]
    UpdateLib {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Cli;
    use clap::{CommandFactory, Parser};

    #[test]
    fn test_config_parses_get_set_delete_local() {
        let get = Cli::try_parse_from(["mg", "config", "get", "registry"]).unwrap();
        match get.command.unwrap() {
            Commands::Config { cmd, local } => match cmd {
                crate::commands::config::ConfigCmd::Get { key } => {
                    assert_eq!(key, "registry");
                    assert!(!local);
                }
                _ => panic!("expected config get"),
            },
            _ => panic!("expected config command"),
        }

        let set = Cli::try_parse_from(["mg", "config", "set", "x", "1", "--local"]).unwrap();
        match set.command.unwrap() {
            Commands::Config { cmd, local } => match cmd {
                crate::commands::config::ConfigCmd::Set { key, value, toml } => {
                    assert_eq!(key, "x");
                    assert_eq!(value, "1");
                    assert!(local);
                    assert!(!toml); // mặc định không phải toml
                }
                _ => panic!("expected config set"),
            },
            _ => panic!("expected config command"),
        }

        // mg config set --toml
        let set_toml =
            Cli::try_parse_from(["mg", "config", "set", "ecosystem", "web", "--toml"]).unwrap();
        match set_toml.command.unwrap() {
            Commands::Config { cmd, .. } => match cmd {
                crate::commands::config::ConfigCmd::Set { key, value, toml } => {
                    assert_eq!(key, "ecosystem");
                    assert_eq!(value, "web");
                    assert!(toml);
                }
                _ => panic!("expected config set --toml"),
            },
            _ => panic!("expected config command"),
        }

        // mg config unset
        let unset = Cli::try_parse_from(["mg", "config", "unset", "registry"]).unwrap();
        match unset.command.unwrap() {
            Commands::Config { cmd, .. } => match cmd {
                crate::commands::config::ConfigCmd::Unset { key, .. } => {
                    assert_eq!(key, "registry");
                }
                _ => panic!("expected config unset"),
            },
            _ => panic!("expected config command"),
        }

        // mg config list --local
        let list = Cli::try_parse_from(["mg", "config", "list", "--local"]).unwrap();
        match list.command.unwrap() {
            Commands::Config { cmd, .. } => match cmd {
                crate::commands::config::ConfigCmd::List { local } => {
                    assert!(local);
                }
                _ => panic!("expected config list"),
            },
            _ => panic!("expected config command"),
        }
    }

    #[test]
    fn test_stage_parses_dir_flag() {
        let cli = Cli::try_parse_from(["mg", "stage", "--dir", "/tmp/demo"]).unwrap();
        match cli.command.unwrap() {
            Commands::Stage { dir } => {
                assert_eq!(dir.as_deref(), Some(std::path::Path::new("/tmp/demo")));
            }
            _ => panic!("expected stage command"),
        }
    }

    #[test]
    fn test_per_core_aliases_resolve() {
        let cmd = Cli::command();
        for (alias, expected) in [
            ("i-web", "install-web"),
            ("a-lib", "add-lib"),
            ("rm-ai", "remove-ai"),
            ("up-clo", "update-clo"),
            ("ls-hardware", "list-hardware"),
            ("i-hardware", "install-hardware"),
        ] {
            let found = cmd
                .find_subcommand(alias)
                .unwrap_or_else(|| panic!("alias {alias} not found"));
            assert_eq!(
                found.get_name(),
                expected,
                "alias {alias} should map to {expected}"
            );
        }
    }

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
    fn test_global_quiet_flag_parses() {
        let cli = Cli::try_parse_from(["mg", "--quiet", "add-web", "zod"]).unwrap();
        assert!(cli.quiet);
        match cli.command.unwrap() {
            Commands::AddWeb { packages, .. } => assert_eq!(packages, vec!["zod"]),
            _ => panic!("expected add-web command"),
        }
    }

    #[test]
    fn test_add_and_remove_accept_no_install() {
        let add = Cli::try_parse_from(["mg", "add-web", "dayjs", "--no-install"]).unwrap();
        match add.command.unwrap() {
            Commands::AddWeb { no_install, .. } => assert!(no_install),
            _ => panic!("expected add-web command"),
        }

        let remove =
            Cli::try_parse_from(["mg", "remove-web", "zod", "lodash", "--no-install"]).unwrap();
        match remove.command.unwrap() {
            Commands::RemoveWeb {
                packages,
                no_install,
            } => {
                assert_eq!(packages, vec!["zod", "lodash"]);
                assert!(no_install);
            }
            _ => panic!("expected remove-web command"),
        }
    }

    #[test]
    fn test_install_accepts_script_policy_flags() {
        let install = Cli::try_parse_from(["mg", "install", "--allow-scripts"]).unwrap();
        match install.command.unwrap() {
            Commands::Install {
                ignore_scripts,
                allow_scripts,
                ..
            } => {
                assert!(!ignore_scripts);
                assert!(allow_scripts);
            }
            _ => panic!("expected install command"),
        }

        let install_web =
            Cli::try_parse_from(["mg", "install-web", "--ignore-scripts", "--allow-scripts"])
                .unwrap();
        match install_web.command.unwrap() {
            Commands::InstallWeb {
                ignore_scripts,
                allow_scripts,
                ..
            } => {
                assert!(ignore_scripts);
                assert!(allow_scripts);
            }
            _ => panic!("expected install-web command"),
        }
    }

    #[test]
    fn test_install_accepts_package_specs() {
        let install = Cli::try_parse_from([
            "mg",
            "install",
            "react@latest",
            "zod@^3.22.4",
            "--allow-scripts",
        ])
        .unwrap();

        match install.command.unwrap() {
            Commands::Install {
                packages,
                allow_scripts,
                ..
            } => {
                assert_eq!(packages, vec!["react@latest", "zod@^3.22.4"]);
                assert!(allow_scripts);
            }
            _ => panic!("expected install command"),
        }
    }

    #[test]
    fn test_cache_command_accepts_status_and_clean_targets() {
        let status = Cli::try_parse_from(["mg", "cache", "status", "--target", "shared"]).unwrap();
        match status.command.unwrap() {
            Commands::Cache {
                action,
                target,
                yes,
                ..
            } => {
                assert_eq!(action, "status");
                assert_eq!(target, "shared");
                assert!(!yes);
            }
            _ => panic!("expected cache command"),
        }

        let clean =
            Cli::try_parse_from(["mg", "cache", "clean", "--target", "build", "--yes"]).unwrap();
        match clean.command.unwrap() {
            Commands::Cache {
                action,
                target,
                yes,
                ..
            } => {
                assert_eq!(action, "clean");
                assert_eq!(target, "build");
                assert!(yes);
            }
            _ => panic!("expected cache command"),
        }

        let prune =
            Cli::try_parse_from(["mg", "cache", "prune", "--target", "shared", "--yes"]).unwrap();
        match prune.command.unwrap() {
            Commands::Cache {
                action,
                target,
                yes,
                dry_run,
                ..
            } => {
                assert_eq!(action, "prune");
                assert_eq!(target, "shared");
                assert!(yes);
                assert!(!dry_run);
            }
            _ => panic!("expected cache command"),
        }

        let dry_run =
            Cli::try_parse_from(["mg", "cache", "prune", "--target", "shared", "--dry-run"])
                .unwrap();
        match dry_run.command.unwrap() {
            Commands::Cache {
                action,
                target,
                yes,
                dry_run,
            } => {
                assert_eq!(action, "prune");
                assert_eq!(target, "shared");
                assert!(!yes);
                assert!(dry_run);
            }
            _ => panic!("expected cache command"),
        }
    }

    #[test]
    fn test_available_cores_matches_build_shape() {
        let available = crate::factory::available_cores();

        #[cfg(all(feature = "web", not(feature = "lib")))]
        assert_eq!(available, vec![("web", "🌐  Web application")]);

        #[cfg(all(feature = "web", feature = "lib", not(feature = "game")))]
        assert_eq!(
            available,
            vec![
                ("web", "🌐  Web application"),
                ("lib", "📚  Library (ts / rust / python)")
            ]
        );

        #[cfg(all(
            feature = "web",
            feature = "lib",
            feature = "game",
            not(feature = "iot")
        ))]
        assert_eq!(
            available,
            vec![
                ("web", "🌐  Web application"),
                ("lib", "📚  Library (ts / rust / python)"),
                ("game", "🎮  Game (bevy / godot / unity / unreal)")
            ]
        );

        #[cfg(all(
            feature = "web",
            feature = "lib",
            feature = "game",
            feature = "iot",
            not(feature = "hardware")
        ))]
        assert_eq!(
            available,
            vec![
                ("web", "🌐  Web application"),
                ("lib", "📚  Library (ts / rust / python)"),
                ("game", "🎮  Game (bevy / godot / unity / unreal)"),
                ("iot", "📡  IoT (esp32-rust / platformio / zephyr)")
            ]
        );

        #[cfg(all(
            feature = "web",
            feature = "lib",
            feature = "game",
            feature = "iot",
            feature = "hardware"
        ))]
        assert_eq!(
            available,
            vec![
                ("web", "🌐  Web application"),
                ("lib", "📚  Library (ts / rust / python)"),
                ("game", "🎮  Game (bevy / godot / unity / unreal)"),
                ("iot", "📡  IoT (esp32-rust / platformio / zephyr)"),
                (
                    "hardware",
                    "⚙️  Hardware (optimizer/bench — GPU/CPU acceleration)"
                )
            ]
        );

        #[cfg(not(feature = "web"))]
        assert!(available.is_empty());
    }

    #[test]
    fn test_help_surface_matches_build_shape() {
        let help = Cli::command().render_long_help().to_string();

        assert!(help.contains("dev"));
        #[cfg(all(feature = "web", not(feature = "all")))]
        {
            assert!(help.contains("create"));
            assert!(help.contains("create-web"));
        }
        #[cfg(any(not(feature = "web"), feature = "all"))]
        {
            assert!(help.contains("install-web"));
            assert!(help.contains("create-web"));
            assert!(help.contains("add-web"));
        }

        assert!(!help.contains("create-game"));
        assert!(!help.contains("add-game"));
        assert!(!help.contains("install-game"));
        assert!(!help.contains("create-ai"));
        assert!(!help.contains("install-ai"));
        assert!(!help.contains("add-ai"));
        assert!(!help.contains("create-clo"));
        assert!(!help.contains("install-clo"));
        assert!(!help.contains("create-cicd"));
        assert!(!help.contains("install-cicd"));
        assert!(!help.contains("create-iot"));
        assert!(!help.contains("install-iot"));
        assert!(!help.contains("create-app"));
        assert!(!help.contains("install-app"));
        assert!(!help.contains("create-lib"));
        assert!(!help.contains("install-lib"));

        #[cfg(any(not(feature = "web"), feature = "all"))]
        assert!(!help.contains("create   "));
    }

    #[test]
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
    fn test_single_core_create_alias_parses() {
        let cli =
            Cli::try_parse_from(["mg", "create", "react@latest", "demo-app", "--ts"]).unwrap();

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
            _ => panic!("expected create-web command through single-core alias"),
        }
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

    #[test]
    fn test_deploy_defaults_to_dry_run() {
        let cli = Cli::try_parse_from(["mg", "deploy"]).unwrap();
        match cli.command.unwrap() {
            Commands::Deploy { run } => assert!(!run),
            _ => panic!("expected deploy command"),
        }
        let cli = Cli::try_parse_from(["mg", "deploy", "--run"]).unwrap();
        match cli.command.unwrap() {
            Commands::Deploy { run } => assert!(run),
            _ => panic!("expected deploy command"),
        }
    }

    #[test]
    fn test_install_parses_dry_run_flag() {
        let cli = Cli::try_parse_from(["mg", "install", "--dry-run"]).unwrap();
        match cli.command.unwrap() {
            Commands::Install { dry_run, .. } => assert!(dry_run),
            _ => panic!("expected install command"),
        }
    }

    #[test]
    fn test_workspace_list_parses_filter_and_json() {
        let cli =
            Cli::try_parse_from(["mg", "workspace", "list", "--filter", "./apps/*", "--json"])
                .unwrap();

        match cli.command.unwrap() {
            Commands::Workspace { cmd } => match cmd {
                crate::commands::workspace::WorkspaceCmd::List { filter, json } => {
                    assert_eq!(filter.as_deref(), Some("./apps/*"));
                    assert!(json);
                }
            },
            _ => panic!("expected workspace command"),
        }
    }
}
