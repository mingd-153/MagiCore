use anyhow::Result;

fn not_available(reason: &str) -> anyhow::Error {
    anyhow::anyhow!("'cicd' {reason}")
}

/// `mg ci generate` — sinh .github/workflows/ci.yml cho github-actions (07 §4).
/// Workflow: checkout → setup-megagate → mg install → mg verify → (deploy step tùy chọn qua `--run`).
pub fn ci_generate() -> Result<()> {
    let root =
        std::env::current_dir().map_err(|e| anyhow::anyhow!("failed to resolve cwd: {e}"))?;
    let provider = provider_config()?;
    if provider != mg_cicd_adapter::CicdProvider::GithubActions {
        anyhow::bail!(
            "'mg ci generate' hiện chỉ hỗ trợ github-actions (provider hiện tại: {}); gitlab/circle P2 (07 §4)",
            provider.as_str()
        );
    }
    let dir = root.join(".github").join("workflows");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("ci.yml");
    let workflow = WORKFLOW_TEMPLATE.replace("{name}", "CI");
    std::fs::write(&path, workflow)?;
    mg_ui::success(&format!("CI workflow generated: {}", path.display()));
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
      - name: Install MegaGate
        run: |
          rustup toolchain install stable --profile minimal
          cargo install --git https://github.com/mingd-153/MegaGate --branch phase-4 mg --locked
      - name: Install dependencies
        run: mg install
      - name: Verify
        run: mg verify
"#;

/// `mg verify` — chạy chain theo adapter: audit (web P1) → test → build (07 §4).
/// 1 bước fail → dừng, báo rõ project (workspace recursive P2 — chỉ cwd P1).
pub async fn verify() -> Result<()> {
    let root =
        std::env::current_dir().map_err(|e| anyhow::anyhow!("failed to resolve cwd: {e}"))?;
    mg_ui::info(&format!("[verify] project: {}", root.display()));

    let chain = verify_chain(&root)?;
    mg_ui::info(&format!("[verify] chain: {}", chain.join(" → ")));

    let core = mg_config::project::ProjectConfig::load(&root)
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
                    mg_ui::warning("audit non-web bail P2 (Q22) — bỏ qua bước audit");
                }
            }
            "test" => run_test_step(&root, &core).await?,
            "build" => {
                if core == "cicd" {
                    mg_ui::warning(
                        "cicd core không có build (07 §4) — pipeline chạy qua `mg ci generate`",
                    );
                } else {
                    crate::commands::build::run(None, None).await?;
                }
            }
            other => mg_ui::warning(&format!("bước verify không biết: '{other}' — bỏ qua")),
        }
    }
    mg_ui::success("Verify chain OK");
    Ok(())
}

/// Chain từ mg.toml `[cicd] verify` — default ["audit", "test", "build"].
fn verify_chain(root: &std::path::Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(root.join("mg.toml"))?;
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
        let pkg = std::fs::read_to_string(root.join("package.json")).map_err(|_| {
            anyhow::anyhow!("web project thiếu package.json — không chạy được test")
        })?;
        let v: serde_json::Value = serde_json::from_str(&pkg)?;
        let has_test = v
            .get("scripts")
            .and_then(|s| s.get("test"))
            .and_then(|s| s.as_str())
            .is_some();
        if !has_test {
            anyhow::bail!("package.json thiếu scripts.test");
        }
        crate::commands::run::run("test".to_string(), vec![], Some("web")).await?;
    } else if core == "lib" {
        if root.join("Cargo.toml").exists() {
            let opts = mg_exec::prelude::ExecOptions {
                cwd: Some(root.to_path_buf()),
                log_path: Some(root.join(".megagate").join("exec.log")),
                clean_env: true,
                ..Default::default()
            };
            mg_exec::prelude::run_inherited("cargo", &["test".into()], &opts)?;
            return Ok(());
        }
        anyhow::bail!("lib core chưa có test runner cho ngôn ngữ này (07 §4 P1)")
    } else {
        mg_ui::warning(&format!(
            "test step cho core '{core}' chưa có P1 — bỏ qua (cargo test cho rust, scripts.test cho web)"
        ));
    }
    Ok(())
}

