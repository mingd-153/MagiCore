#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! SBOM generation tests — Test tạo SBOM
//! Tests SBOM generation from lockfile with behavior validation — Test tạo SBOM từ lockfile với kiểm tra hành vi

use mgc_sbom::{Bom, Component, ComponentType, SbomFormat, SbomGenerator, SbomOptions};

#[test]
fn test_sbom_format_cyclonedx() {
    let format = SbomFormat::CycloneDx;
    assert_eq!(format, SbomFormat::CycloneDx);
}

#[test]
fn test_component_can_be_created() {
    // Create component — Tạo component
    let component = Component {
        component_type: ComponentType::Library,
        bom_ref: "pkg:npm/test@1.0.0".to_string(),
        name: "test-package".to_string(),
        version: "1.0.0".to_string(),
        purl: Some("pkg:npm/test-package@1.0.0".to_string()),
        licenses: None,
        hashes: None,
    };

    assert_eq!(component.name, "test-package");
    assert_eq!(component.version, "1.0.0");
}

#[test]
fn test_bom_can_be_created() {
    let bom = Bom::new();
    assert_eq!(bom.bom_format, "CycloneDX");
}

#[test]
fn test_sbom_generator_can_be_created() {
    // Create generator with options — Tạo generator với options
    let options = SbomOptions {
        include_dev: false,
        include_licenses: true,
        include_hashes: true,
        format: SbomFormat::CycloneDx,
    };
    let _generator = SbomGenerator::new(options);
    // Generator created successfully — Generator tạo thành công
}

#[test]
fn test_sbom_generate_from_lockfile() {
    use mgc_lockfile::{Lockfile, Package};

    // Create test lockfile — Tạo lockfile test
    let mut lockfile = Lockfile::new();
    lockfile.packages = vec![
        Package {
            name: "lodash".to_string(),
            version: "4.17.21".to_string(),
            resolved: "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz".to_string(),
            integrity: "blake3:abc123".to_string(),
            dependencies: vec![],
            ..Default::default()
        },
        Package {
            name: "axios".to_string(),
            version: "1.0.0".to_string(),
            resolved: "https://registry.npmjs.org/axios/-/axios-1.0.0.tgz".to_string(),
            integrity: "blake3:def456".to_string(),
            dependencies: vec!["lodash@4.17.21".to_string()],
            ..Default::default()
        },
    ];

    let generator = SbomGenerator::new(SbomOptions {
        include_dev: false,
        include_licenses: false,
        include_hashes: true,
        format: SbomFormat::CycloneDx,
    });

    // Generate SBOM — Tạo SBOM
    let bom = generator.generate(&lockfile).unwrap();

    // Verify components — Kiểm tra components
    assert_eq!(bom.components.len(), 2);
    assert_eq!(bom.components[0].name, "lodash");
    assert_eq!(bom.components[1].name, "axios");

    // Verify hashes included — Kiểm tra hashes có được thêm
    assert!(bom.components[0].hashes.is_some());
    let hash = &bom.components[0].hashes.as_ref().unwrap()[0];
    assert_eq!(hash.alg, "BLAKE3");
    assert_eq!(hash.content, "abc123");

    // Verify dependencies graph — Kiểm tra dependency graph
    assert!(bom.dependencies.is_some());
    let deps = bom.dependencies.as_ref().unwrap();
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].dependency_ref, bom.components[1].bom_ref);
    assert_eq!(
        deps[0].depends_on.as_ref().unwrap()[0],
        bom.components[0].bom_ref
    );
}

