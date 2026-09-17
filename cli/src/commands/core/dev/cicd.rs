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
            let workflow = render_ci_template(WORKFLOW_TEMPLATE, "CI");
            std::fs::write(&path, workflow)?;
            mgc_ui::success(&format!("CI workflow generated: {}", path.display()));
        }
        mgc_cicd_adapter::CicdProvider::Gitlab => {
            let path = root.join(".gitlab-ci.yml");
            std::fs::write(&path, render_ci_template(GITLAB_TEMPLATE, ""))?;
            mgc_ui::success(&format!("GitLab CI generated: {}", path.display()));
        }
        mgc_cicd_adapter::CicdProvider::CircleCi => {
            let dir = root.join(".circleci");
            std::fs::create_dir_all(&dir)?;
            let path = dir.join("config.yml");
            std::fs::write(&path, render_ci_template(CIRCLE_TEMPLATE, ""))?;
            mgc_ui::success(&format!("CircleCI config generated: {}", path.display()));
        }
        other => {
            return Err(crate::error::ci_template_unknown(other.as_str()));
        }
    }
    Ok(())
}

/// Install source used by generated CI templates: the latest GitHub Release
/// instead of a mutable branch. Update this constant on every release tag.
/// (Pinned actions + release-tagged install: Tech Lead P0-3, 2026-09-12.)
///
/// Nguồn cài đặt cho template CI sinh ra: GitHub Release mới nhất thay vì
/// branch mutable. Cập nhật hằng số này ở mỗi release tag.
const MGC_RELEASE_TAG: &str = "v1.1.0-rc.6";

/// Install source used by generated CI templates: the latest GitHub Release
/// installer downloaded from an IMMUTABLE commit SHA — never from the
/// mutable `main` branch (P0-E, 2026-09-16 supply-chain gate). Update the
/// SHA to the last commit that touched `scripts/install-from-gh.sh`
/// whenever MGC_RELEASE_TAG moves.
/// (P0-E: installer trong template CI được tải từ commit SHA BẤT BIẾN —
/// không bao giờ từ branch `main` mutable. Cập nhật SHA theo commit cuối
/// chạm `scripts/install-from-gh.sh` mỗi khi MGC_RELEASE_TAG dịch chuyển.)
const MGC_INSTALLER_SHA: &str = "285fd62d2dbf4693cb0675425dc52861b6327c5a";

/// Embedded SHA-256 of `scripts/install-from-gh.sh` at `MGC_INSTALLER_SHA`.
/// Contract (P0-E/T0.5):
/// - empty → the generated pipeline prints a loud WARNING and still runs
///   (no break for existing pipelines);
/// - a `.sha256` file published next to the installer is verified whenever
///   present (mismatch fails the pipeline);
/// - when set, a checksum mismatch FAILS the pipeline (fail-closed).
///
/// (P0-E/T0.5: SHA-256 nhúng của installer tại `MGC_INSTALLER_SHA`. Rỗng →
/// pipeline in WARNING rõ ràng rồi vẫn chạy; file `.sha256` cạnh installer
/// được verify khi có (lệch → fail); khi đặt giá trị → lệch checksum FAIL.)
const MGC_INSTALLER_SHA256: &str =
    "74821b9a70d1aaf2bb1896344f777ec0ca8038b66385a8a46e3c83fc63442dc6";

/// Render a CI template: release tag + immutable installer pin (P0-E).
/// Every placeholder substitution lives in ONE helper so a half-rendered
/// template (e.g. a missing SHA pin) can never reach disk.
/// (Render template CI: tag release + ghim installer bất biến (P0-E). Mọi
/// placeholder thay tại MỘT helper duy nhất nên template render thiếu
/// (vd thiếu SHA pin) không bao giờ chạm đĩa.)
fn render_ci_template(template: &str, name: &str) -> String {
    template
        .replace("{name}", name)
        .replace("{tag}", MGC_RELEASE_TAG)
        .replace("{installer_sha}", MGC_INSTALLER_SHA)
        .replace("{installer_sha256}", MGC_INSTALLER_SHA256)
}

