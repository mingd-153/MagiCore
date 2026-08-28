//! cicd tooling lệnh: `mgc ci generate`, `mgc verify`, `mgc deploy`, `mgc dev` (07 §4).

use anyhow::Result;

fn not_available(reason: &str) -> anyhow::Error {
    crate::error::cicd_reason(reason)
}

/// `mgc ci generate` — sinh file CI theo provider (07 §4).
/// Workflow: checkout → setup-magicore → mgc install → mgc verify.
pub fn ci_generate() -> Result<()> {
    let root = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let provider = provider_config()?;
    match provider {
        mgc_cicd_adapter::CicdProvider::GithubActions => {
            let dir = root.join(".github").join("workflows");
            std::fs::create_dir_all(&dir)?;
            let path = dir.join("ci.yml");
            let workflow = WORKFLOW_TEMPLATE.replace("{name}", "CI");
            std::fs::write(&path, workflow)?;
            mgc_ui::success(&format!("CI workflow generated: {}", path.display()));
        }
        mgc_cicd_adapter::CicdProvider::Gitlab => {
            let path = root.join(".gitlab-ci.yml");
            std::fs::write(&path, GITLAB_TEMPLATE)?;
            mgc_ui::success(&format!("GitLab CI generated: {}", path.display()));
        }
        mgc_cicd_adapter::CicdProvider::CircleCi => {
            let dir = root.join(".circleci");
            std::fs::create_dir_all(&dir)?;
            let path = dir.join("config.yml");
            std::fs::write(&path, CIRCLE_TEMPLATE)?;
            mgc_ui::success(&format!("CircleCI config generated: {}", path.display()));
        }
        other => {
            return Err(crate::error::ci_template_unknown(other.as_str()));
        }
    }
    Ok(())
}

const WORKFLOW_TEMPLATE: &str = r#"name: {name}

on:
  push:
  pull_request:

jobs:
  ci:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install MagiCore
        run: |
          rustup toolchain install stable --profile minimal
          cargo install --git https://github.com/mingd-153/MagiCore --branch phase-4 mgc --locked
      - name: Install dependencies
        run: mgc install
      - name: Verify
        run: mgc verify
"#;

const GITLAB_TEMPLATE: &str = r#"stages:
  - ci

ci:
  stage: ci
  image: rust:1.86
  before_script:
    - rustup toolchain install stable --profile minimal
    - cargo install --git https://github.com/mingd-153/MagiCore --branch phase-4 mgc --locked
  script:
    - mgc install
    - mgc verify
"#;

const CIRCLE_TEMPLATE: &str = r#"version: 2.1
jobs:
  ci:
    docker:
      - image: cimg/rust:1.86
    steps:
      - checkout
      - run: rustup toolchain install stable --profile minimal
      - run: cargo install --git https://github.com/mingd-153/MagiCore --branch phase-4 mgc --locked
      - run: mgc install
      - run: mgc verify
workflows:
  version: 2
  ci:
    jobs:
      - ci
"#;

/// `mgc verify` — chạy chain theo adapter: audit (web P1) → test → build (07 §4).
/// 1 bước fail → dừng, báo rõ project (workspace recursive P2 — chỉ cwd P1).
pub async fn verify() -> Result<()> {
    let root = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    mgc_ui::info(&format!("[verify] project: {}", root.display()));

    let chain = verify_chain(&root)?;
    mgc_ui::info(&format!("[verify] chain: {}", chain.join(" → ")));

    let core = mgc_config::project::ProjectConfig::load(&root)
        .ok()
        .flatten()
        .map(|cfg| cfg.ecosystem)
        .unwrap_or_default();

    for step in &chain {
        match step.as_str() {
            "audit" => {
                if core == "web" {
                    crate::commands::audit::run(None, false).await?;
                } else {
                    mgc_ui::warning(
                        "audit for non-web cores is P2 (Q22) — skipping the audit step",
                    );
                }
            }
            "test" => run_test_step(&root, &core).await?,
            "build" => {
                if core == "cicd" {
                    mgc_ui::warning(
                        "cicd core has no build (07 §4) — pipelines run via `mgc ci generate`",
                    );
                } else {
                    crate::commands::build::run(None, None).await?;
                }
            }
            other => mgc_ui::warning(&format!("unknown verify step: '{other}' — skipping")),
        }
    }
    mgc_ui::success("Verify chain OK");
    Ok(())
}