#[test]
fn legacy_sbom_uses_ecosystem_specific_purls_instead_of_labeling_everything_npm() {
    use mgc_lockfile::{EcosystemTag, Lockfile, Package};

    let mut lockfile = Lockfile::new();
    lockfile.packages = vec![
        Package {
            name: "serde".into(),
            version: "1.0.0".into(),
            ecosystem: EcosystemTag::Rust,
            ..Default::default()
        },
        Package {
            name: "Django_REST.framework".into(),
            version: "3.15.0".into(),
            ecosystem: EcosystemTag::Python,
            ..Default::default()
        },
        Package {
            name: "org.example:core-lib".into(),
            version: "2.1.0".into(),
            ecosystem: EcosystemTag::Maven,
            ..Default::default()
        },
        Package {
            name: "@angular/core".into(),
            version: "18.0.0".into(),
            ecosystem: EcosystemTag::Web,
            ..Default::default()
        },
    ];

    let bom = SbomGenerator::default().generate(&lockfile).unwrap();
    let purls: Vec<_> = bom
        .components
        .iter()
        .map(|component| component.purl.as_deref())
        .collect();
    assert_eq!(
        purls,
        vec![
            Some("pkg:cargo/serde@1.0.0"),
            Some("pkg:pypi/django-rest-framework@3.15.0"),
            Some("pkg:maven/org.example/core-lib@2.1.0"),
            Some("pkg:npm/%40angular/core@18.0.0"),
        ]
    );
}

#[test]
fn legacy_sbom_refs_do_not_disclose_credentials_from_resolved_urls() {
    use mgc_lockfile::{EcosystemTag, Lockfile, Package};

    let secret_url = "https://build-user:token-value@packages.internal.invalid/pkg.tgz";
    let mut lockfile = Lockfile::new();
    lockfile.packages.push(Package {
        name: "private-lib".into(),
        version: "1.0.0".into(),
        resolved: secret_url.into(),
        ecosystem: EcosystemTag::Rust,
        ..Default::default()
    });

    let json = SbomGenerator::default().generate_json(&lockfile).unwrap();
    assert!(!json.contains("build-user"));
    assert!(!json.contains("token-value"));
    assert!(!json.contains("packages.internal.invalid"));
    let spdx = SbomGenerator::default()
        .generate_spdx_json(&lockfile, "private-project")
        .unwrap();
    assert!(!spdx.contains("build-user"));
    assert!(!spdx.contains("token-value"));
    assert!(!spdx.contains("packages.internal.invalid"));
}

#[test]
fn v4_sbom_refs_do_not_disclose_credentials_from_source_ids_or_swift_urls() {
    use mgc_lockfile::{
        EcosystemTag,
        canonical::{LockfileV4, PackageV4},
        v4::PackageKey,
    };

    let mut lock = LockfileV4::new("mgc-test");
    lock.packages.push(PackageV4 {
        key: PackageKey {
            ecosystem: EcosystemTag::Swift,
            name: "https://build-user:token-value@git.example.invalid/org/library.git".into(),
            version: "1.0.0".into(),
            source_id: "https://build-user:token-value@registry.example.invalid".into(),
            variant: Default::default(),
        },
        edges: vec![],
        artifact: None,
        provenance: None,
        toolchain: None,
        scripts_policy: None,
        store_ref: None,
    });
    lock.metadata.lockfile_hash = mgc_lockfile::canonical::payload_digest(&lock.payload());

    let json = SbomGenerator::default()
        .generate_json_v4(&lock, mgc_lockfile::policy::LockPolicyMode::Warn, &[])
        .unwrap();
    assert!(!json.contains("build-user"));
    assert!(!json.contains("token-value"));
    assert!(!json.contains("registry.example.invalid"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json).unwrap()["components"][0]["purl"],
        "pkg:swift/git.example.invalid/org/library@1.0.0"
    );
    let (spdx, _) = SbomGenerator::default()
        .generate_spdx_json_v4_with_report(
            &lock,
            mgc_lockfile::policy::LockPolicyMode::Warn,
            &[],
            "private-swift-project",
        )
        .unwrap();
    assert!(!spdx.contains("build-user"));
    assert!(!spdx.contains("token-value"));
    assert!(!spdx.contains("registry.example.invalid"));
}

#[test]
fn legacy_sbom_resolves_bare_dependency_names_only_when_unique() {
    use mgc_lockfile::{EcosystemTag, Lockfile, Package};

    let mut lock = Lockfile::new();
    lock.packages = vec![
        Package {
            name: "serde".into(),
            version: "1.0.0".into(),
            ecosystem: EcosystemTag::Rust,
            ..Default::default()
        },
        Package {
            name: "consumer".into(),
            version: "1.0.0".into(),
            dependencies: vec!["serde".into()],
            ecosystem: EcosystemTag::Rust,
            ..Default::default()
        },
    ];
    let bom = SbomGenerator::default().generate(&lock).unwrap();
    assert_eq!(
        bom.dependencies.as_ref().unwrap()[0]
            .depends_on
            .as_ref()
            .unwrap(),
        &[bom.components[0].bom_ref.clone()]
    );

    lock.packages.push(Package {
        name: "serde".into(),
        version: "2.0.0".into(),
        ecosystem: EcosystemTag::Rust,
        ..Default::default()
    });
    assert!(SbomGenerator::default().generate(&lock).is_err());
}

