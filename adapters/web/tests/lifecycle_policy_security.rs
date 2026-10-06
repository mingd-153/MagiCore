//! Lifecycle policy security tests — exercise resolved package identity and fail-closed manifest parsing.
//! Test bảo mật lifecycle — kiểm tra định danh package đã resolve và parse manifest theo fail-closed.

use mgc_types::PackageId;
use mgc_web_adapter::install::script_policy::{
    decide_lifecycle_scripts, manifest_has_lifecycle_scripts,
};
use std::collections::HashMap;

fn temp_package(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mgc-lifecycle-policy-{tag}-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).expect("create temporary package directory");
    dir
}

#[test]
fn deny_uses_resolved_identity_not_self_declared_manifest_identity() {
    let resolved = PackageId::parse("resolved-package@1.2.3").expect("parse resolved package id");
    let trust = HashMap::from([("resolved-package@1.2.3".to_owned(), "denied".to_owned())]);
    let package = temp_package("identity");
    std::fs::write(
        package.join("package.json"),
        r#"{"name":"innocent-alias","version":"9.9.9","scripts":{"install":"node setup.js"}}"#,
    )
    .expect("write package manifest");

    let has_scripts =
        manifest_has_lifecycle_scripts(&package).expect("parse package lifecycle metadata");
    assert!(has_scripts);
    let verdict = decide_lifecycle_scripts(&resolved, None, &trust, true);
    assert!(
        matches!(verdict, mgc_config::project::ScriptVerdict::Deny(_)),
        "resolved package deny must win even when package.json claims a different identity"
    );
}

#[test]
fn malformed_package_manifest_fails_closed() {
    let package = temp_package("malformed");
    std::fs::write(package.join("package.json"), b"{not-json")
        .expect("write malformed package manifest");
    assert!(
        manifest_has_lifecycle_scripts(&package).is_err(),
        "malformed package metadata must not be interpreted as no scripts"
    );
}

#[test]
fn non_string_lifecycle_script_fails_closed() {
    let package = temp_package("wrong-type");
    std::fs::write(
        package.join("package.json"),
        r#"{"name":"pkg","version":"1.0.0","scripts":{"postinstall":false}}"#,
    )
    .expect("write package manifest with non-string hook");
    assert!(
        manifest_has_lifecycle_scripts(&package).is_err(),
        "a malformed lifecycle script value must fail instead of being skipped"
    );
}

#[test]
fn manifest_without_lifecycle_hooks_is_not_scheduled() {
    let package = temp_package("no-hooks");
    std::fs::write(
        package.join("package.json"),
        r#"{"name":"pkg","version":"1.0.0","scripts":{"test":"node test.js"}}"#,
    )
    .expect("write package manifest without lifecycle hooks");
    assert!(
        !manifest_has_lifecycle_scripts(&package)
            .expect("parse package manifest without lifecycle hooks")
    );
}

#[test]
fn package_without_manifest_has_no_declared_lifecycle_hooks() {
    let package = temp_package("missing-manifest");
    assert!(!manifest_has_lifecycle_scripts(&package).expect("handle package without manifest"));
}

#[cfg(unix)]
#[test]
fn symlinked_package_manifest_fails_closed() {
    let package = temp_package("symlink");
    let external = temp_package("external").join("package.json");
    std::fs::write(&external, r#"{"scripts":{"install":"node setup.js"}}"#)
        .expect("write external package manifest");
    std::os::unix::fs::symlink(external, package.join("package.json"))
        .expect("create package manifest symlink");
    assert!(
        manifest_has_lifecycle_scripts(&package).is_err(),
        "package metadata symlinks must not be followed"
    );
}