/// Chain từ mgc.toml `[cicd] verify` — default ["audit", "test", "build"].
fn verify_chain(root: &std::path::Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(root.join("mgc.toml"))?;
    let v: toml::Value = toml::from_str(&content)?;
    let chain = v
        .get("cicd")
        .and_then(|c| c.get("verify"))
        .and_then(|k| k.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec!["audit".into(), "test".into(), "build".into()]);
    Ok(chain)
}

/// Test step theo core: rust → cargo test; web → package.json scripts.test (không PM wrapper).
async fn run_test_step(root: &std::path::Path, core: &str) -> Result<()> {
    if core == "web" {
        let pkg = std::fs::read_to_string(root.join("package.json"))
            .map_err(|_| crate::error::web_missing_package_json())?;
        let v: serde_json::Value = serde_json::from_str(&pkg)?;
        let has_test = v
            .get("scripts")
            .and_then(|s| s.get("test"))
            .and_then(|s| s.as_str())
            .is_some();
        if !has_test {
            return Err(crate::error::package_json_missing_test_script());
        }
        crate::commands::run::run("test".to_string(), vec![], Some("web")).await?;
    } else if core == "lib" {
        if root.join("Cargo.toml").exists() {
            let opts = mgc_exec::prelude::ExecOptions {
                cwd: Some(root.to_path_buf()),
                log_path: Some(root.join(".magicore").join("exec.log")),
                clean_env: true,
                ..Default::default()
            };
            mgc_exec::prelude::run_inherited("cargo", &["test".into()], &opts)?;
            return Ok(());
        }
        return Err(crate::error::lib_no_test_runner());
    } else {
        mgc_ui::warning(&format!(
            "test step for core '{core}' is not P1 yet — skipping (cargo test for rust, scripts.test for web)"
        ));
    }
    Ok(())
}

fn provider_config() -> Result<mgc_cicd_adapter::CicdProvider> {
    let cwd = std::env::current_dir()?;
    mgc_cicd_adapter::adapter_for(&cwd)
        .map(|a| a.provider)
        .ok_or_else(crate::error::cicd_project_not_detected)
}

/// Deploy command theo provider — dry-run mặc định, --run để chạy thật (§5.4/S2).
struct DeployCommand {
    tool: &'static str,
    args: Vec<String>,
}

/// Một target trong `[deploy] targets` (07 §3).
#[derive(Debug, serde::Deserialize)]
struct DeployTarget {
    provider: String,
    #[serde(default)]
    stack: String,
    #[serde(default)]
    region: String,
}

/// Đọc `[deploy] targets` từ mgc.toml — None = không có target, dùng provider detect.
fn deploy_targets(root: &std::path::Path) -> Result<Option<Vec<DeployTarget>>> {
    let content = match std::fs::read_to_string(root.join("mgc.toml")) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    let v: toml::Value = toml::from_str(&content)?;
    let targets = v
        .get("deploy")
        .and_then(|d| d.get("targets"))
        .and_then(|t| t.as_array())
        .filter(|arr| !arr.is_empty())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.clone().try_into().ok())
                .collect::<Vec<DeployTarget>>()
        });
    Ok(targets)
}

