//! `mg dev`/`mg deploy` clo — cloud tooling (terraform/cdk/pulumi) — tách từ core/clo.rs (v5).

use anyhow::Result;
use std::path::{Path, PathBuf};

fn project_root() -> Result<PathBuf> {
    super::super::shared::core_project_root("clo")
}

/// Cloud type từ mg.toml `[cloud] type` hoặc manifest probe — dùng cho dev/deploy.
pub fn cloud_type(root: &Path) -> anyhow::Result<String> {
    let adapter = mg_cloud_adapter::adapter_for(root)
        .ok_or_else(|| crate::error::no_framework_detected("cloud", root))?;
    Ok(adapter.cloud_type().to_string())
}

pub async fn dev(dry_run: bool) -> Result<()> {
    let root = project_root()?;
    let kind = cloud_type(&root)?;
    let (cmd, args) = dev_command(&kind)?;
    if dry_run {
        mg_ui::info(&format!("[dry-run] would run: {} {}", cmd, args.join(" ")));
        return Ok(());
    }
    run_tool(&root, &cmd, &args)?;
    Ok(())
}

/// `mg deploy` — mặc định dry-run (in lệnh deploy theo type, KHÔNG chạy);
/// chạy thật chỉ với `--run` (spec §4: deploy = hành động ghi cloud).
pub async fn deploy(run: bool) -> Result<()> {
    let root = project_root()?;
    let kind = cloud_type(&root)?;
    let (cmd, args) = deploy_command(&kind)?;
    if !run {
        mg_ui::info(&format!(
            "[dry-run] would run: {} {} (real deploy requires `mg deploy --run`)",
            cmd,
            args.join(" ")
        ));
        return Ok(());
    }
    mg_ui::info(&format!("Deploying: {} {}", cmd, args.join(" ")));
    run_tool(&root, &cmd, &args)?;
    Ok(())
}

fn dev_command(kind: &str) -> Result<(String, Vec<String>)> {
    match kind {
        "terraform" => Ok(("terraform".to_string(), vec!["plan".to_string()])),
        // cdk/pulumi: npm-installed CLIs — resolve node_modules/.bin (allowlist §3: "qua .bin"),
        // nhưng dry-run in tên tool, resolve bin ở bước chạy thật.
        "cdk" => Ok(("cdk".to_string(), vec!["synth".to_string()])),
        "pulumi" => Ok(("pulumi".to_string(), vec!["preview".to_string()])),
        other => Err(crate::error::dev_cloud_not_implemented(other)),
    }
}

/// cdk/pulumi chạy từ node_modules/.bin (npm-installed, allowlist §3);
/// thiếu → lỗi rõ hướng `mg install` trước.
fn bin_resolved_path(root: &std::path::Path, cmd: &str) -> Option<std::path::PathBuf> {
    match cmd {
        "cdk" | "pulumi" => {
            let bin = root.join("node_modules").join(".bin").join(cmd);
            bin.is_file().then_some(bin)
        }
        _ => None,
    }
}

pub fn deploy_command(kind: &str) -> Result<(String, Vec<String>)> {
    match kind {
        "terraform" => Ok(("terraform".to_string(), vec!["apply".to_string()])),
        "cdk" => Ok(("cdk".to_string(), vec!["deploy".to_string()])),
        "pulumi" => Ok(("pulumi".to_string(), vec!["up".to_string()])),
        other => Err(crate::error::deploy_not_implemented(other)),
    }
}

fn run_tool(root: &Path, cmd: &str, args: &[String]) -> Result<()> {
    let (resolved, run_cmd): (Option<PathBuf>, String) =
        if let Some(bin) = bin_resolved_path(root, cmd) {
            (Some(bin.clone()), bin.to_string_lossy().to_string())
        } else {
            (None, cmd.to_string())
        };
    // cdk/pulumi là npm-installed tools — chưa cài → lỗi rõ hướng `mg install`.
    if resolved.is_none() && matches!(cmd, "cdk" | "pulumi") {
        return Err(crate::error::tool_not_installed_project(cmd));
    }
    let opts = mg_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".megagate").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    let res = mg_exec::prelude::run_inherited(&run_cmd, args, &opts);
    if resolved.is_some() {
        return match res {
            Ok(_) => Ok(()),
            Err(e) => Err(crate::error::project_tool_failed(cmd, &e)),
        };
    }
    res.map(|_| ())
}

#[cfg(test)]
#[path = "test/clo.rs"]
mod tests;
