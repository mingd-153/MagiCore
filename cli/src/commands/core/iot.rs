use anyhow::{bail, Result};
use mg_types::adapter::PackageAdapter;
use mg_types::Ecosystem;
use mg_ui::info;
use std::path::PathBuf;
use std::sync::Arc;

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| {
        anyhow::anyhow!("failed to resolve current working directory — has it been deleted?: {e}")
    })?;
    let root = super::shared::find_project_root(&cwd)?.ok_or_else(|| {
        anyhow::anyhow!(
            "No MegaGate IoT project found (missing mg.toml with ecosystem = \"iot\" or platformio.ini/west.yml/Cargo.toml in the current project)"
        )
    })?;
    Ok(root)
}

fn iot_adapter() -> Arc<dyn PackageAdapter> {
    crate::factory::create_adapter(&Ecosystem::Iot, None, None)
        .expect("iot adapter always available in iot core build")
}

/// Build + flash firmware esp32 (Q16). Board đọc từ arg hoặc mg.toml `[iot] board`;
/// fail-closed nếu không xác định target (00-index §3, 04 §3).
pub async fn flash(board_override: Option<&str>, skip_build: bool) -> Result<()> {
    let root = project_root()?;
    let adapter = mg_iot_adapter::adapter_for(&root)
        .ok_or_else(|| anyhow::anyhow!("No IoT framework detected in {}", root.display()))?;

    if adapter.framework() != "esp32-rust" {
        bail!(
            "'mg flash' hiện chỉ hỗ trợ framework esp32-rust ({} đang dùng) — platformio/zephyr flash là P2",
            adapter.framework()
        );
    }

    let board = board_override
        .map(str::to_string)
        .or_else(|| adapter.board(&root))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No board specified — add `[iot] board = \"<id>\"` to mg.toml (known boards: {}) or pass `mg flash --board <id>`",
                mg_iot_adapter::known_boards()
                    .iter()
                    .map(|(id, _, _)| id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    let target = adapter
        .target(&root)
        .or_else(|| mg_iot_adapter::board_target(&board))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Unsupported board '{board}' — known boards: {}",
                mg_iot_adapter::known_boards()
                    .iter()
                    .map(|(id, _, _)| id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    info(&format!(
        "Board: {board} ({}), target: {target}",
        chip(&board)
    ));

    if !skip_build {
        info("Building firmware (cargo build --release)...");
        let build_args = ["build", "--release", "--target", &target];
        run_tool(&root, "cargo", &build_args)?;
    }

    let elf = find_elf(&root, &target)?;
    let elf_str = elf
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("firmware path is not valid UTF-8: {}", elf.display()))?;
    info(&format!("Flashing firmware: espflash flash {elf_str}"));
    let flash_args = ["flash", elf_str];
    run_tool(&root, "espflash", &flash_args)
        .map_err(|e| anyhow::anyhow!("espflash failed: {e} — install espflash first (`cargo install espflash`), it must be in PATH"))
}

/// Board id → chip (tra registry KNOWN_BOARDS).
fn chip(board: &str) -> String {
    mg_iot_adapter::known_boards()
        .iter()
        .find(|(id, _, _)| id == board)
        .map(|(_, chip, _)| chip.clone())
        .unwrap_or_else(|| board.to_string())
}

/// Tìm binary ELF đã build — ưu tiên target/<triple>/release/<name>.elf, fallback scan.
fn find_elf(root: &PathBuf, target: &str) -> Result<PathBuf> {
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
    Ok(matches.pop().unwrap())
}

fn run_tool(root: &PathBuf, cmd: &str, args: &[&str]) -> Result<()> {
    let opts = mg_exec::prelude::ExecOptions {
        cwd: Some(root.clone()),
        log_path: Some(root.join(".megagate").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    let args = args.iter().map(|a| a.to_string()).collect::<Vec<_>>();
    mg_exec::prelude::run_inherited(cmd, &args, &opts)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn add(
    packages: Vec<String>,
    version: Option<String>,
    dev: bool,
    exact: bool,
    optional: bool,
    peer: bool,
    no_save: bool,
    global: bool,
) -> Result<()> {
    let root = project_root()?;
    let adapter = iot_adapter();
    super::shared::add(
        &*adapter, &root, packages, version, dev, exact, optional, peer, no_save, true, global,
    )
    .await
}

pub async fn remove(packages: Vec<String>) -> Result<()> {
    let root = project_root()?;
    let adapter = iot_adapter();
    super::shared::remove(&*adapter, &root, packages, true).await
}

pub async fn list() -> Result<()> {
    let root = project_root()?;
    let adapter = iot_adapter();
    super::shared::list(&*adapter, &root).await
}

pub async fn update(packages: Vec<String>, install: bool) -> Result<()> {
    let root = project_root()?;
    let adapter = iot_adapter();
    super::shared::update(&*adapter, &root, packages, install).await
}

pub async fn install(packages: Vec<String>) -> Result<()> {
    let root = project_root()?;
    let adapter = iot_adapter();
    for pkg in &packages {
        let spinner = mg_ui::create_spinner(&format!("  Adding {}...", pkg));
        let name = mg_types::PackageName::new(pkg)?;
        let opts = mg_types::adapter::AddOptions::default();
        adapter.add(&root, &name, None, opts).await?;
        spinner.finish_and_clear();
    }
    super::shared::install_with_adapter(
        &*adapter,
        &root,
        "mg add",
        false,
        mg_types::adapter::InstallOptions {
            legacy_flat: false,
            ..Default::default()
        },
    )
    .await
}

pub mod create {
    use anyhow::Result;

    pub async fn run(framework: &str, project_name: &str) -> Result<()> {
        let mut config = crate::wizard::iot::IotWizard::run();
        config.project_name = project_name.to_string();
        if !framework.is_empty() {
            config.frameworks = vec![framework.to_string()];
        }
        if let Some(fw) = config.frameworks.first() {
            // Registry-first: fetch layer iot/<fw> nếu chưa có; fetch fail → fallback procedural.
            crate::commands::template::ensure_layer(&format!("iot/{fw}")).await;
        }
        crate::scaffold::processor::Scaffolder::scaffold(&config)?;
        mg_ui::success("IoT project created. Run `mg add-iot <pkg>` or `mg install-iot` next.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mg-iot-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn chip_resolves_from_registry() {
        assert_eq!(chip("esp32c3"), "esp32c3");
        assert_eq!(chip("unknown-board"), "unknown-board");
    }

    #[test]
    fn find_elf_locates_release_binary() {
        let dir = tmp_dir("elf");
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"firmware\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let target = dir
            .join("target")
            .join("riscv32imac-unknown-none-elf")
            .join("release");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("firmware.elf"), "ELF").unwrap();
        let elf = find_elf(&dir, "riscv32imac-unknown-none-elf").unwrap();
        assert!(elf.ends_with("firmware.elf"));
    }

    #[test]
    fn find_elf_prefers_requested_target() {
        let dir = tmp_dir("elf2");
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"firmware\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        for triple in ["thumbv7em-none-eabihf", "riscv32imac-unknown-none-elf"] {
            let target = dir.join("target").join(triple).join("release");
            std::fs::create_dir_all(&target).unwrap();
            std::fs::write(target.join("firmware.elf"), "ELF").unwrap();
        }
        let elf = find_elf(&dir, "riscv32imac-unknown-none-elf").unwrap();
        assert!(elf
            .to_string_lossy()
            .contains("riscv32imac-unknown-none-elf"));
    }

    #[test]
    fn find_elf_bails_without_build() {
        let dir = tmp_dir("noelf");
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"firmware\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        assert!(find_elf(&dir, "riscv32imac-unknown-none-elf").is_err());
    }
}
