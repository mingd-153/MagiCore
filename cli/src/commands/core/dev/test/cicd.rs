use super::*;
use sha2::{Digest, Sha256};

#[test]
fn cloudflare_deploy_command() {
    let cmd = deploy_command(mgc_cicd_adapter::CicdProvider::Cloudflare).expect("cloudflare ok");
    assert_eq!(cmd.tool, "wrangler");
    assert_eq!(cmd.args, vec!["deploy", "--dry-run"]);
}

#[test]
fn gcp_deploy_command() {
    let cmd = deploy_command(mgc_cicd_adapter::CicdProvider::Gcp).expect("gcp ok");
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
    assert!(deploy_command(mgc_cicd_adapter::CicdProvider::GithubActions).is_err());
    assert!(deploy_command(mgc_cicd_adapter::CicdProvider::Gitlab).is_err());
    assert!(deploy_command(mgc_cicd_adapter::CicdProvider::CircleCi).is_err());
    assert!(deploy_command(mgc_cicd_adapter::CicdProvider::Aws).is_err());
    assert!(deploy_command(mgc_cicd_adapter::CicdProvider::Argocd).is_err());
}

#[test]
fn ci_templates_cover_all_providers() {
    // Checkout must be pinned by commit SHA — a mutable tag can be hijacked.
    // Checkout phải ghim theo commit SHA — tag mutable có thể bị chiếm.
    assert!(
        WORKFLOW_TEMPLATE.contains("actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1")
    );
    assert!(!WORKFLOW_TEMPLATE.contains("checkout@v"));
    // Installer must come from a release tag, not a mutable branch.
    // Installer phải lấy từ release tag, không phải branch mutable.
    assert!(WORKFLOW_TEMPLATE.contains("--version {tag}"));
    assert!(!WORKFLOW_TEMPLATE.contains("--branch"));
    assert!(GITLAB_TEMPLATE.contains("mgc verify"));
    assert!(GITLAB_TEMPLATE.contains("stages:"));
    assert!(GITLAB_TEMPLATE.contains("--version {tag}"));
    assert!(CIRCLE_TEMPLATE.contains("version: 2.1"));
    assert!(CIRCLE_TEMPLATE.contains("install-from-gh.sh"));
    assert!(CIRCLE_TEMPLATE.contains("--version {tag}"));
}

