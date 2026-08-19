// Workspace shared lock seeding: existing_versions_from union first-wins.
use mg_lockfile::{existing_versions_from, write_lockfile, LockPackage, Lockfile};
use std::collections::HashMap;

fn lock(pkgs: Vec<(&str, &str)>) -> Lockfile {
    let mut lockfile = Lockfile::new("web", "project");
    lockfile.packages = pkgs
        .into_iter()
        .map(|(name, version)| LockPackage {
            name: name.to_string(),
            version: version.to_string(),
            integrity: None,
            direct: false,
            dev: false,
            dependencies: vec![],
            peer_deps: vec![],
        })
        .collect();
    lockfile
}

#[test]
fn union_first_wins_prefers_root() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("ws-root");
    let app1 = base.path().join("apps/app1");
    let app2 = base.path().join("apps/app2");
    for dir in [&root, &app1, &app2] {
        std::fs::create_dir_all(dir).unwrap();
    }

    write_lockfile(
        &root,
        &lock(vec![("react", "18.2.0"), ("lodash", "4.17.21")]),
    )
    .unwrap();
    write_lockfile(&app1, &lock(vec![("react", "18.2.0"), ("axios", "1.7.0")])).unwrap();
    write_lockfile(&app2, &lock(vec![("react", "18.3.1")])).unwrap();

    let versions = existing_versions_from(&[&root, &app1, &app2]).unwrap();
    assert_eq!(
        versions.get("react"),
        Some(&"18.2.0".to_string()),
        "root lock first wins"
    );
    assert_eq!(versions.get("lodash"), Some(&"4.17.21".to_string()));
    assert_eq!(versions.get("axios"), Some(&"1.7.0".to_string()));
    assert_eq!(versions.len(), 3);
}

#[test]
fn missing_roots_are_ignored() {
    let base = tempfile::tempdir().unwrap();
    let empty = base.path().join("no-lock-here");
    std::fs::create_dir_all(&empty).unwrap();

    let versions = existing_versions_from(&[&empty]).unwrap();
    assert!(versions.is_empty());

    let mut app = base.path().join("app");
    std::fs::create_dir_all(&app).unwrap();
    write_lockfile(&app, &lock(vec![("is-number", "7.0.0")])).unwrap();
    let versions = existing_versions_from(&[&empty, &app]).unwrap();
    let expected: HashMap<String, String> =
        HashMap::from([("is-number".to_string(), "7.0.0".to_string())]);
    assert_eq!(versions, expected);
}