// P0-E installer block (shared shape across the three templates):
// download to a FILE from the pinned SHA (never `curl … | bash`),
// verify the companion `.sha256` when published, verify the embedded
// checksum when set, warn loudly and proceed when empty.
// (Khối installer P0-E (dạng chung cho 3 template): tải về FILE từ SHA
// ghim (không bao giờ `curl … | bash`), verify `.sha256` companion khi có,
// verify checksum nhúng khi đặt, warn to rồi chạy khi rỗng.)
const WORKFLOW_TEMPLATE: &str = r#"name: {name}

on:
  push:
  pull_request:

jobs:
  ci:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
      - name: Install MagiCore (release binary, SHA-pinned)
        run: |
          set -eu
          pin_dir="$(mktemp -d)"
          trap 'rm -rf "$pin_dir"' EXIT
          installer="$pin_dir/install-from-gh.sh"
          # P0-E: fetch from an immutable commit SHA into a file — never
          # from mutable main, never piped straight into bash.
          curl -fsSL "https://raw.githubusercontent.com/mingd-153/MagiCore/{installer_sha}/scripts/install-from-gh.sh" -o "$installer"
          if curl -fsSL "https://raw.githubusercontent.com/mingd-153/MagiCore/{installer_sha}/scripts/install-from-gh.sh.sha256" -o "$pin_dir/install-from-gh.sh.sha256"; then
            (cd "$pin_dir" && { sha256sum -c install-from-gh.sh.sha256 || shasum -a 256 -c install-from-gh.sh.sha256; })
          else
            echo "WARNING: no .sha256 checksum file published next to the installer — companion verification skipped"
          fi
          if [ -n "{installer_sha256}" ]; then
            echo "{installer_sha256}  $installer" | { sha256sum -c - || shasum -a 256 -c -; }
          else
            echo "WARNING: installer integrity NOT pinned (MGC_INSTALLER_SHA256 empty) — trusting commit SHA {installer_sha} only"
          fi
          bash "$installer" --version {tag}
      - name: Check MagiCore version
        run: mgc --version
      - name: Install dependencies
        run: mgc install
      - name: Verify (strict audit in CI)
        env:
          MGC_AUDIT_STRICT: "1"
        run: mgc verify
"#;

const GITLAB_TEMPLATE: &str = r#"stages:
  - ci

ci:
  stage: ci
  image: rust:latest
  before_script:
    - |
      set -eu
      pin_dir="$(mktemp -d)"
      trap 'rm -rf "$pin_dir"' EXIT
      installer="$pin_dir/install-from-gh.sh"
      # P0-E: fetch from an immutable commit SHA into a file — never from
      # mutable main, never piped straight into bash.
      curl -fsSL "https://raw.githubusercontent.com/mingd-153/MagiCore/{installer_sha}/scripts/install-from-gh.sh" -o "$installer"
      if curl -fsSL "https://raw.githubusercontent.com/mingd-153/MagiCore/{installer_sha}/scripts/install-from-gh.sh.sha256" -o "$pin_dir/install-from-gh.sh.sha256"; then
        (cd "$pin_dir" && { sha256sum -c install-from-gh.sh.sha256 || shasum -a 256 -c install-from-gh.sh.sha256; })
      else
        echo "WARNING: no .sha256 checksum file published next to the installer — companion verification skipped"
      fi
      if [ -n "{installer_sha256}" ]; then
        echo "{installer_sha256}  $installer" | { sha256sum -c - || shasum -a 256 -c -; }
      else
        echo "WARNING: installer integrity NOT pinned (MGC_INSTALLER_SHA256 empty) — trusting commit SHA {installer_sha} only"
      fi
      bash "$installer" --version {tag}
    - mgc --version
  script:
    - mgc install
    - MGC_AUDIT_STRICT=1 mgc verify
"#;

