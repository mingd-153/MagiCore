//! Trust-pending database failure regressions — hồi quy lỗi database của trust pending.

use super::load_db_policies;
use std::time::Duration;

#[test]
fn unreadable_trust_database_is_not_reported_as_empty_policy_set() {
    let temp = tempfile::tempdir().expect("temporary directory must be created");
    let blocker = temp.path().join("not-a-directory");
    std::fs::write(&blocker, "file blocks database parent")
        .expect("database parent blocker must be written");
    let db_path = blocker.join("trust.db");

    let result = load_db_policies(&db_path);

    assert!(
        result.is_err(),
        "an unreadable trust database must fail instead of appearing empty"
    );
}

#[test]
fn readable_trust_database_returns_saved_policies() {
    let temp = tempfile::tempdir().expect("temporary directory must be created");
    let db_path = temp.path().join("trust.db");
    let db = mgc_store::Database::open(&db_path).expect("trust database must open");
    db.upsert_trust_policy("sample-package@1.2.3", "denied")
        .expect("trust policy must be saved");
    drop(db);

    let policies = load_db_policies(&db_path).expect("saved policy must be readable");

    assert_eq!(
        policies.get("sample-package@1.2.3").map(String::as_str),
        Some("denied")
    );
}

#[test]
fn trust_scan_refuses_to_read_during_a_dependency_mutation() {
    let temp = tempfile::tempdir().expect("temporary project must be created");
    let _held =
        mgc_lockfile::project_lock::ProjectWriteLock::acquire(temp.path(), Duration::from_secs(1))
            .expect("first writer lock must be acquired");

    let result = super::acquire_scan_lock(temp.path(), Duration::from_millis(1));

    assert!(
        result.is_err(),
        "trust scan must not race an active dependency mutation"
    );
}

#[test]
fn malformed_file_policy_is_not_reported_as_absent() {
    let temp = tempfile::tempdir().expect("temporary project must be created");
    std::fs::write(temp.path().join("mgc.toml"), "[scripts]\nallow = true\n")
        .expect("malformed script policy must be written");

    let result = super::load_file_policy(temp.path());

    assert!(
        result.is_err(),
        "a malformed committed trust policy must not be treated as absent"
    );
}

#[test]
fn unreadable_node_modules_tree_is_not_reported_as_no_scripted_packages() {
    let temp = tempfile::tempdir().expect("temporary directory must be created");
    let blocker = temp.path().join("not-a-directory");
    std::fs::write(&blocker, "not a directory").expect("blocker must be written");

    let result = super::scan_scripted_packages(&blocker);

    assert!(
        result.is_err(),
        "an unreadable package tree must not look like an empty tree"
    );
}

#[test]
fn malformed_installed_package_manifest_is_not_silently_skipped() {
    let temp = tempfile::tempdir().expect("temporary directory must be created");
    let node_modules = temp.path().join("node_modules");
    let package = node_modules.join("sample-package");
    std::fs::create_dir_all(&package).expect("package directory must be created");
    std::fs::write(package.join("package.json"), "{not-json")
        .expect("malformed package manifest must be written");

    let result = super::scan_scripted_packages(&node_modules);

    assert!(
        result.is_err(),
        "a malformed installed package manifest must not disappear from trust review"
    );
}

#[test]
fn package_json_with_non_object_scripts_is_not_treated_as_hook_free() {
    let temp = tempfile::tempdir().expect("temporary directory must be created");
    let node_modules = temp.path().join("node_modules");
    let package = node_modules.join("sample-package");
    std::fs::create_dir_all(&package).expect("package directory must be created");
    std::fs::write(
        package.join("package.json"),
        r#"{"name":"sample-package","version":"1.0.0","scripts":["postinstall"]}"#,
    )
    .expect("malformed scripts shape must be written");

    let result = super::scan_scripted_packages(&node_modules);

    assert!(
        result.is_err(),
        "a non-object scripts field must not be reported as hook-free"
    );
}

#[test]
fn non_object_package_json_root_is_not_treated_as_hook_free() {
    let temp = tempfile::tempdir().expect("temporary directory must be created");
    let node_modules = temp.path().join("node_modules");
    let package = node_modules.join("sample-package");
    std::fs::create_dir_all(&package).expect("package directory must be created");
    std::fs::write(package.join("package.json"), "[]")
        .expect("non-object manifest root must be written");

    let result = super::scan_scripted_packages(&node_modules);

    assert!(
        result.is_err(),
        "a non-object package manifest must not be reported as hook-free"
    );
}

#[test]
fn oversized_package_json_is_rejected_before_full_parse() {
    let temp = tempfile::tempdir().expect("temporary directory must be created");
    let manifest = temp.path().join("package.json");
    let file = std::fs::File::create(&manifest).expect("manifest must be created");
    file.set_len(super::MAX_TRUST_PACKAGE_JSON_BYTES + 1)
        .expect("oversized sparse manifest must be created");

    let result = super::read_bounded_package_json(&manifest);

    assert!(result.is_err(), "oversized manifests must be refused");
}

#[test]
#[cfg(unix)]
fn package_json_symlink_is_not_followed() {
    let temp = tempfile::tempdir().expect("temporary directory must be created");
    let target = temp.path().join("outside.json");
    let link = temp.path().join("package.json");
    std::fs::write(&target, r#"{"scripts":{"install":"evil"}}"#)
        .expect("target manifest must be written");
    std::os::unix::fs::symlink(&target, &link).expect("manifest symlink must be created");

    let result = super::read_bounded_package_json(&link);

    assert!(result.is_err(), "symlinked manifests must be refused");
}

#[test]
fn package_json_directory_is_not_treated_as_missing() {
    let temp = tempfile::tempdir().expect("temporary directory must be created");
    let manifest = temp.path().join("package.json");
    std::fs::create_dir(&manifest).expect("manifest directory must be created");

    let result = super::read_bounded_package_json(&manifest);

    assert!(result.is_err(), "non-regular manifests must be refused");
}

#[test]
fn recursive_scan_finds_scoped_and_nested_lifecycle_packages() {
    let temp = tempfile::tempdir().expect("temporary directory must be created");
    let node_modules = temp.path().join("node_modules");
    let scoped_package = node_modules.join("@scope/package");
    let nested_package = scoped_package.join("node_modules/child-package");
    std::fs::create_dir_all(&nested_package).expect("package tree must be created");
    std::fs::write(
        scoped_package.join("package.json"),
        r#"{"name":"@scope/package","version":"1.0.0","scripts":{"install":"node setup.js"}}"#,
    )
    .expect("scoped package manifest must be written");
    std::fs::write(
        nested_package.join("package.json"),
        r#"{"name":"child-package","version":"2.0.0","scripts":{"postinstall":"node setup.js"}}"#,
    )
    .expect("nested package manifest must be written");

    let scripted = super::scan_scripted_packages(&node_modules)
        .expect("complete package tree must be scanned");

    assert!(scripted.contains(&("@scope/package".to_string(), "1.0.0".to_string())));
    assert!(scripted.contains(&("child-package".to_string(), "2.0.0".to_string())));
}