/// Lệnh deploy cho 1 target (dry_run=false khi --run).
fn target_deploy_command(target: &DeployTarget, dry_run: bool) -> Result<DeployCommand> {
    match target.provider.as_str() {
        "cloudflare" => Ok(DeployCommand {
            tool: "wrangler",
            args: if dry_run {
                vec!["deploy".to_string(), "--dry-run".to_string()]
            } else {
                vec!["deploy".to_string()]
            },
        }),
        "gcp" | "google" => Ok(DeployCommand {
            tool: "gcloud",
            args: if dry_run {
                vec![
                    "app".to_string(),
                    "deploy".to_string(),
                    "--no-promote".to_string(),
                ]
            } else {
                vec!["app".to_string(), "deploy".to_string()]
            },
        }),
        "aws" => {
            // Dry-run: validate template (không deploy). Thật: cloudformation deploy.
            if target.stack.is_empty() {
                return Err(crate::error::deploy_target_missing_stack());
            }
            let template = format!("{}.yaml", target.stack);
            if dry_run {
                Ok(DeployCommand {
                    tool: "aws",
                    args: vec![
                        "cloudformation".to_string(),
                        "validate-template".to_string(),
                        "--template-body".to_string(),
                        format!("file://{template}"),
                    ],
                })
            } else {
                let mut args = vec![
                    "cloudformation".to_string(),
                    "deploy".to_string(),
                    "--stack-name".to_string(),
                    target.stack.clone(),
                    "--template-body".to_string(),
                    format!("file://{template}"),
                ];
                if !target.region.is_empty() {
                    args.extend(["--region".to_string(), target.region.clone()]);
                }
                Ok(DeployCommand { tool: "aws", args })
            }
        }
        other => Err(crate::error::deploy_target_unknown(other)),
    }
}

fn deploy_command(provider: mgc_cicd_adapter::CicdProvider) -> Result<DeployCommand> {
    match provider {
        mgc_cicd_adapter::CicdProvider::Cloudflare => Ok(DeployCommand {
            tool: "wrangler",
            args: vec!["deploy".to_string(), "--dry-run".to_string()],
        }),
        mgc_cicd_adapter::CicdProvider::Gcp => Ok(DeployCommand {
            tool: "gcloud",
            args: vec!["app".to_string(), "deploy".to_string(), "--no-promote".to_string()],
        }),
        mgc_cicd_adapter::CicdProvider::GithubActions
        | mgc_cicd_adapter::CicdProvider::Gitlab
        | mgc_cicd_adapter::CicdProvider::CircleCi => Err(not_available(
            "is CI-only — push to trigger; no local deploy command.",
        )),
        mgc_cicd_adapter::CicdProvider::Aws => Err(not_available(
            "aws deploy needs a target (s3 bucket/pipeline) — configure [deploy] targets then run `mgc deploy`.",
        )),
        mgc_cicd_adapter::CicdProvider::Argocd => Err(not_available(
            "argocd runs server-side (GitOps) — commit + push to trigger sync; no local deploy command.",
        )),
    }
}

pub async fn deploy(run: bool) -> Result<()> {
    let root = std::env::current_dir()?;
    let targets = deploy_targets(&root)?;
    let commands: Vec<DeployCommand> = if let Some(targets) = targets {
        targets
            .iter()
            .map(|target| target_deploy_command(target, !run))
            .collect::<Result<Vec<_>>>()?
    } else {
        let provider = provider_config()?;
        vec![deploy_command(provider)?]
    };

    if commands.is_empty() {
        return Err(crate::error::no_deploy_targets());
    }

    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(root.clone()),
        log_path: Some(root.join(".magicore").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    for cmd in &commands {
        if !run {
            mgc_ui::info(&format!(
                "[dry-run] would run: {} {} (real deploy requires `mgc deploy --run`)",
                cmd.tool,
                cmd.args.join(" ")
            ));
            continue;
        }
        mgc_ui::info(&format!("Deploying: {} {}", cmd.tool, cmd.args.join(" ")));
        mgc_exec::prelude::run_inherited(cmd.tool, &cmd.args, &opts)?;
    }
    Ok(())
}

/// `mgc dev` cho cicd — in lệnh deploy (dry-run), không chạy thật (§5.4/S2).
pub async fn dev(dry_run: bool) -> Result<()> {
    let root = std::env::current_dir()?;
    let provider = provider_config()?;
    let cmd = deploy_command(provider)?;
    mgc_ui::info(&format!(
        "[dry-run] preview: {} {} (run with `mgc deploy --run`)",
        cmd.tool,
        cmd.args.join(" ")
    ));
    let _ = (root, dry_run);
    Ok(())
}

#[cfg(test)]
#[path = "test/cicd.rs"]
mod tests;