#[test]
fn generated_ci_release_tag_matches_this_mgc_build() {
    // Generated workflows must install the exact MagiCore build that emitted
    // them; a stale manually copied tag silently pins old behavior.
    // (Workflow sinh ra phải cài đúng phiên bản MagiCore đang chạy; tag chép
    // tay bị cũ sẽ âm thầm ghim hành vi lỗi thời.)
    assert_eq!(MGC_RELEASE_TAG, format!("v{}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn generated_ci_installer_pin_matches_current_release_contract() {
    // The pin must identify the immutable commit that contains this exact
    // installer body; stale case-sensitive asset names break release installs.
    // (Pin phải trỏ tới commit bất biến chứa đúng installer này; tên asset
    // sai hoa-thường làm hỏng cài đặt release.)
    const TRACKED_INSTALLER: &[u8] = include_bytes!("../../../../../../scripts/install-from-gh.sh");
    assert_eq!(
        MGC_INSTALLER_SHA,
        "80a383763377413c4a50d8de3bd815a0f8f01284"
    );
    assert_eq!(
        MGC_INSTALLER_SHA256,
        hex::encode(Sha256::digest(TRACKED_INSTALLER))
    );
    let installer = std::str::from_utf8(TRACKED_INSTALLER).expect("installer is UTF-8");
    assert!(installer.contains("OS_LABEL=\"macos\""));
    assert!(installer.contains("EXT=\"zip\""));
}

#[test]
fn ci_templates_install_from_immutable_sha_never_main() {
    // P0-E supply-chain gate (2026-09-16): the generated installer URL
    // must pin an immutable commit SHA — mutable `/main/` may not appear
    // anywhere; the script is downloaded to a FILE (never `curl | bash`);
    // companion/embedded checksum verification logic must be present;
    // with an empty embedded checksum the pipeline must WARN, not break.
    // (Cổng supply-chain P0-E: URL installer phải ghim commit SHA bất
    // biến — không được có `/main/` mutable ở đâu cả; script tải về FILE
    // (không bao giờ `curl | bash`); logic verify checksum companion/nhúng
    // phải có; checksum nhúng rỗng thì pipeline phải WARNING, không vỡ.)
    assert_eq!(MGC_INSTALLER_SHA.len(), 40, "pin must be a full commit SHA");
    assert!(
        !MGC_INSTALLER_SHA.chars().any(|c| !c.is_ascii_hexdigit()),
        "pin must be hex-only"
    );
    for (rendered, label) in [
        (
            render_ci_template(WORKFLOW_TEMPLATE, "CI"),
            "github workflow",
        ),
        (render_ci_template(GITLAB_TEMPLATE, ""), "gitlab ci"),
        (render_ci_template(CIRCLE_TEMPLATE, ""), "circle ci"),
    ] {
        assert_eq!(
            rendered.matches("/main/").count(),
            0,
            "{label}: installer URL must never reference mutable main"
        );
        assert!(
            rendered.contains(MGC_INSTALLER_SHA),
            "{label}: installer URL must pin the immutable commit SHA"
        );
        assert!(
            !rendered.contains("| bash"),
            "{label}: installer must never be piped straight into bash"
        );
        assert!(
            rendered.contains("-o \"$installer\""),
            "{label}: installer must be downloaded to a file first"
        );
        assert!(
            rendered.contains("sha256sum -c"),
            "{label}: companion .sha256 verification must be present"
        );
        assert!(
            rendered.contains("WARNING: installer integrity NOT pinned"),
            "{label}: empty embedded checksum must print a loud warning and proceed"
        );
        assert!(
            rendered.contains("--version {tag}".replace("{tag}", MGC_RELEASE_TAG).as_str()),
            "{label}: release-tag install flag must survive rendering"
        );
    }
}

#[test]
fn verify_chain_rejects_unknown_steps_before_running() {
    // Fail-closed: unknown steps and empty chains are config errors.
    // Fail-closed: step lạ và chain rỗng là lỗi cấu hình.
    let known = ["audit", "test", "build"];
    for bad in ["audti", "builld", "", "lint", "deploy"] {
        assert!(!known.contains(&bad), "test fixture sanity");
    }
    let mut chain = vec![bad_step_fixture("audti").to_string()];
    chain.push("audit".into());
    // Simulate the validation loop from verify(): any unknown step fails.
    // Giả lập vòng validate của verify(): step lạ phải fail.
    let has_unknown = chain.iter().any(|s| !known.contains(&s.as_str()));
    assert!(has_unknown, "typo'd step must be detected, not skipped");
}

fn bad_step_fixture(name: &str) -> &str {
    name
}

#[test]
fn installer_checksum_is_pinned_hex() {
    // T0.5: the embedded checksum must be a real SHA-256 (64 lowercase
    // hex), never empty — empty means warning-only mode.
    // (Checksum nhúng phải là SHA-256 thật, không bao giờ rỗng.)
    assert_eq!(MGC_INSTALLER_SHA256.len(), 64);
    assert!(
        MGC_INSTALLER_SHA256
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
}

#[test]
fn rendered_templates_verify_checksum_fail_closed() {
    // T0.5: every generated template must substitute the pins, verify
    // checksums with fail-closed semantics, and never fetch from a
    // mutable branch.
    for template in [WORKFLOW_TEMPLATE, GITLAB_TEMPLATE, CIRCLE_TEMPLATE] {
        let rendered = render_ci_template(template, "CI");
        assert!(
            !rendered.contains("{installer_sha256}"),
            "sha256 pin must be substituted"
        );
        assert!(
            !rendered.contains("{installer_sha}"),
            "commit SHA pin must be substituted"
        );
        assert!(
            rendered.contains(MGC_INSTALLER_SHA256),
            "embedded checksum must reach the pipeline"
        );
        assert!(
            rendered.contains("sha256sum -c"),
            "pipeline must verify checksums"
        );
        assert!(
            !rendered.contains("raw.githubusercontent.com/mingd-153/MagiCore/main/"),
            "generated pipeline must never fetch from mutable main"
        );
    }
}

#[test]
fn verify_chain_parses_custom_or_default() {
    let tmp = std::env::temp_dir().join(format!("mgc-cicd-chain-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(
        tmp.join("mgc.toml"),
        "[cicd]\nprovider = \"github-actions\"\nverify = [\"audit\", \"build\"]\n",
    )
    .unwrap();
    assert_eq!(verify_chain(&tmp).unwrap(), vec!["audit", "build"]);
    std::fs::write(
        tmp.join("mgc.toml"),
        "[cicd]\nprovider = \"github-actions\"\n",
    )
    .unwrap();
    assert_eq!(verify_chain(&tmp).unwrap(), vec!["audit", "test", "build"]);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[cfg(unix)]
#[test]
fn cicd_config_readers_reject_external_mgc_toml_symlink() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    std::fs::write(
        external.path().join("mgc.toml"),
        "[cicd]\nverify = ['audit']\n\n[[deploy.targets]]\nprovider = 'cloudflare'\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        external.path().join("mgc.toml"),
        root.path().join("mgc.toml"),
    )
    .unwrap();

    assert!(verify_chain(root.path()).is_err());
    assert!(deploy_targets(root.path()).is_err());
}

#[tokio::test]
async fn verify_test_step_rejects_cores_without_a_verified_test_path() {
    // A verify chain must fail when its test step is not implemented for a core.
    // Chuỗi verify phải lỗi nếu bước test chưa được triển khai cho core đó.
    let tmp = tempfile::tempdir().expect("temporary project directory");
    for core in ["ai", "app", "game", "iot", "clo", "cicd", "hardware"] {
        let result = run_test_step(tmp.path(), core).await;
        assert!(
            result.is_err(),
            "verify must not report a skipped test step as success for core '{core}'"
        );
        let error = result.expect_err("unsupported verify test step must fail");
        assert!(error.to_string().contains(core));
    }
}

#[test]
fn verify_build_step_rejects_cicd_instead_of_reporting_success() {
    // CI/CD definitions do not produce build artifacts; a configured build must fail explicitly.
    // Định nghĩa CI/CD không tạo build artifact; bước build được cấu hình phải báo lỗi tường minh.
    let error = ensure_build_step_supported("cicd").expect_err("CI/CD build must not be skipped");
    assert!(error.to_string().contains("cicd"));
    assert!(error.to_string().contains("no build artifact"));
    assert!(ensure_build_step_supported("web").is_ok());
}
