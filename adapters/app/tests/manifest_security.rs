#![allow(clippy::unwrap_used)]

#[cfg(unix)]
#[test]
fn flutter_manifest_read_and_write_refuse_symlink_targets() {
    use mgc_app_adapter::manifest::flutter::{parse_pubspec, write_pubspec};
    use mgc_types::{Ecosystem, Manifest};
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let external = outside.path().join("outside.yaml");
    std::fs::write(&external, "name: outside\ndependencies: {}\n").unwrap();
    symlink(&external, project.path().join("pubspec.yaml")).unwrap();

    assert!(parse_pubspec(project.path()).is_err());
    assert!(write_pubspec(project.path(), &Manifest::new("demo", Ecosystem::App)).is_err());
    assert_eq!(
        std::fs::read_to_string(external).unwrap(),
        "name: outside\ndependencies: {}\n"
    );
}
