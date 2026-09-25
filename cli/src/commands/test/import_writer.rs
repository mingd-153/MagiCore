use super::{merge_imported_lock, read_optional_regular_file};
use mgc_lockfile::{EcosystemTag, Lockfile, Package};

fn package(owner: Option<&str>, name: &str, version: &str) -> Package {
    Package {
        owner_core: owner.map(str::to_string),
        name: name.to_string(),
        version: version.to_string(),
        ecosystem: EcosystemTag::Other,
        ..Default::default()
    }
}

#[test]
fn import_replaces_only_active_core_and_preserves_sibling_lock_entries() {
    let mut existing = Lockfile::new();
    existing.packages = vec![
        package(Some("ai"), "shared", "2.0.0"),
        package(Some("web"), "old-web", "1.0.0"),
        package(None, "ambiguous-legacy", "1.0.0"),
    ];
    let mut imported = Lockfile::new();
    imported.packages = vec![package(None, "new-web", "3.0.0")];

    let merged = merge_imported_lock(imported, Some(existing), "web");

    assert!(merged.packages.iter().any(|package| {
        package.owner_core.as_deref() == Some("ai")
            && package.name == "shared"
            && package.version == "2.0.0"
    }));
    assert!(
        merged
            .packages
            .iter()
            .any(|package| { package.owner_core.is_none() && package.name == "ambiguous-legacy" })
    );
    assert!(
        !merged
            .packages
            .iter()
            .any(|package| package.name == "old-web")
    );
    assert!(merged.packages.iter().any(|package| {
        package.owner_core.as_deref() == Some("web")
            && package.ecosystem == EcosystemTag::Web
            && package.name == "new-web"
    }));
}

#[test]
fn import_adopts_ownerless_legacy_entries_only_without_competing_core() {
    let mut existing = Lockfile::new();
    existing.packages.push(package(None, "old", "1.0.0"));
    let mut imported = Lockfile::new();
    imported.packages.push(package(None, "new", "2.0.0"));

    let merged = merge_imported_lock(imported, Some(existing), "web");

    assert_eq!(merged.packages.len(), 1);
    assert_eq!(merged.packages[0].owner_core.as_deref(), Some("web"));
    assert_eq!(merged.packages[0].ecosystem, EcosystemTag::Web);
    assert_eq!(merged.packages[0].name, "new");
}

#[test]
fn import_artifact_reader_distinguishes_missing_from_invalid_file_types() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.lock");
    assert_eq!(read_optional_regular_file(&missing).unwrap(), None);

    let regular = dir.path().join("mgc.lock");
    std::fs::write(&regular, b"lock-bytes").unwrap();
    assert_eq!(
        read_optional_regular_file(&regular).unwrap(),
        Some(b"lock-bytes".to_vec())
    );

    let directory = dir.path().join("signature-directory");
    std::fs::create_dir(&directory).unwrap();
    assert!(read_optional_regular_file(&directory).is_err());
}

#[cfg(unix)]
#[test]
fn import_artifact_reader_refuses_symlinks() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target");
    let link = dir.path().join("mgc.lock");
    std::fs::write(&target, b"must-not-follow").unwrap();
    symlink(&target, &link).unwrap();
    assert!(read_optional_regular_file(&link).is_err());
    assert_eq!(std::fs::read(target).unwrap(), b"must-not-follow");
}
