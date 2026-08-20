use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::Cli;
use crate::Commands;

use super::types::DispatchCommand;

pub async fn run(cli: Cli) -> Result<()> {
    mg_ui::set_quiet(cli.quiet);
    let core = cli.core.as_deref();

    if let Some(dir) = cli.dir.as_deref() {
        std::env::set_current_dir(dir)
            .map_err(|e| crate::error::dir_missing(&dir.display().to_string(), e.to_string()))?;
    }

    if cli.recursive {
        if let Some(command) = cli.command.as_ref() {
            if !recursive_supported(command) {
                reject_unsupported_recursive(Some(command))?;
            }
            // Publish recursive là pipeline topo riêng — --filter chưa nối vào đó.
            if cli.filter.is_some() && matches!(command, Commands::Publish { .. }) {
                reject_unsupported_filter(command)?;
            }
        }
    } else if cli.filter.is_some() {
        reject_filter_without_recursive()?;
    }

    if cli.audit_strict {
        if let Some(command) = cli.command.as_ref() {
            reject_unsupported_audit_strict(command)?;
        }
        std::env::set_var("MG_AUDIT_STRICT", "1");
    }

    match cli.command {
        // Publish recursive là pipeline riêng (topo sort) — giữ path cũ,
        // chỉ bật khi user truyền --recursive.
        Some(command @ Commands::Publish { .. }) => {
            dispatch_command(command, core, cli.recursive).await
        }
        Some(command) if cli.recursive => run_recursive(command, core, cli.filter.as_deref()).await,
        Some(command) => dispatch_command(command, core, false).await,
        None => {
            let cores = crate::factory::available_cores();
            mg_ui::help::print_custom_help(&cores);
            Ok(())
        }
    }
}

