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
    assert!(deploy_command(mg_cicd_adapter::CicdProvider::Gitlab).is_err());
    assert!(deploy_command(mg_cicd_adapter::CicdProvider::CircleCi).is_err());
    assert!(deploy_command(mg_cicd_adapter::CicdProvider::Aws).is_err());
    assert!(deploy_command(mg_cicd_adapter::CicdProvider::Argocd).is_err());
}

#[test]
fn ci_templates_cover_all_providers() {
    assert!(WORKFLOW_TEMPLATE.contains("actions/checkout@v4"));
    assert!(GITLAB_TEMPLATE.contains("mg verify"));
    assert!(GITLAB_TEMPLATE.contains("stages:"));
    assert!(CIRCLE_TEMPLATE.contains("version: 2.1"));
    assert!(CIRCLE_TEMPLATE.contains("cimg/rust"));
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