#[test]
fn v4_sbom_preserves_peer_instances_and_structured_edges() {
    use mgc_lockfile::{
        EcosystemTag,
        canonical::{LockfileV4, PackageV4},
        v4::{Edge, EdgeKind, EdgeOrigin, PackageKey, VariantKey},
    };

    let peer_a = PackageKey {
        ecosystem: EcosystemTag::Web,
        name: "plugin".into(),
        version: "1.0.0".into(),
        source_id: "npm-public".into(),
        variant: VariantKey {
            peer_context: Some("peer-a".into()),
            ..Default::default()
        },
    };
    let peer_b = PackageKey {
        variant: VariantKey {
            peer_context: Some("peer-b".into()),
            ..Default::default()
        },
        ..peer_a.clone()
    };
    let mut lock = LockfileV4::new("mgc-test");
    lock.packages = vec![
        PackageV4 {
            key: peer_a.clone(),
            edges: vec![Edge {
                target_key: peer_b.clone(),
                range: "*".into(),
                kind: EdgeKind::Peer,
                origin: EdgeOrigin::Manifest,
                marker: None,
            }],
            artifact: None,
            provenance: None,
            toolchain: None,
            scripts_policy: None,
            store_ref: None,
        },
        PackageV4 {
            key: peer_b,
            edges: vec![],
            artifact: None,
            provenance: None,
            toolchain: None,
            scripts_policy: None,
            store_ref: None,
        },
    ];
    lock.metadata.lockfile_hash = mgc_lockfile::canonical::payload_digest(&lock.payload());

    let bom = SbomGenerator::default()
        .generate_v4(&lock, mgc_lockfile::policy::LockPolicyMode::Warn, &[])
        .unwrap();
    assert_eq!(bom.components.len(), 2);
    assert_ne!(bom.components[0].bom_ref, bom.components[1].bom_ref);
    assert_eq!(
        bom.components[0].purl.as_deref(),
        Some("pkg:npm/plugin@1.0.0")
    );
    let edge = &bom.dependencies.as_ref().unwrap()[0];
    assert_eq!(edge.dependency_ref, bom.components[0].bom_ref);
    assert_eq!(
        edge.depends_on.as_ref().unwrap(),
        &[bom.components[1].bom_ref.clone()]
    );
}

#[test]
fn v4_sbom_refuses_bad_digest_and_dangling_edges() {
    use mgc_lockfile::{
        EcosystemTag,
        canonical::{LockfileV4, PackageV4},
        v4::{Edge, EdgeKind, EdgeOrigin, PackageKey, VariantKey},
    };

    let key = PackageKey {
        ecosystem: EcosystemTag::Rust,
        name: "serde".into(),
        version: "1.0.0".into(),
        source_id: "crates-io".into(),
        variant: VariantKey::default(),
    };
    let missing_key = PackageKey {
        name: "missing".into(),
        ..key.clone()
    };
    let package = PackageV4 {
        key,
        edges: vec![Edge {
            target_key: missing_key,
            range: "^1".into(),
            kind: EdgeKind::Normal,
            origin: EdgeOrigin::Manifest,
            marker: None,
        }],
        artifact: None,
        provenance: None,
        toolchain: None,
        scripts_policy: None,
        store_ref: None,
    };
    let mut lock = LockfileV4::new("mgc-test");
    lock.packages = vec![package];
    lock.metadata.lockfile_hash = mgc_lockfile::canonical::payload_digest(&lock.payload());
    assert!(
        SbomGenerator::default()
            .generate_v4(&lock, mgc_lockfile::policy::LockPolicyMode::Warn, &[])
            .is_err()
    );

    lock.packages[0].edges.clear();
    lock.metadata.lockfile_hash = "blake3-invalid".into();
    assert!(
        SbomGenerator::default()
            .generate_v4(&lock, mgc_lockfile::policy::LockPolicyMode::Warn, &[])
            .is_err()
    );
}

