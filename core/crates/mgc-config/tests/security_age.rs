use mgc_config::project::load_security_config;

#[test]
fn security_policy_loads_from_a_minimal_project_config() {
    let project = tempfile::tempdir().expect("create temp project");
    std::fs::write(
        project.path().join("mgc.toml"),
        "[security]\nmin_release_age = 48\nhardware = 8\nallow_missing_time = true\n",
    )
    .expect("write project config");

    let security = load_security_config(project.path())
        .expect("load security policy")
        .expect("security policy");
    assert_eq!(security.min_age_for_ecosystem("hardware"), Some(8));
    assert_eq!(security.min_age_for_ecosystem("ai"), Some(48));
    assert_eq!(security.allow_missing_time, Some(true));
}

#[test]
fn security_policy_rejects_malformed_security_values() {
    let project = tempfile::tempdir().expect("create temp project");
    std::fs::write(
        project.path().join("mgc.toml"),
        "[security]\nmin_release_age = \"48\"\n",
    )
    .expect("write malformed project config");

    assert!(load_security_config(project.path()).is_err());
}
