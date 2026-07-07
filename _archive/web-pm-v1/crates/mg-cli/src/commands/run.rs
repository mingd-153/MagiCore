use std::path::{Path, PathBuf};
use std::process::Command as ProcessCmd;

use colored::Colorize;
use mg_core::config::MgpmConfig;

use super::super::{load_package_json, run_on_members};

pub fn cmd_run_recursive(
    members: &[&mg_workspace::WorkspaceMember],
    script: &str,
    args: &[String],
    fail_fast: bool,
) -> Result<(), String> {
    run_on_members(members, "run", fail_fast, |member| {
        println!("[{}] Running '{}'...", member.name.green(), script);

        let pkg_path = member.path.join("package.json");
        let pkg = load_package_json(&pkg_path)?;
        let scripts = pkg
            .get("scripts")
            .and_then(|s| s.as_object())
            .ok_or_else(|| format!("no 'scripts' field in {}", pkg_path.display()))?;
        let script_cmd = scripts
            .get(script)
            .and_then(|s| s.as_str())
            .ok_or_else(|| format!("script '{}' not found in {}", script, pkg_path.display()))?;

        let path = format!(
            "{}:{}",
            member.path.join("node_modules").join(".bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );

        let mut cmd_string = script_cmd.to_string();
        for arg in args {
            cmd_string.push(' ');
            cmd_string.push_str(arg);
        }

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd_string)
            .current_dir(&member.path)
            .env("PATH", &path)
            .output()
            .map_err(|e| format!("failed to execute: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.is_empty() {
            print!("{stdout}");
        }

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.is_empty() {
                eprintln!("{}", stderr.red());
            }
            return Err(format!("exit code: {:?}", output.status.code()));
        }
        Ok(())
    })
}

pub fn cmd_exec_recursive(
    members: &[&mg_workspace::WorkspaceMember],
    command: &str,
    args: &[String],
    fail_fast: bool,
) -> Result<(), String> {
    run_on_members(members, "exec", fail_fast, |member| {
        println!("[{}] Executing '{}'...", member.name.green(), command);
        let bin_path = member.path.join("node_modules").join(".bin").join(command);
        let executable = if bin_path.exists() {
            bin_path
        } else {
            PathBuf::from(command)
        };

        let path = format!(
            "{}:{}",
            member.path.join("node_modules").join(".bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );

        let output = std::process::Command::new(&executable)
            .args(args)
            .current_dir(&member.path)
            .env("PATH", &path)
            .output()
            .map_err(|e| format!("failed to execute command: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.is_empty() {
            print!("{stdout}");
        }

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.is_empty() {
                eprintln!("{}", stderr.red());
            }
            return Err(format!("exit code: {:?}", output.status.code()));
        }
        Ok(())
    })
}

fn build_node_bin_path() -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    let node_bin = format!("{}/node_modules/.bin", cwd.display());
    match std::env::var("PATH") {
        Ok(existing) => format!("{}:{}", node_bin, existing),
        Err(_) => format!("{}:/usr/bin:/bin:/usr/local/bin", node_bin),
    }
}

pub fn run_script(script_name: &str) -> Result<(), String> {
    let pkg = load_package_json(Path::new("package.json"))?;
    let scripts = pkg
        .get("scripts")
        .and_then(|s| s.as_object())
        .ok_or_else(|| "no 'scripts' field in package.json".to_string())?;
    let script_cmd = scripts
        .get(script_name)
        .and_then(|s| s.as_str())
        .ok_or_else(|| format!("script '{}' not found in package.json", script_name))?;

    let path = build_node_bin_path();
    let status = ProcessCmd::new("sh")
        .arg("-c")
        .arg(script_cmd)
        .env("PATH", &path)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| format!("failed to run script: {}", e))?;

    if status.success() {
        println!(
            "{} Script '{}' completed",
            "[OK]".green().bold(),
            script_name
        );
        Ok(())
    } else {
        Err(format!(
            "Script '{}' failed with exit code {:?}",
            script_name,
            status.code()
        ))
    }
}

pub fn run_dev(args: &[String], _config: &MgpmConfig) -> Result<(), Box<dyn std::error::Error>> {
    let pkg = load_package_json(Path::new("package.json"))?;
    let dev_script = pkg["scripts"]["dev"]
        .as_str()
        .ok_or("No \"dev\" script found in package.json")?
        .to_string();
    tracing::info!("Starting dev server: {}", dev_script);

    let path = build_node_bin_path();
    let mut cmd_string = dev_script;
    for arg in args {
        cmd_string.push(' ');
        cmd_string.push_str(arg);
    }
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(&cmd_string)
        .env("PATH", &path)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

pub fn exec_command(cmd: &str, args: &[String]) -> Result<(), String> {
    let bin_path = PathBuf::from("node_modules").join(".bin").join(cmd);
    let executable = if bin_path.exists() {
        bin_path
    } else {
        PathBuf::from(cmd)
    };

    let path = build_node_bin_path();
    let status = ProcessCmd::new(&executable)
        .args(args)
        .env("PATH", &path)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| format!("failed to execute '{}': {}", cmd, e))?;

    if status.success() {
        println!("{} Command '{}' completed", "[OK]".green().bold(), cmd);
        Ok(())
    } else {
        Err(format!(
            "Command '{}' failed with exit code {:?}",
            cmd,
            status.code()
        ))
    }
}