#[test]
fn test_sbom_generate_json() {
    use mgc_lockfile::{Lockfile, Package};

    // Create test lockfile — Tạo lockfile test
    let mut lockfile = Lockfile::new();
    lockfile.packages = vec![Package {
        name: "react".to_string(),
        version: "18.0.0".to_string(),
        resolved: "https://registry.npmjs.org/react/-/react-18.0.0.tgz".to_string(),
        integrity: "blake3:xyz789".to_string(),
        dependencies: vec![],
        ..Default::default()
    }];

    let generator = SbomGenerator::default();
    // Generate JSON — Tạo JSON
    let json = generator.generate_json(&lockfile).unwrap();

    // Verify JSON structure — Kiểm tra cấu trúc JSON
    assert!(json.contains("\"bomFormat\": \"CycloneDX\""));
    assert!(json.contains("\"name\": \"react\""));
    assert!(json.contains("\"version\": \"18.0.0\""));
}

#[test]
fn spdx_23_json_preserves_package_identity_hashes_and_dependency_edges() {
    use mgc_lockfile::{EcosystemTag, Lockfile, Package};

    let mut lockfile = Lockfile::new();
    lockfile.packages = vec![
        Package {
            name: "lodash".into(),
            version: "4.17.21".into(),
            ecosystem: EcosystemTag::Web,
            integrity: format!("sha256:{}", "a".repeat(64)),
            ..Default::default()
        },
        Package {
            name: "axios".into(),
            version: "1.0.0".into(),
            ecosystem: EcosystemTag::Web,
            dependencies: vec!["lodash@4.17.21".into()],
            ..Default::default()
        },
    ];

    let json = SbomGenerator::default()
        .generate_spdx_json(&lockfile, "sample-project@1.2.3")
        .unwrap();
    let document: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(document["spdxVersion"], "SPDX-2.3");
    assert_eq!(document["dataLicense"], "CC0-1.0");
    assert_eq!(document["SPDXID"], "SPDXRef-DOCUMENT");
    assert_eq!(document["name"], "sample-project@1.2.3");
    assert_eq!(document["packages"].as_array().unwrap().len(), 2);
    assert_eq!(document["documentDescribes"].as_array().unwrap().len(), 2);

    let packages = document["packages"].as_array().unwrap();
    let lodash = packages
        .iter()
        .find(|package| package["name"] == "lodash")
        .unwrap();
    let axios = packages
        .iter()
        .find(|package| package["name"] == "axios")
        .unwrap();
    assert_eq!(lodash["downloadLocation"], "NOASSERTION");
    assert_eq!(lodash["filesAnalyzed"], false);
    assert_eq!(lodash["licenseDeclared"], "NOASSERTION");
    assert_eq!(
        lodash["externalRefs"][0]["referenceCategory"],
        "PACKAGE-MANAGER"
    );
    assert_eq!(lodash["externalRefs"][0]["referenceType"], "purl");
    assert_eq!(
        lodash["externalRefs"][0]["referenceLocator"],
        "pkg:npm/lodash@4.17.21"
    );
    assert_eq!(lodash["checksums"][0]["algorithm"], "SHA256");
    assert_eq!(lodash["checksums"][0]["checksumValue"], "a".repeat(64));

    let relationships = document["relationships"].as_array().unwrap();
    assert!(relationships.iter().any(|relationship| {
        relationship["spdxElementId"] == axios["SPDXID"]
            && relationship["relationshipType"] == "DEPENDS_ON"
            && relationship["relatedSpdxElement"] == lodash["SPDXID"]
    }));
}

#[test]
fn test_sbom_empty_lockfile() {
    use mgc_lockfile::Lockfile;

    // Empty lockfile test — Test lockfile rỗng
    let lockfile = Lockfile::new();
    let generator = SbomGenerator::default();
    let bom = generator.generate(&lockfile).unwrap();

    assert_eq!(bom.components.len(), 0);
    assert!(bom.dependencies.is_none() || bom.dependencies.as_ref().unwrap().is_empty());
}