fn provider_config() -> Result<mg_cicd_adapter::CicdProvider> {
    let cwd = std::env::current_dir()?;
    mg_cicd_adapter::adapter_for(&cwd)
        .map(|a| a.provider)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Cannot detect a cicd project here (missing mg.toml [cicd] provider / wrangler.toml / argocd / .github/workflows)."
            )
        })
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

/// Đọc `[deploy] targets` từ mg.toml — None = không có target, dùng provider detect.
fn deploy_targets(root: &std::path::Path) -> Result<Option<Vec<DeployTarget>>> {
    let content = match std::fs::read_to_string(root.join("mg.toml")) {
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
                vec!["app".to_string(), "deploy".to_string(), "--no-promote".to_string()]
            } else {
                vec!["app".to_string(), "deploy".to_string()]
            },
        }),
        "aws" => {
            // Dry-run: validate template (không deploy). Thật: cloudformation deploy.
            if target.stack.is_empty() {
                return Err(anyhow::anyhow!(
                    "aws target thiếu `stack` trong [deploy] targets — ví dụ: stack = \"my-infra\""
                ));
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
        other => Err(not_available(&format!(
            "deploy target '{other}' chưa có adapter — hỗ trợ aws | cloudflare | gcp"
        ))),
    }
}

fn deploy_command(provider: mg_cicd_adapter::CicdProvider) -> Result<DeployCommand> {
    match provider {
        mg_cicd_adapter::CicdProvider::Cloudflare => Ok(DeployCommand {
            tool: "wrangler",
            args: vec!["deploy".to_string(), "--dry-run".to_string()],
        }),
        mg_cicd_adapter::CicdProvider::Gcp => Ok(DeployCommand {
            tool: "gcloud",
            args: vec!["app".to_string(), "deploy".to_string(), "--no-promote".to_string()],
        }),
        mg_cicd_adapter::CicdProvider::GithubActions => Err(not_available(
            "github-actions is CI-only — push to trigger; no local deploy command.",
        )),
        mg_cicd_adapter::CicdProvider::Aws => Err(not_available(
            "aws deploy needs a target (s3 bucket/pipeline) — configure [deploy] targets then run `mg deploy`.",
        )),
        mg_cicd_adapter::CicdProvider::Argocd => Err(not_available(
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
        anyhow::bail!("không có deploy target — thêm [deploy] targets vào mg.toml");
    }

    let opts = mg_exec::prelude::ExecOptions {
        cwd: Some(root.clone()),
        log_path: Some(root.join(".megagate").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    for cmd in &commands {
        if !run {
            mg_ui::info(&format!(
                "[dry-run] would run: {} {} (deploy chạy thật cần `mg deploy --run`)",
                cmd.tool,
                cmd.args.join(" ")
            ));
            continue;
        }
        mg_ui::info(&format!("Deploying: {} {}", cmd.tool, cmd.args.join(" ")));
        mg_exec::prelude::run_inherited(cmd.tool, &cmd.args, &opts)?;
    }
    Ok(())
}

/// `mg dev` cho cicd — in lệnh deploy (dry-run), không chạy thật (§5.4/S2).
pub async fn dev(_dry_run: bool) -> Result<()> {
    let root = std::env::current_dir()?;
    let provider = provider_config()?;
    let cmd = deploy_command(provider)?;
    mg_ui::info(&format!(
        "[dry-run] preview: {} {} (run with `mg deploy --run`)",
        cmd.tool,
        cmd.args.join(" ")
    ));
    let _ = root;
    Ok(())
}

fn verb_hint(verb: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "'{verb}' không áp dụng cho cicd core (07 §4) — dùng `mg ci generate`, `mg verify`, hoặc `mg deploy` (dry-run mặc định). Provider CLI thay thế: wrangler / gcloud / aws (exec passthrough)."
    )
}

#[allow(clippy::too_many_arguments)]
pub async fn add(
    packages: Vec<String>,
    _version: Option<String>,
    _dev: bool,
    _exact: bool,
    _optional: bool,
    _peer: bool,
    _no_save: bool,
    _global: bool,
) -> Result<()> {
    let _ = packages;
    Err(verb_hint("add"))
}
pub async fn remove(_packages: Vec<String>) -> Result<()> {
    Err(verb_hint("remove"))
}
pub async fn list() -> Result<()> {
    Err(verb_hint("list"))
}
pub async fn update(_packages: Vec<String>, _install: bool) -> Result<()> {
    Err(verb_hint("update"))
}
pub async fn install(_packages: Vec<String>, _dry_run: bool) -> Result<()> {
    Err(verb_hint("install"))
}

pub mod create {
    use anyhow::Result;

    pub async fn run(framework: &str, project_name: &str) -> Result<()> {
        let mut config = crate::wizard::cicd::CicdWizard::run();
        config.project_name = project_name.to_string();
        if !framework.is_empty() {
            config.frameworks = vec![framework.to_string()];
        }
        if let Some(fw) = config.frameworks.first() {
            // Registry-first: fetch layer cicd/<fw> nếu chưa có; fetch fail → fallback procedural.
            crate::commands::template::ensure_layer(&format!("cicd/{fw}")).await;
        }
        crate::scaffold::processor::Scaffolder::scaffold(&config)?;
        mg_ui::success("CICD project created. Run `mg deploy` (dry-run) to preview deployment.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloudflare_deploy_command() {
        let cmd = deploy_command(mg_cicd_adapter::CicdProvider::Cloudflare).expect("cloudflare ok");
        assert_eq!(cmd.tool, "wrangler");
        assert_eq!(cmd.args, vec!["deploy", "--dry-run"]);
    }

    #[test]
    fn gcp_deploy_command() {
        let cmd = deploy_command(mg_cicd_adapter::CicdProvider::Gcp).expect("gcp ok");
        assert_eq!(cmd.tool, "gcloud");
        assert_eq!(cmd.args, vec!["app", "deploy", "--no-promote"]);
    }

    #[test]
    fn target_deploy_commands() {
        let cf = DeployTarget {
            provider: "cloudflare".into(),
            stack: String::new(),
            region: String::new(),
        };
        assert_eq!(
            target_deploy_command(&cf, true).unwrap().args,
            vec!["deploy", "--dry-run"]
        );
        let aws = DeployTarget {
            provider: "aws".into(),
            stack: "my-infra".into(),
            region: "ap-southeast-1".into(),
        };
        let cmd = target_deploy_command(&aws, false).unwrap();
        assert_eq!(cmd.tool, "aws");
        assert!(cmd.args.iter().any(|a| a == "my-infra"));
        assert!(cmd.args.iter().any(|a| a == "ap-southeast-1"));
        let no_stack = DeployTarget {
            provider: "aws".into(),
            stack: String::new(),
            region: String::new(),
        };
        assert!(target_deploy_command(&no_stack, true).is_err());
    }

    #[test]
    fn ci_only_providers_bail() {
        assert!(deploy_command(mg_cicd_adapter::CicdProvider::GithubActions).is_err());
        assert!(deploy_command(mg_cicd_adapter::CicdProvider::Aws).is_err());
        assert!(deploy_command(mg_cicd_adapter::CicdProvider::Argocd).is_err());
    }

    #[test]
    fn verify_chain_parses_custom_or_default() {
        let tmp = std::env::temp_dir().join(format!("mg-cicd-chain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("mg.toml"),
            "[cicd]\nprovider = \"github-actions\"\nverify = [\"audit\", \"build\"]\n",
        )
        .unwrap();
        assert_eq!(verify_chain(&tmp).unwrap(), vec!["audit", "build"]);
        std::fs::write(
            tmp.join("mg.toml"),
            "[cicd]\nprovider = \"github-actions\"\n",
        )
        .unwrap();
        assert_eq!(verify_chain(&tmp).unwrap(), vec!["audit", "test", "build"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
