//! Project-context identity regressions — hồi quy nhận diện core của context.

use super::*;

#[test]
fn project_context_rejects_marker_config_conflict_even_with_core_override() {
    let project = tempfile::tempdir().expect("temporary project directory must be created");
    mgc_config::project::ProjectConfig::new("identity-route", "web")
        .save(project.path())
        .expect("web project identity must be initialized");
    std::fs::write(project.path().join(".mgc.core"), "ai\n")
        .expect("conflicting marker must be written");

    let root = project.path().to_path_buf();
    for core_override in [None, Some("ai")] {
        let result = ProjectContext::resolve_config(&root, Some(&root), core_override);
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("conflicting project identities must be rejected"),
        };
        assert!(
            error.to_string().contains("conflicts with mgc.toml"),
            "identity conflict should be explicit, got: {error}"
        );
    }
}

#[test]
fn loading_legacy_config_claims_the_existing_core_identity() {
    let project = tempfile::tempdir().expect("temporary project directory must be created");
    std::fs::write(
        project.path().join("mgc.toml"),
        "name = \"legacy-web\"\nversion = \"0.1.0\"\necosystem = \"web\"\n",
    )
    .expect("legacy project config must be written");

    let root = project.path().to_path_buf();
    let (_, config) = ProjectContext::resolve_config(&root, Some(&root), None)
        .expect("existing project config should load and claim its recorded core");

    assert_eq!(config.ecosystem, "web");
    assert_eq!(
        mgc_config::project::ProjectConfig::read_core_marker(&root)
            .expect("claimed marker must be readable")
            .as_deref(),
        Some("web"),
        "loading an existing config without a marker must persist its core owner"
    );
}

#[test]
fn matching_core_override_also_claims_legacy_config_identity() {
    let project = tempfile::tempdir().expect("temporary project directory must be created");
    std::fs::write(
        project.path().join("mgc.toml"),
        "name = \"legacy-web-override\"\nversion = \"0.1.0\"\necosystem = \"web\"\n",
    )
    .expect("legacy project config must be written");

    let root = project.path().to_path_buf();
    let (_, config) = ProjectContext::resolve_config(&root, Some(&root), Some("web"))
        .expect("matching core override should preserve and claim the configured core");

    assert_eq!(config.ecosystem, "web");
    assert_eq!(
        mgc_config::project::ProjectConfig::read_core_marker(&root)
            .expect("claimed marker must be readable")
            .as_deref(),
        Some("web"),
        "a matching explicit override must not bypass identity-marker creation"
    );
}

#[cfg(unix)]
#[test]
fn read_only_legacy_project_still_loads_from_its_config_identity() {
    let project = tempfile::tempdir().expect("temporary project directory must be created");
    std::fs::write(
        project.path().join("mgc.toml"),
        "name = \"readonly-web\"\nversion = \"0.1.0\"\necosystem = \"web\"\n",
    )
    .expect("legacy project config must be written");
    let original_permissions = std::fs::metadata(project.path())
        .expect("project directory metadata must be readable")
        .permissions();
    let mut read_only_permissions = original_permissions.clone();
    read_only_permissions.set_readonly(true);
    std::fs::set_permissions(project.path(), read_only_permissions)
        .expect("project directory must become read-only");

    let root = project.path().to_path_buf();
    let result = ProjectContext::resolve_config(&root, Some(&root), None);
    std::fs::set_permissions(project.path(), original_permissions)
        .expect("project directory must be writable for cleanup");

    let (_, config) = result.expect("read-only project config remains a valid identity source");
    assert_eq!(config.ecosystem, "web");
    assert!(
        mgc_config::project::ProjectConfig::read_core_marker(&root)
            .expect("missing optional marker must be readable")
            .is_none(),
        "read-only project must not fabricate a persisted marker"
    );
}

#[test]
fn explicit_core_override_cannot_reassign_an_owned_project() {
    let project = tempfile::tempdir().expect("temporary project directory must be created");
    mgc_config::project::ProjectConfig::new("identity-override", "web")
        .save(project.path())
        .expect("web project identity must be initialized");

    let root = project.path().to_path_buf();
    let result = ProjectContext::resolve_config(&root, Some(&root), Some("ai"));
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("explicit core override must not reassign an owned project"),
    };
    assert!(
        error.to_string().contains("core override 'ai' conflicts"),
        "identity conflict should be explicit, got: {error}"
    );

    std::fs::remove_file(root.join(".mgc.core"))
        .expect("test must exercise config-only project identity");
    let result = ProjectContext::resolve_config(&root, Some(&root), Some("ai"));
    assert!(
        result
            .expect_err("config identity must reject reassignment")
            .to_string()
            .contains("core override 'ai' conflicts"),
        "config-only identity conflict should be explicit"
    );
}

#[test]
fn explicit_core_override_cannot_erase_a_single_detected_signature() {
    let project = tempfile::tempdir().expect("temporary project directory must be created");
    std::fs::write(project.path().join("package.json"), "{}\n")
        .expect("web signature must be written");

    let root = project.path().to_path_buf();
    let result = ProjectContext::resolve_config(&root, Some(&root), Some("ai"));
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("core override must not contradict an unambiguous project signature"),
    };
    assert!(
        error.to_string().contains("core override 'ai' conflicts")
            && error.to_string().contains("'web'"),
        "signature conflict should be explicit, got: {error}"
    );
    assert!(
        !root.join(".mgc.core").exists(),
        "rejected override must not write an identity marker"
    );

    ProjectContext::resolve_config(&root, Some(&root), Some("web"))
        .expect("matching override should claim the detected core");
    assert_eq!(
        mgc_config::project::ProjectConfig::read_core_marker(&root)
            .expect("claimed signature must remain readable")
            .as_deref(),
        Some("web")
    );
}
