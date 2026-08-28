use super::*;

#[test]
fn dev_command_maps_types() {
    let (cmd, args) = dev_command("terraform").unwrap();
    assert_eq!(cmd, "terraform");
    assert_eq!(args, vec!["plan"]);
    let (cmd, args) = dev_command("cdk").unwrap();
    assert_eq!(cmd, "cdk");
    assert_eq!(args, vec!["synth"]);
    let (cmd, args) = dev_command("pulumi").unwrap();
    assert_eq!(cmd, "pulumi");
    assert_eq!(args, vec!["preview"]);
    assert!(dev_command("unknown").is_err());
}

#[test]
fn bin_resolution_for_npm_tools() {
    let dir = std::env::temp_dir().join(format!("mgc-clo-bin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("node_modules").join(".bin")).unwrap();
    std::fs::write(
        dir.join("node_modules").join(".bin").join("cdk"),
        "#!/bin/sh\n",
    )
    .unwrap();
    assert!(bin_resolved_path(&dir, "cdk").unwrap().is_file());
    assert!(bin_resolved_path(&dir, "pulumi").is_none());
    assert!(bin_resolved_path(&dir, "terraform").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn deploy_command_maps_types() {
    let (cmd, args) = deploy_command("terraform").unwrap();
    assert_eq!(cmd, "terraform");
    assert_eq!(args, vec!["apply"]);
    let (cmd, args) = deploy_command("cdk").unwrap();
    assert_eq!(cmd, "cdk");
    assert_eq!(args, vec!["deploy"]);
    let (cmd, args) = deploy_command("pulumi").unwrap();
    assert_eq!(cmd, "pulumi");
    assert_eq!(args, vec!["up"]);
}
