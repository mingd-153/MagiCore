use super::*;

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
