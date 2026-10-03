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

/// Pin generated CI to the exact version of the MagiCore binary generating it.
/// (Workflow mới ghim đúng version binary MagiCore đang sinh ra nó.)
const MGC_RELEASE_TAG: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// Install source used by generated CI templates: the latest GitHub Release
/// installer downloaded from an IMMUTABLE commit SHA — never from the
/// mutable `main` branch (P0-E, 2026-09-16 supply-chain gate). Update the
/// SHA to the last commit that touched `scripts/install-from-gh.sh`
/// whenever MGC_RELEASE_TAG moves.
/// (P0-E: installer trong template CI được tải từ commit SHA BẤT BIẾN —
/// không bao giờ từ branch `main` mutable. Cập nhật SHA theo commit cuối
/// chạm `scripts/install-from-gh.sh` mỗi khi MGC_RELEASE_TAG dịch chuyển.)
const MGC_INSTALLER_SHA: &str = "80a383763377413c4a50d8de3bd815a0f8f01284";

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
    "2e722bbebdccad1f00719da21f56963d44ff519d3868538f9abdc2e03a27ef56";

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
                // A verify chain is a release gate: incomplete scanner coverage fails locally and in CI.
                // Chuỗi verify là cổng phát hành: scanner thiếu coverage phải lỗi cả local lẫn CI.
                crate::commands::audit::run_strict(None, false, None).await?;
            }
            "test" => run_test_step(&root, &core).await?,
            "build" => {
                ensure_build_step_supported(&core)?;
                crate::commands::build::run(None, None, None).await?;
            }
            // Unreachable (chain validated above) — kept fail-closed anyway.
            // Không thể tới đây (chain đã validate) — vẫn giữ fail-closed.
            other => return Err(crate::error::cicd_verify_unknown_step(other)),
        }
    }
    mgc_ui::success("Verify chain OK");
    Ok(())
}

/// Reject a configured build step for CI/CD projects, which produce pipeline configuration rather than a build artifact.
/// Từ chối bước build được cấu hình cho project CI/CD vì đầu ra là pipeline config, không phải build artifact.
fn ensure_build_step_supported(core: &str) -> Result<()> {
    if core == "cicd" {
        return Err(crate::error::build_not_supported(
            core,
            "CI/CD projects have no build artifact; remove `build` from `[cicd].verify` or define a project-specific build command",
        ));
    }
    Ok(())
}

/// Chain từ mgc.toml `[cicd] verify` — default ["audit", "test", "build"].
fn verify_chain(root: &std::path::Path) -> Result<Vec<String>> {
    let content =
        mgc_config::project::read_regular_project_text(&root.join("mgc.toml"), "project config")?
            .ok_or_else(|| anyhow::anyhow!("project config mgc.toml is missing"))?;
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

/// Run the shared `mgc test` policy for this project root; never report a skipped step as success.
/// Dùng chung chính sách `mgc test` trên project root; không bao giờ báo thành công khi đã skip.
async fn run_test_step(root: &std::path::Path, core: &str) -> Result<()> {
    let core_override = (!core.is_empty()).then_some(core);
    crate::commands::test::test_at(root, Vec::new(), core_override, None)
        .await
        .map_err(|error| crate::error::cicd_test_step_failed(core, &error))
}

fn provider_config() -> Result<mgc_cicd_adapter::CicdProvider> {
    let cwd = std::env::current_dir()?;
    mgc_cicd_adapter::adapter_for(&cwd)
        .map(|a| a.provider)
        .ok_or_else(crate::error::cicd_project_not_detected)
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
    let content = match mgc_config::project::read_regular_project_text(
        &root.join("mgc.toml"),
        "project config",
    ) {
        Ok(Some(content)) => content,
        Ok(None) => return Ok(None),
        Err(error) => return Err(error),
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

/// Reject provider CLI pass-through until MagiCore owns a native deploy path.
/// (Từ chối gọi CLI vendor cho tới khi MagiCore có đường deploy native.)
fn target_deploy_unavailable(target: &DeployTarget) -> anyhow::Error {
    match target.provider.as_str() {
        "cloudflare" | "gcp" | "google" => crate::error::deploy_not_implemented(&target.provider),
        "aws" => crate::error::cicd_deploy_target_not_implemented(
            &target.provider,
            &target.stack,
            if target.region.is_empty() {
                "default"
            } else {
                &target.region
            },
        ),
        other => crate::error::deploy_target_unknown(other),
    }
}

fn provider_deploy_unavailable(provider: mgc_cicd_adapter::CicdProvider) -> anyhow::Error {
    match provider {
        mgc_cicd_adapter::CicdProvider::Cloudflare => {
            crate::error::deploy_not_implemented("cloudflare")
        }
        mgc_cicd_adapter::CicdProvider::Gcp => crate::error::deploy_not_implemented("gcp"),
        mgc_cicd_adapter::CicdProvider::GithubActions
        | mgc_cicd_adapter::CicdProvider::Gitlab
        | mgc_cicd_adapter::CicdProvider::CircleCi => {
            not_available("is CI-only — push to trigger; no local deploy command.")
        }
        mgc_cicd_adapter::CicdProvider::Aws => crate::error::deploy_not_implemented("aws"),
        mgc_cicd_adapter::CicdProvider::Argocd => not_available(
            "argocd runs server-side (GitOps) — commit + push to trigger sync; no local deploy command.",
        ),
    }
}

pub async fn deploy(_run: bool) -> Result<()> {
    let root = std::env::current_dir()?;
    if let Some(targets) = deploy_targets(&root)? {
        let Some(target) = targets.first() else {
            return Err(crate::error::no_deploy_targets());
        };
        return Err(target_deploy_unavailable(target));
    }
    Err(provider_deploy_unavailable(provider_config()?))
}

/// CI core has no native local deploy flow yet; never preview a vendor CLI.
/// (CI core chưa có deploy local native; không preview lệnh CLI vendor.)
pub async fn dev(_dry_run: bool) -> Result<()> {
    Err(provider_deploy_unavailable(provider_config()?))
}

#[cfg(test)]
#[path = "test/cicd.rs"]
mod tests;
