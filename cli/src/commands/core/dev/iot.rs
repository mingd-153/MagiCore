//! iot tooling lệnh: `mgc flash` (Q16 — esp32-rust P1, platformio/zephyr P2).

use anyhow::{Result, anyhow, bail};
use mgc_ui::info;
use std::path::{Path, PathBuf};

fn project_root() -> Result<PathBuf> {
    crate::commands::core::shared::core_project_root("iot")
}

/// Build + flash firmware esp32 (Q16). Board đọc từ arg hoặc mgc.toml `[iot] board`;
/// fail-closed nếu không xác định target (00-index §3, 04 §3).
pub async fn flash(board_override: Option<&str>, skip_build: bool) -> Result<()> {
    let root = project_root()?;
    let adapter = mgc_iot_adapter::adapter_for(&root)
        .ok_or_else(|| crate::error::no_framework_detected("IoT", &root))?;

    if adapter.framework() != "esp32-rust" {
        return Err(crate::error::flash_framework_unsupported(
            adapter.framework(),
        ));
    }

    let board = board_override
        .map(str::to_string)
        .or_else(|| adapter.board(&root))
        .ok_or_else(|| {
            let boards = mgc_iot_adapter::boards_for_framework(adapter.framework())
                .iter()
                .map(|board| board.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            crate::error::no_board_specified(&boards)
        })?;

    let explicit_target = adapter.target(&root);
    let target = resolve_target(adapter.framework(), &board, explicit_target.as_deref())?;

    info(&format!(
        "Board: {board} ({}), target: {target}",
        chip(&board)
    ));

    if !skip_build {
        info("Building firmware (cargo build --release)...");
        let build_args = [
            "build",
            "--release",
            "--locked",
            "--offline",
            "--target",
            &target,
        ];
        run_tool(&root, "cargo", &build_args)?;
    }

    let elf = find_elf(&root, &target)?;
    let elf_str = elf
        .to_str()
        .ok_or_else(|| crate::error::fw_path_not_utf8(&elf))?;
    info(&format!("Flashing firmware: espflash flash {elf_str}"));
    let flash_args = ["flash", elf_str];
    run_tool(&root, "espflash", &flash_args).map_err(|e| crate::error::espflash_failed(&e))
}

/// Validate board ownership before honoring an explicit target override.
/// Xác thực board trước khi nhận target override tường minh.
fn resolve_target(framework: &str, board: &str, target_override: Option<&str>) -> Result<String> {
    let registered =
        mgc_iot_adapter::board_target_for_framework(framework, board).ok_or_else(|| {
            let boards = mgc_iot_adapter::boards_for_framework(framework)
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            crate::error::unsupported_board(board, &boards)
        })?;
    Ok(target_override.unwrap_or(&registered).to_string())
}

/// Board id → chip (tra registry KNOWN_BOARDS).
fn chip(board: &str) -> String {
    mgc_iot_adapter::boards_for_framework("esp32-rust")
        .iter()
        .find(|entry| entry.id == board)
        .map(|entry| entry.chip.clone())
        .unwrap_or_else(|| board.to_string())
}

/// Tìm binary ELF đã build — ưu tiên target/<triple>/release/<name>.elf, fallback scan.
fn find_elf(root: &Path, target: &str) -> Result<PathBuf> {
    if !root.join("Cargo.toml").exists() {
        bail!("No Cargo.toml found — esp32-rust project missing");
    }
    let name: String = std::fs::read_to_string(root.join("Cargo.toml"))
        .ok()
        .and_then(|s| toml::from_str::<toml::Value>(&s).ok())
        .and_then(|v| {
            v.get("package")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "firmware".to_string());

    let target_dir = root.join("target");
    if !target_dir.exists() {
        bail!(
            "No target/ directory found — build the firmware first (or run without --skip-build)"
        );
    }
    let preferred = target_dir
        .join(target)
        .join("release")
        .join(format!("{name}.elf"));
    if preferred.exists() {
        return Ok(preferred);
    }
    let mut matches: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&target_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let release = path.join("release");
            let elf = release.join(format!("{name}.elf"));
            if elf.exists() {
                matches.push(elf);
            }
        }
    }
    if matches.is_empty() {
        bail!(
            "No {name}.elf found in target/*/release — build the firmware first (or run without --skip-build)"
        );
    }
    matches.sort();
    matches
        .pop()
        .ok_or_else(|| anyhow!("No firmware artifact was selected"))
}

fn run_tool(root: &Path, cmd: &str, args: &[&str]) -> Result<()> {
    let execution_scope = match cmd {
        "cargo" => mgc_exec::allowlist::ExecutionScope::BuildRunner,
        "espflash" => mgc_exec::allowlist::ExecutionScope::DeviceControl,
        _ => bail!("unsupported IoT tool"),
    };
    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".magicore").join("exec.log")),
        clean_env: true,
        execution_scope: Some(execution_scope),
        ..Default::default()
    };
    let args = args.iter().map(|a| a.to_string()).collect::<Vec<_>>();
    mgc_exec::prelude::run_inherited(cmd, &args, &opts)?;
    Ok(())
}

#[cfg(test)]
#[path = "test/iot.rs"]
mod tests;
