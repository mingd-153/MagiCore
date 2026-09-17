//! v4 migration tests (design §16.4/A5): identity split, edge
//! resolution honesty, peer loss declaration, unknown sources, empty
//! integrity preservation, full roundtrip.
//! (Test migrate v3→v4: tách identity, trung thực cạnh, khai mất peer,
//! nguồn unknown, giữ integrity rỗng, roundtrip đầy đủ.)

use mgc_lockfile::canonical::{parse_v4_document, payload_digest, write_v4_document};
use mgc_lockfile::{Lockfile, Package, migrate_v3_to_v4};

fn package(name: &str, version: &str, registry: Option<&str>) -> Package {
    Package {
        name: name.to_string(),
        version: version.to_string(),
        resolved: format!("https://example.invalid/{name}-{version}.tgz"),
        integrity: format!("blake3-{name}-{version}"),
        dependencies: Vec::new(),
        registry: registry.map(str::to_string),
        ..Default::default()
    }
}

fn v3_lock(packages: Vec<Package>) -> Lockfile {
    let mut lock = Lockfile::new();
    lock.version = "3".to_string();
    for package in packages {
        lock.add_package(package);
    }
    lock
}

#[test]
fn same_name_version_two_registries_becomes_two_keys() {
    // The v3 merge lie, undone: identical (name, version) from two
    // registries are two instances with different source_ids.
    let mut left = package("lodash", "4.17.21", None);
    left.registry = Some("npm://registry.npmjs.org".to_string());
    let mut right = package("lodash", "4.17.21", None);
    right.registry = Some("npm://mirror.corp".to_string());
    let (v4, _) = migrate_v3_to_v4(v3_lock(vec![left, right])).unwrap();
    assert_eq!(v4.packages.len(), 2);
    assert_ne!(
        v4.packages[0].key.source_id, v4.packages[1].key.source_id,
        "same name+version from two registries must not merge"
    );
    assert_eq!(v4.sources.len(), 2);
}

#[test]
fn unique_name_dep_becomes_transitive_edge() {
    let mut app = package("app", "1.0.0", None);
    app.registry = Some("npm://registry.npmjs.org".to_string());
    app.dependencies = vec!["lodash".to_string()];
    let mut lodash = package("lodash", "4.17.21", None);
    lodash.registry = Some("npm://registry.npmjs.org".to_string());
    let (v4, warnings) = migrate_v3_to_v4(v3_lock(vec![app, lodash])).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let app_entry = v4.packages.iter().find(|p| p.key.name == "app").unwrap();
    assert_eq!(app_entry.edges.len(), 1);
    assert_eq!(app_entry.edges[0].target_key.name, "lodash");
    assert_eq!(app_entry.edges[0].range, "*");
}

#[test]
fn missing_and_ambiguous_deps_drop_with_warnings() {
    let mut ghost = package("ghost", "1.0.0", None);
    ghost.registry = Some("npm://registry.npmjs.org".to_string());
    ghost.dependencies = vec!["missing-dep".to_string(), "dup".to_string()];
    // `integrity` is fabricated-but-labeled here only to keep THIS test
    // focused on edges; the empty-integrity warning is covered below.
    let mut dup_a = package("dup", "1.0.0", None);
    dup_a.registry = Some("npm://registry.npmjs.org".to_string());
    let mut dup_b = package("dup", "2.0.0", None);
    dup_b.registry = Some("npm://registry.npmjs.org".to_string());
    let (v4, warnings) = migrate_v3_to_v4(v3_lock(vec![ghost, dup_a, dup_b])).unwrap();
    let ghost_entry = v4.packages.iter().find(|p| p.key.name == "ghost").unwrap();
    assert!(ghost_entry.edges.is_empty());
    assert!(
        warnings.iter().any(|w| w.contains("missing-dep")),
        "{warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| w.contains("ambiguous")),
        "{warnings:?}"
    );
}

#[test]
fn peers_drop_with_warning_instead_of_fake_digest() {
    let mut with_peers = package("button", "1.0.0", None);
    with_peers.registry = Some("npm://registry.npmjs.org".to_string());
    with_peers.peers = Some(vec!["react".to_string()]);
    let (v4, warnings) = migrate_v3_to_v4(v3_lock(vec![with_peers])).unwrap();
    assert!(v4.packages[0].key.variant.peer_context.is_none());
    assert!(warnings.iter().any(|w| w.contains("peer")), "{warnings:?}");
}

#[test]
fn registry_less_pins_share_labeled_unknown_source() {
    let (v4, warnings) = migrate_v3_to_v4(v3_lock(vec![package("x", "1.0.0", None)])).unwrap();
    assert_eq!(v4.packages[0].key.source_id, "unknown");
    assert!(v4.sources.iter().any(|s| s.id == "unknown"));
    assert!(
        warnings.iter().any(|w| w.contains("unknown")),
        "{warnings:?}"
    );
}

#[test]
fn empty_integrity_is_preserved_never_invented() {
    let mut bare = package("bare", "1.0.0", None);
    bare.registry = Some("npm://registry.npmjs.org".to_string());
    bare.integrity = String::new();
    let (v4, warnings) = migrate_v3_to_v4(v3_lock(vec![bare])).unwrap();
    assert!(v4.packages[0].key.name == "bare");
    assert!(
        warnings.iter().any(|w| w.contains("no integrity")),
        "{warnings:?}"
    );
}

#[test]
fn migrated_document_roundtrips_with_stable_digest() {
    let mut app = package("app", "1.0.0", None);
    app.registry = Some("npm://registry.npmjs.org".to_string());
    app.dependencies = vec!["lodash".to_string()];
    let mut lodash = package("lodash", "4.17.21", None);
    lodash.registry = Some("npm://registry.npmjs.org".to_string());
    let (mut v4, _) = migrate_v3_to_v4(v3_lock(vec![app, lodash])).unwrap();
    v4.metadata.generated_at = "2026-09-17T00:00:00Z".to_string();
    v4.metadata.generator = "mgc/1.2.0-test".to_string();
    v4.metadata.lockfile_hash = payload_digest(&v4.payload());
    let text = write_v4_document(&v4).unwrap();
    let back = parse_v4_document(&text).unwrap();
    assert_eq!(back, v4);
    assert_eq!(payload_digest(&back.payload()), v4.metadata.lockfile_hash);
}