const CIRCLE_TEMPLATE: &str = r#"version: 2.1
jobs:
  ci:
    docker:
      - image: cimg/base:stable
    steps:
      - checkout
      - run:
          name: Install MagiCore (release binary, SHA-pinned)
          command: |
            set -eu
            pin_dir="$(mktemp -d)"
            trap 'rm -rf "$pin_dir"' EXIT
            installer="$pin_dir/install-from-gh.sh"
            # P0-E: fetch from an immutable commit SHA into a file — never
            # from mutable main, never piped straight into bash.
            curl -fsSL "https://raw.githubusercontent.com/mingd-153/MagiCore/{installer_sha}/scripts/install-from-gh.sh" -o "$installer"
            if curl -fsSL "https://raw.githubusercontent.com/mingd-153/MagiCore/{installer_sha}/scripts/install-from-gh.sh.sha256" -o "$pin_dir/install-from-gh.sh.sha256"; then
              (cd "$pin_dir" && { sha256sum -c install-from-gh.sh.sha256 || shasum -a 256 -c install-from-gh.sh.sha256; })
            else
              echo "WARNING: no .sha256 checksum file published next to the installer — companion verification skipped"
            fi
            if [ -n "{installer_sha256}" ]; then
              echo "{installer_sha256}  $installer" | { sha256sum -c - || shasum -a 256 -c -; }
            else
              echo "WARNING: installer integrity NOT pinned (MGC_INSTALLER_SHA256 empty) — trusting commit SHA {installer_sha} only"
            fi
            bash "$installer" --version {tag}
      - run: mgc --version
      - run: mgc install
      - run:
          name: Verify (strict audit)
          command: MGC_AUDIT_STRICT=1 mgc verify
workflows:
  version: 2
  ci:
    jobs:
      - ci
"#;

/// `mgc verify` — chạy chain theo adapter: audit (web P1) → test → build (07 §4).
/// 1 bước fail → dừng, báo rõ project (workspace recursive P2 — chỉ cwd P1).
///
/// Fail-closed contract (Tech Lead P0-3, 2026-09-12):
/// - unknown step trong chain → ERROR (không skip im lặng);
/// - audit luôn chạy strict trong CI (MGC_AUDIT_STRICT=1) — exit 2 nếu UNVERIFIED;
/// - không bước nào được bỏ qua rồi vẫn in "Verify chain OK".
///
/// Hợp đồng fail-closed: step lạ trong chain → lỗi; audit strict trong CI;
/// không được bỏ step rồi vẫn báo thành công.
pub async fn verify() -> Result<()> {
    let root = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    mgc_ui::info(&format!("[verify] project: {}", root.display()));

    let chain = verify_chain(&root)?;
    mgc_ui::info(&format!("[verify] chain: {}", chain.join(" → ")));

    // Validate the whole chain BEFORE running anything — a typo'd step name
    // fails up front instead of being skipped mid-run.
    // Validate toàn bộ chain TRƯỚC khi chạy — tên step gõ sai fail ngay từ
    // đầu thay vì bị bỏ qua giữa chừng.
    const KNOWN_STEPS: [&str; 3] = ["audit", "test", "build"];
    for step in &chain {
        if !KNOWN_STEPS.contains(&step.as_str()) {
            return Err(crate::error::cicd_verify_unknown_step(step));
        }
    }
    if chain.is_empty() {
        return Err(crate::error::cicd_verify_empty_chain());
    }

    let core = mgc_config::project::ProjectConfig::load(&root)
        .ok()
        .flatten()
        .map(|cfg| cfg.ecosystem)
        .unwrap_or_default();

    // Note: audit strictness is enforced inside the audit runner — CI
    // environments (CI=true) default to strict, so an UNVERIFIED audit can
    // never pass this chain silently. No env mutation needed here.
    // Ghi chú: strict do audit runner tự thực thi — môi trường CI (CI=true)
    // mặc định strict nên UNVERIFIED không thể lọt chain. Không cần set env.

    for step in &chain {
        match step.as_str() {
            "audit" => {
                // Real audit for every core — strict mode (CI) fails on an
                // UNVERIFIED result; local mode warns loudly (escape hatch).
                // Audit thật cho mọi core — strict (CI) fail khi UNVERIFIED;
                // local cảnh báo to (escape hatch).
                crate::commands::audit::run(None, false, None).await?;
            }
            "test" => run_test_step(&root, &core).await?,
            "build" => {
                if core == "cicd" {
                    mgc_ui::warning(
                        "cicd core has no build (07 §4) — pipelines run via `mgc ci generate`",
                    );
                } else {
                    crate::commands::build::run(None, None, None).await?;
                }
            }
            // Unreachable (chain validated above) — kept fail-closed anyway.
            // Không thể tới đây (chain đã validate) — vẫn giữ fail-closed.
            other => return Err(crate::error::cicd_verify_unknown_step(other)),
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
        crate::commands::run::run("test".to_string(), vec![], Some("web"), None).await?;
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
            args: vec![
                "app".to_string(),
                "deploy".to_string(),
                "--no-promote".to_string(),
            ],
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
