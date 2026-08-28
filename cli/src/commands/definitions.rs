//! Clap command definitions — tách từ main.rs (Phase 7 v5 — user chốt 2026-08-19).
//! main.rs chỉ parse + dispatch; enum ~80 lệnh ở đây (định nghĩa lệnh, không logic).

use clap::Subcommand;

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
            help = "Write core signature marker (.mgc.core) for the current project, no wizard"
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
    #[command(about = "Update MagiCore CLI to the latest version")]
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
        about = "Import legacy lockfile (package-lock.json, pnpm-lock.yaml, yarn.lock, bun.lock) to mgc.lock"
    )]
    Import {
        #[arg(long, help = "Target project directory to import")]
        dir: Option<std::path::PathBuf>,
    },

    // ── W6: SBOM Export ──────────────────────────────────────────────
    #[command(about = "Generate Software Bill of Materials (SBOM) from lockfile")]
    Sbom {
        #[arg(
            long,
            help = "Output format (cyclonedx-json, cyclonedx-xml, spdx-json)"
        )]
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
        #[arg(long, help = "override token (env MGC_NPM_TOKEN recommended)")]
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
    #[command(about = "Inspect or clean MagiCore caches")]
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
        #[arg(long, help = "Fail if mgc.lock is missing or outdated (CI mode)")]
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
        #[arg(
            long,
            help = "Offline mode: install from cache only, no network (T4.1)"
        )]
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
    #[command(about = "Run user-defined pre/post scripts (mgc.hooks.toml)")]
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
    #[command(
        name = "create-web",
        about = "Scaffold a new web project",
        visible_alias = "cre-w"
    )]
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
        visible_alias = "cre-g"
    )]
    CreateGame {
        /// Framework with optional version
        #[arg(value_name = "FRAMEWORK[@VERSION]")]
        framework: String,
        /// Project directory name
        #[arg(value_name = "PROJECT")]
        project_name: String,
    },
    #[command(
        name = "create-ai",
        about = "Scaffold a new AI project",
        visible_alias = "cre-ai"
    )]
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
        visible_alias = "cre-c"
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
        visible_alias = "cre-ci"
    )]
    CreateCicd {
        /// Framework with optional version
        #[arg(value_name = "FRAMEWORK[@VERSION]")]
        framework: String,
        /// Project directory name
        #[arg(value_name = "PROJECT")]
        project_name: String,
    },
    #[command(
        name = "create-iot",
        about = "Scaffold a new IoT project",
        visible_alias = "cre-i"
    )]
    CreateIot {
        /// Framework with optional version
        #[arg(value_name = "FRAMEWORK[@VERSION]")]
        framework: String,
        /// Project directory name
        #[arg(value_name = "PROJECT")]
        project_name: String,
    },
    #[command(
        name = "create-app",
        about = "Scaffold a new app project",
        visible_alias = "cre-a"
    )]
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
        visible_alias = "cre-l"
    )]
    CreateLib {
        /// Project directory name
        #[arg(value_name = "PROJECT")]
        project_name: String,
    },
    #[command(
        name = "create-hardware",
        about = "Scaffold hardware packages (optimizer/bench)",
        visible_alias = "cre-h"
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
        #[arg(long, help = "Fail if mgc.lock is missing or outdated (CI mode)")]
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
        about = "Install game dependencies"
    )]
    InstallGame { packages: Vec<String> },
    #[command(name = "install-ai", alias = "i-ai", about = "Install AI dependencies")]
    InstallAi {
        packages: Vec<String>,
        #[arg(long)]
        dry_run: bool,
    },
    #[command(
        name = "install-clo",
        alias = "i-clo",
        about = "Install cloud dependencies"
    )]
    InstallClo { packages: Vec<String> },
    #[command(
        name = "install-cicd",
        alias = "i-cicd",
        about = "Install CI/CD dependencies"
    )]
    InstallCicd { packages: Vec<String> },
    #[command(
        name = "install-iot",
        alias = "i-iot",
        about = "Install IoT dependencies"
    )]
    InstallIot { packages: Vec<String> },
    #[command(
        name = "install-app",
        alias = "i-app",
        about = "Install app dependencies"
    )]
    InstallApp { packages: Vec<String> },
    #[command(
        name = "install-lib",
        alias = "i-lib",
        about = "Install library dependencies"
    )]
    InstallLib { packages: Vec<String> },
    #[command(
        name = "install-hardware",
        alias = "i-hardware",
        about = "Install hardware packages (optimizer/bench)"
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
    #[command(name = "add-game", alias = "a-game", about = "Add game dependencies")]
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
    #[command(name = "add-ai", alias = "a-ai", about = "Add AI dependencies")]
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
    #[command(name = "add-clo", alias = "a-clo", about = "Add cloud dependencies")]
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
    #[command(name = "add-cicd", alias = "a-cicd", about = "Add CI/CD dependencies")]
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
    #[command(name = "add-iot", alias = "a-iot", about = "Add IoT dependencies")]
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
    #[command(name = "add-app", alias = "a-app", about = "Add app dependencies")]
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
    #[command(name = "add-lib", alias = "a-lib", about = "Add library dependencies")]
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
        about = "Add hardware packages (optimizer/bench)"
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
        about = "Remove game dependencies"
    )]
    RemoveGame { packages: Vec<String> },
    #[command(name = "remove-ai", alias = "rm-ai", about = "Remove AI dependencies")]
    RemoveAi { packages: Vec<String> },
    #[command(
        name = "remove-clo",
        alias = "rm-clo",
        about = "Remove cloud dependencies"
    )]
    RemoveClo { packages: Vec<String> },
    #[command(
        name = "remove-cicd",
        alias = "rm-cicd",
        about = "Remove CI/CD dependencies"
    )]
    RemoveCicd { packages: Vec<String> },
    #[command(
        name = "remove-iot",
        alias = "rm-iot",
        about = "Remove IoT dependencies"
    )]
    RemoveIot { packages: Vec<String> },
    #[command(
        name = "remove-app",
        alias = "rm-app",
        about = "Remove app dependencies"
    )]
    RemoveApp { packages: Vec<String> },
    #[command(
        name = "remove-lib",
        alias = "rm-lib",
        about = "Remove library dependencies"
    )]
    RemoveLib { packages: Vec<String> },

    // ── Per-core: list-<core> ──────────────────────────────────
    #[cfg_attr(not(feature = "web"), command(hide = true))]
    #[command(name = "list-web", alias = "ls-web", about = "List web packages")]
    ListWeb,
    #[command(name = "list-game", alias = "ls-game", about = "List game packages")]
    ListGame,
    #[command(name = "list-ai", alias = "ls-ai", about = "List AI packages")]
    ListAi,
    #[command(name = "list-clo", alias = "ls-clo", about = "List cloud packages")]
    ListClo,
    #[command(name = "list-cicd", alias = "ls-cicd", about = "List CI/CD packages")]
    ListCicd,
    #[command(name = "list-iot", alias = "ls-iot", about = "List IoT packages")]
    ListIot,
    #[command(name = "list-app", alias = "ls-app", about = "List app packages")]
    ListApp,
    #[command(name = "list-lib", alias = "ls-lib", about = "List library packages")]
    ListLib,
    #[command(
        name = "list-hardware",
        alias = "ls-hardware",
        about = "List hardware packages"
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
        about = "Update game packages"
    )]
    UpdateGame {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(name = "update-ai", alias = "up-ai", about = "Update AI packages")]
    UpdateAi {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(name = "update-clo", alias = "up-clo", about = "Update cloud packages")]
    UpdateClo {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(
        name = "update-cicd",
        alias = "up-cicd",
        about = "Update CI/CD packages"
    )]
    UpdateCicd {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(name = "update-iot", alias = "up-iot", about = "Update IoT packages")]
    UpdateIot {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(name = "update-app", alias = "up-app", about = "Update app packages")]
    UpdateApp {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
    #[command(
        name = "update-lib",
        alias = "up-lib",
        about = "Update library packages"
    )]
    UpdateLib {
        packages: Vec<String>,
        #[arg(long, help = "Install updated packages immediately")]
        install: bool,
    },
}


#[cfg(test)]
#[path = "../test/definitions_test.rs"]
mod tests;