/// Các lệnh workspace-aware khi chạy `--recursive` (pnpm -r parity).
/// Chạy tuần tự từng workspace, lỗi 1 project không chặn repo (báo tổng cuối).
async fn run_recursive(command: Commands, core: Option<&str>, filter: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let project_root = mg_config::project::ProjectConfig::find_project_root(&cwd)
        .ok_or_else(crate::error::project_root_missing)?;
    let mut workspaces = crate::commands::install::discover_workspace_projects(&project_root)?
        .ok_or_else(|| {
            anyhow::anyhow!("--recursive requires megagate.workspace.toml (mode = \"monorepo\")")
        })?;

    if let Some(pattern) = filter {
        // Reuse mg-workspace filter matcher (không import PM khác): tên package,
        // path tương đối (`./apps/*`), scope (`@core/*`).
        let mut selected: Vec<PathBuf> = Vec::new();
        for ws in &workspaces {
            let relative = ws.strip_prefix(&project_root).unwrap_or(ws);
            let name = crate::commands::install::workspace_package_name(ws).unwrap_or_else(|| {
                ws.file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
            if mg_workspace::filter_matches(pattern, relative, &name) {
                selected.push(ws.clone());
            }
        }
        if selected.is_empty() {
            mg_ui::info(&format!(
                "--filter '{pattern}' matched no workspace packages."
            ));
            return Ok(());
        }
        workspaces = selected;
        mg_ui::info(&format!(
            "--filter '{pattern}' → {} workspace(s)",
            workspaces.len()
        ));
    }

    if workspaces.is_empty() {
        bail!("No workspace projects found in this monorepo.");
    }

    // Xây dựng đồ thị phụ thuộc workspace và sắp xếp topo (level-by-level)
    if let Ok(graph) = mg_workspace::build_workspace_graph(&workspaces) {
        if let Ok(levels) = mg_workspace::topo_levels(&graph) {
            let mut ordered = Vec::new();
            for level in levels {
                for idx in level {
                    let node_path = &graph.nodes[idx].path;
                    if let Some(pos) = workspaces.iter().position(|w| w == node_path) {
                        ordered.push(workspaces.remove(pos));
                    }
                }
            }
            // Thêm các workspace còn lại (nếu có)
            ordered.append(&mut workspaces);
            workspaces = ordered;
        }
    }

    let name = command_name(&command);
    let mut failed = 0usize;
    let original_cwd = cwd;
    let is_build_cmd = matches!(command, Commands::Build { .. });
    let mut ws_composite_hashes: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();

    for ws in &workspaces {
        // T4 core-aware: nếu --core flag không có, đọc .mg.core marker của workspace này.
        // Ưu tiên: CLI --core > marker file > None.
        let ws_core: Option<String> = if core.is_some() {
            core.map(|s| s.to_string())
        } else {
            // Đọc marker .mg.core trong workspace folder
            read_core_marker(ws)
        };
        let ws_core_str = ws_core.as_deref();

        let ws_name = crate::commands::install::workspace_package_name(ws).unwrap_or_else(|| {
            ws.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        });

        if is_build_cmd {
            if let Ok((should_rebuild, _src_hash, comp_hash)) =
                mg_workspace::check_package_build_freshness(ws, &ws_composite_hashes)
            {
                if !should_rebuild {
                    mg_ui::info(&format!(
                        "⚡ [cached] {} (core: {}) — source & deps unchanged",
                        ws.display(),
                        ws_core_str.unwrap_or("auto")
                    ));
                    ws_composite_hashes.insert(ws_name.clone(), comp_hash);
                    continue;
                }
            }
        }

        mg_ui::info(&format!(
            "== {name} → {} (core: {})",
            ws.display(),
            ws_core_str.unwrap_or("auto")
        ));
        let result = async {
            std::env::set_current_dir(ws)?;
            dispatch_command(command.clone(), ws_core_str, false).await
        }
        .await;
        if let Err(e) = result {
            failed += 1;
            mg_ui::error(&format!("{name} failed in '{}': {e:#}", ws.display()));
        } else if is_build_cmd {
            if let Ok(src_hash) = mg_workspace::compute_package_source_hash(ws) {
                if let Ok(cache) = mg_workspace::save_package_build_cache(
                    ws,
                    src_hash,
                    ws_composite_hashes.clone(),
                ) {
                    ws_composite_hashes.insert(ws_name, cache.composite_hash);
                }
            }
        }
    }
    std::env::set_current_dir(&original_cwd)?;

    if failed > 0 {
        return Err(crate::error::workspace_failed(failed));
    }
    Ok(())
}

/// Đọc nội dung `.mg.core` marker file trong thư mục `dir` — trả về tên core nếu hợp lệ.
/// Format: 1 dòng plain text = tên core (có thể có comment `# ...` sau tên).
fn read_core_marker(dir: &Path) -> Option<String> {
    let marker = dir.join(mg_config::project::ProjectConfig::CORE_MARKER_FILE);
    let content = std::fs::read_to_string(marker).ok()?;
    // Lấy dòng đầu, bỏ comment
    let first_line = content.lines().next()?.trim();
    let core_name = first_line.split('#').next()?.trim();
    if core_name.is_empty() {
        None
    } else {
        Some(core_name.to_string())
    }
}

fn recursive_supported(command: &Commands) -> bool {
    // T4: mở rộng danh sách lệnh hỗ trợ --recursive (pnpm -r parity)
    // Thêm: Build, Run, Audit, Outdated, Dev (core-aware per workspace)
    matches!(
        command,
        Commands::Publish { .. }
            | Commands::Install { .. }
            | Commands::Add { .. }
            | Commands::Remove { .. }
            | Commands::Update { .. }
            | Commands::List
            | Commands::Build { .. }
            | Commands::Run { .. }
            | Commands::Audit { .. }
            | Commands::Outdated { .. }
            | Commands::Dev { .. }
            | Commands::InstallWeb { .. }
            | Commands::InstallGame { .. }
            | Commands::InstallAi { .. }
            | Commands::InstallClo { .. }
            | Commands::InstallCicd { .. }
            | Commands::InstallIot { .. }
            | Commands::InstallApp { .. }
            | Commands::InstallLib { .. }
            | Commands::InstallHardware { .. }
            | Commands::AddWeb { .. }
            | Commands::AddGame { .. }
            | Commands::AddAi { .. }
            | Commands::AddClo { .. }
            | Commands::AddCicd { .. }
            | Commands::AddIot { .. }
            | Commands::AddApp { .. }
            | Commands::AddLib { .. }
            | Commands::AddHardware { .. }
            | Commands::RemoveWeb { .. }
            | Commands::RemoveGame { .. }
            | Commands::RemoveAi { .. }
            | Commands::RemoveClo { .. }
            | Commands::RemoveCicd { .. }
            | Commands::RemoveIot { .. }
            | Commands::RemoveApp { .. }
            | Commands::RemoveLib { .. }
            | Commands::UpdateWeb { .. }
            | Commands::UpdateGame { .. }
            | Commands::UpdateAi { .. }
            | Commands::UpdateClo { .. }
            | Commands::UpdateCicd { .. }
            | Commands::UpdateIot { .. }
            | Commands::UpdateApp { .. }
            | Commands::UpdateLib { .. }
            | Commands::ListWeb
            | Commands::ListGame
            | Commands::ListAi
            | Commands::ListClo
            | Commands::ListCicd
            | Commands::ListIot
            | Commands::ListApp
            | Commands::ListLib
            | Commands::ListHardware
    )
}

fn reject_unsupported_recursive(command: Option<&Commands>) -> Result<()> {
    let Some(command) = command else {
        bail!(
            "--recursive is declared on the CLI surface but workspace recursion is not implemented yet. Refusing to pretend it ran."
        );
    };

    bail!(
        "--recursive is not implemented for '{}' yet. Beta safety rule: refusing a silent no-op on workspace commands.",
        command_name(command)
    )
}

fn reject_unsupported_filter(command: &Commands) -> Result<()> {
    bail!(
        "--filter is not wired into '{}' yet (publish recursive pipeline chưa nhận filter). Refusing a silent no-op.",
        command_name(command)
    )
}

fn reject_filter_without_recursive() -> Result<()> {
    bail!("--filter requires --recursive (it filters workspace targets). Use `mg <cmd> --recursive --filter <glob>`.")
}

fn reject_unsupported_audit_strict(command: &Commands) -> Result<()> {
    let _ = command;
    Ok(())
}

fn command_name(command: &Commands) -> &'static str {
    match command {
        Commands::Init { .. } => "init",
        Commands::Info { .. } => "info",
        Commands::Search { .. } => "search",
        Commands::Outdated { .. } => "outdated",
        Commands::Audit { .. } => "audit",
        Commands::SelfUpdate => "self-update",
        Commands::Config { .. } => "config",
        Commands::Stage { .. } => "stage",
        Commands::Publish { .. } => "publish",
        Commands::Patch { .. } => "patch",
        Commands::Dedupe { .. } => "dedupe",
        Commands::Template { .. } => "template",
        Commands::Workspace { .. } => "workspace",
        Commands::Store { .. } => "store",
        Commands::Bench { .. } => "bench",
        Commands::Network { .. } => "network",
        Commands::Doctor { .. } => "doctor",
        Commands::Trust { .. } => "trust",
        Commands::Hooks { .. } => "hooks",
        Commands::Docs { .. } => "docs",
        Commands::Telemetry { .. } => "telemetry",
        Commands::Sbom { .. } => "sbom",
        Commands::Login { .. } => "login",
        Commands::Registry { .. } => "registry",
        Commands::Model { .. } => "model",
        Commands::Mcp => "mcp",
        Commands::Dev { .. } => "dev",
        Commands::Run { .. } => "run",
        Commands::Build { .. } => "build",
        Commands::Flash { .. } => "flash",
        Commands::Deploy { .. } => "deploy",
        Commands::CiGenerate => "ci-generate",
        Commands::Verify => "verify",
        Commands::Start => "start",
        Commands::Exec { .. } => "exec",
        Commands::Dlx { .. } => "dlx",
        Commands::Cache { .. } => "cache",
        Commands::Install { .. } => "install",
        Commands::Add { .. } => "add",
        Commands::Remove { .. } => "remove",
        Commands::Update { .. } => "update",
        Commands::List => "list",
        Commands::Link { .. } => "link",
        Commands::Unlink { .. } => "unlink",
        Commands::Why { .. } => "why",
        Commands::CreateWeb { .. } => "create-web",
        Commands::CreateGame { .. } => "create-game",
        Commands::CreateAi { .. } => "create-ai",
        Commands::CreateClo { .. } => "create-clo",
        Commands::CreateCicd { .. } => "create-cicd",
        Commands::CreateIot { .. } => "create-iot",
        Commands::CreateApp { .. } => "create-app",
        Commands::CreateLib { .. } => "create-lib",
        Commands::CreateHardware { .. } => "create-hardware",
        Commands::InstallWeb { .. } => "install-web",
        Commands::InstallGame { .. } => "install-game",
        Commands::InstallAi { .. } => "install-ai",
        Commands::InstallClo { .. } => "install-clo",
        Commands::InstallCicd { .. } => "install-cicd",
        Commands::InstallIot { .. } => "install-iot",
        Commands::InstallApp { .. } => "install-app",
        Commands::InstallLib { .. } => "install-lib",
        Commands::InstallHardware { .. } => "install-hardware",
        Commands::AddWeb { .. } => "add-web",
        Commands::AddGame { .. } => "add-game",
        Commands::AddAi { .. } => "add-ai",
        Commands::AddClo { .. } => "add-clo",
        Commands::AddCicd { .. } => "add-cicd",
        Commands::AddIot { .. } => "add-iot",
        Commands::AddApp { .. } => "add-app",
        Commands::AddLib { .. } => "add-lib",
        Commands::AddHardware { .. } => "add-hardware",
        Commands::RemoveWeb { .. } => "remove-web",
        Commands::RemoveGame { .. } => "remove-game",
        Commands::RemoveAi { .. } => "remove-ai",
        Commands::RemoveClo { .. } => "remove-clo",
        Commands::RemoveCicd { .. } => "remove-cicd",
        Commands::RemoveIot { .. } => "remove-iot",
        Commands::RemoveApp { .. } => "remove-app",
        Commands::RemoveLib { .. } => "remove-lib",
        Commands::ListWeb => "list-web",
        Commands::ListGame => "list-game",
        Commands::ListAi => "list-ai",
        Commands::ListClo => "list-clo",
        Commands::ListCicd => "list-cicd",
        Commands::ListIot => "list-iot",
        Commands::ListApp => "list-app",
        Commands::ListLib => "list-lib",
        Commands::ListHardware => "list-hardware",
        Commands::UpdateWeb { .. } => "update-web",
        Commands::UpdateGame { .. } => "update-game",
        Commands::UpdateAi { .. } => "update-ai",
        Commands::UpdateClo { .. } => "update-clo",
        Commands::UpdateCicd { .. } => "update-cicd",
        Commands::UpdateIot { .. } => "update-iot",
        Commands::UpdateApp { .. } => "update-app",
        Commands::UpdateLib { .. } => "update-lib",
        Commands::Import { .. } => "import",
    }
}

async fn dispatch_command(command: Commands, core: Option<&str>, recursive: bool) -> Result<()> {
    match crate::dispatch::per_core::command_to_dispatch(command, core)? {
        DispatchCommand::Common(cmd) => super::common::dispatch_common(cmd, core, recursive).await,
        DispatchCommand::Core(cmd) => super::core::dispatch_core(cmd).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audit_strict_rejects_materializing_install_commands() {
        let install = Commands::Install {
            packages: vec![],
            frozen: false,
            ignore_scripts: false,
            allow_scripts: false,
            prefer_dedupe: false,
            repair: false,
            dry_run: false,
        };
        assert!(reject_unsupported_audit_strict(&install).is_ok());

        let add = Commands::AddWeb {
            packages: vec!["zod".into()],
            dev: false,
            exact: false,
            optional: false,
            peer: false,
            no_save: false,
            no_install: false,
            global: false,
        };
        assert!(reject_unsupported_audit_strict(&add).is_ok());
    }

    #[test]
    fn audit_strict_allows_audit_and_manifest_only_mutation() {
        assert!(reject_unsupported_audit_strict(&Commands::Audit { fix: false }).is_ok());

        let add = Commands::AddWeb {
            packages: vec!["zod".into()],
            dev: false,
            exact: false,
            optional: false,
            peer: false,
            no_save: false,
            no_install: true,
            global: false,
        };
        assert!(reject_unsupported_audit_strict(&add).is_ok());
    }

    #[test]
    fn recursive_is_rejected_for_unsupported_commands() {
        // Init không có trong recursive_supported → phải bị reject.
        let err = reject_unsupported_recursive(Some(&Commands::Init {
            template: None,
            signature: None,
        }))
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("--recursive is not implemented for 'init' yet"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn recursive_supported_includes_build_run_audit_outdated_dev() {
        // T4: xác nhận các lệnh mới được mở rộng recursive support
        assert!(recursive_supported(&Commands::Build { target: None }));
        assert!(recursive_supported(&Commands::Audit { fix: false }));
        assert!(recursive_supported(&Commands::Outdated { json: false }));
        assert!(recursive_supported(&Commands::Dev {
            host: None,
            port: None,
            clear: false,
        }));
    }

    #[test]
    fn recursive_supported_includes_install_and_add() {
        // Existing commands vẫn supported sau T4
        assert!(recursive_supported(&Commands::Install {
            packages: vec![],
            frozen: false,
            ignore_scripts: false,
            allow_scripts: false,
            prefer_dedupe: false,
            repair: false,
            dry_run: false,
        }));
        assert!(recursive_supported(&Commands::List));
    }

    #[test]
    fn read_core_marker_parses_plain_and_comment() {
        // T4: đọc .mg.core marker với/không có comment
        let dir = std::env::temp_dir().join(format!("mg_test_marker_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // Plain value
        std::fs::write(dir.join(".mg.core"), "web\n").unwrap();
        assert_eq!(read_core_marker(&dir), Some("web".to_string()));

        // With comment
        std::fs::write(dir.join(".mg.core"), "ai # generated by mg init\n").unwrap();
        assert_eq!(read_core_marker(&dir), Some("ai".to_string()));

        // Empty → None
        std::fs::write(dir.join(".mg.core"), "# just a comment\n").unwrap();
        assert_eq!(read_core_marker(&dir), None);

        // Missing file → None
        std::fs::remove_file(dir.join(".mg.core")).unwrap();
        assert_eq!(read_core_marker(&dir), None);

        std::fs::remove_dir(&dir).ok();
    }
}
