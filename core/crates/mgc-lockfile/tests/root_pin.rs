use mgc_lockfile::{
    EcosystemTag, Lockfile, format_root_pin, parse_root_pin, update_owner_root_pins,
};

#[test]
fn qualified_roots_roundtrip_ecosystems_with_hyphenated_names() {
    for ecosystem in [
        EcosystemTag::Web,
        EcosystemTag::Dart,
        EcosystemTag::CloudModule,
        EcosystemTag::NuGet,
    ] {
        let encoded = format_root_pin(ecosystem, "@scope/pkg@1.2.3");
        let parsed = parse_root_pin(&encoded);
        assert_eq!(parsed.ecosystem, Some(ecosystem));
        assert_eq!(parsed.package_id, "@scope/pkg@1.2.3");
    }
}

#[test]
fn legacy_roots_remain_unqualified() {
    let parsed = parse_root_pin("left-pad@1.3.0");
    assert_eq!(parsed.ecosystem, None);
    assert_eq!(parsed.package_id, "left-pad@1.3.0");
}

#[test]
fn updating_one_owner_ecosystem_preserves_sibling_roots() {
    let mut lock = Lockfile::new();
    lock.root_dependencies_by_owner.insert(
        "app".to_string(),
        vec![
            "left-pad@1.3.0".to_string(),
            format_root_pin(EcosystemTag::Swift, "swift-nio@2.0.0"),
        ],
    );

    update_owner_root_pins(
        &mut lock,
        "app",
        EcosystemTag::Dart,
        ["http@1.2.0".to_string()],
    );
    update_owner_root_pins(
        &mut lock,
        "app",
        EcosystemTag::Web,
        ["react@19.0.0".to_string()],
    );

    let roots = &lock.root_dependencies_by_owner["app"];
    assert!(roots.contains(&format_root_pin(EcosystemTag::Dart, "http@1.2.0")));
    assert!(roots.contains(&format_root_pin(EcosystemTag::Swift, "swift-nio@2.0.0")));
    assert!(roots.contains(&format_root_pin(EcosystemTag::Web, "react@19.0.0")));
    assert!(!roots.iter().any(|root| root == "left-pad@1.3.0"));
}

#[test]
fn empty_update_removes_only_the_active_ecosystem_roots() {
    let mut lock = Lockfile::new();
    lock.root_dependencies_by_owner.insert(
        "lib".to_string(),
        vec![
            format_root_pin(EcosystemTag::Rust, "serde@1.0.0"),
            format_root_pin(EcosystemTag::Python, "requests@2.0.0"),
        ],
    );

    update_owner_root_pins(&mut lock, "lib", EcosystemTag::Rust, []);

    assert_eq!(
        lock.root_dependencies_by_owner["lib"],
        vec![format_root_pin(EcosystemTag::Python, "requests@2.0.0")]
    );
}

#[test]
fn malformed_reserved_root_is_not_silently_pruned() {
    let malformed = "mgc-root-v1:unknown:pkg@1.0.0";
    let mut lock = Lockfile::new();
    lock.root_dependencies_by_owner
        .insert("web".to_string(), vec![malformed.to_string()]);

    update_owner_root_pins(&mut lock, "web", EcosystemTag::Web, ["next@1.0.0".into()]);

    assert!(lock.root_dependencies_by_owner["web"].contains(&malformed.to_string()));
}
