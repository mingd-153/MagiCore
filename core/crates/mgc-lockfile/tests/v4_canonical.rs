//! v4 canonical payload tests (design §1): determinism, golden bytes,
//! digest stability, TOML validity, out-of-payload exclusion.
//! (Test payload canonical v4: tất định, byte golden, digest ổn định,
//! TOML hợp lệ, loại trừ ngoài-payload.)

use std::collections::BTreeMap;

use mgc_lockfile::EcosystemTag;
use mgc_lockfile::canonical::{
    LockfileV4, LockfileV4Metadata, PackageV4, canonical_toml, parse_v4_document, payload_digest,
    write_v4_document,
};
use mgc_lockfile::parser::parse_document;
use mgc_lockfile::schema::{ArtifactRef, CrossEdge, Provenance, WorkspaceTopology};
use mgc_lockfile::v4::{
    Edge, EdgeKind, EdgeOrigin, PackageKey, SignatureBlock, SourceRef, VariantKey,
    peer_context_digest,
};

fn sample_sources() -> Vec<SourceRef> {
    vec![
        SourceRef {
            id: "pypi".to_string(),
            url: "https://pypi.org/simple".to_string(),
            ecosystem: EcosystemTag::Python,
            priority: 100,
            claims: vec!["*".to_string()],
            trusted: false,
            allow_hosts: Vec::new(),
            allow_cidrs: Vec::new(),
            allow_protocols: vec!["https".to_string()],
        },
        SourceRef {
            id: "internal".to_string(),
            url: "https://nexus.corp/simple".to_string(),
            ecosystem: EcosystemTag::Python,
            priority: 10,
            claims: vec!["corp-*".to_string()],
            trusted: true,
            allow_hosts: vec!["nexus.corp".to_string()],
            allow_cidrs: Vec::new(),
            allow_protocols: vec!["https".to_string()],
        },
    ]
}

fn sample_packages() -> Vec<PackageV4> {
    let peers = vec![
        ("react".to_string(), "18.2.0".to_string()),
        ("react-dom".to_string(), "18.2.0".to_string()),
    ];
    vec![
        PackageV4 {
            key: PackageKey {
                ecosystem: EcosystemTag::Python,
                name: "requests".to_string(),
                version: "2.31.0".to_string(),
                source_id: "pypi".to_string(),
                variant: VariantKey::default(),
            },
            edges: Vec::new(),
            artifact: None,
            provenance: None,
            toolchain: None,
            scripts_policy: None,
            store_ref: None,
        },
        PackageV4 {
            key: PackageKey {
                ecosystem: EcosystemTag::Web,
                name: "button".to_string(),
                version: "1.0.0".to_string(),
                source_id: "npm".to_string(),
                variant: VariantKey {
                    peer_context: Some(peer_context_digest(&peers)),
                    feature_set: None,
                    target: None,
                },
            },
            edges: vec![Edge {
                target_key: PackageKey {
                    ecosystem: EcosystemTag::Web,
                    name: "react".to_string(),
                    version: "18.2.0".to_string(),
                    source_id: "npm".to_string(),
                    variant: VariantKey::default(),
                },
                range: "^18.0.0".to_string(),
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
    ]
}

fn sample_lockfile() -> LockfileV4 {
    let peers = vec![
        ("react".to_string(), "18.2.0".to_string()),
        ("react-dom".to_string(), "18.2.0".to_string()),
    ];
    let mut peer_contexts = BTreeMap::new();
    peer_contexts.insert(peer_context_digest(&peers), peers);
    LockfileV4 {
        version: "4".to_string(),
        metadata: LockfileV4Metadata {
            generated_at: "2026-09-17T00:00:00Z".to_string(),
            generator: "mgc/1.2.0".to_string(),
            lockfile_hash: "junk-should-not-matter".to_string(),
            signature: None,
        },
        sources: sample_sources(),
        peer_contexts,
        root_dependencies: vec!["requests@2.31.0".to_string()],
        root_dependencies_by_owner: BTreeMap::new(),
        packages: sample_packages(),
        workspace: None,
        optimizer_profile: None,
    }
}

#[test]
fn canonical_output_is_order_independent() {
    let first = canonical_toml(&sample_lockfile().payload());
    let mut shuffled = sample_lockfile();
    shuffled.packages.reverse();
    shuffled.sources.reverse();
    let second = canonical_toml(&shuffled.payload());
    assert_eq!(first, second);
}

#[test]
fn edge_target_variant_is_authenticated() {
    let original = sample_lockfile();
    for axis in ["target", "features", "peers"] {
        let mut changed = original.clone();
        let variant = &mut changed.packages[1].edges[0].target_key.variant;
        match axis {
            "target" => variant.target = Some("linux-x86_64-gnu".into()),
            "features" => variant.feature_set = Some(vec!["tls".into()]),
            _ => variant.peer_context = Some("different-peer-context".into()),
        }
        assert_ne!(
            payload_digest(&original.payload()),
            payload_digest(&changed.payload()),
            "edge {axis} must be authenticated"
        );
    }
}

#[test]
fn transitive_origin_variant_is_authenticated() {
    let mut original = sample_lockfile();
    original.packages[1].edges[0].origin = EdgeOrigin::Transitive {
        from_key: Box::new(original.packages[0].key.clone()),
    };
    let mut changed = original.clone();
    if let EdgeOrigin::Transitive { from_key } = &mut changed.packages[1].edges[0].origin {
        from_key.variant.target = Some("win32-x86_64-msvc".into());
    }
    assert_ne!(
        payload_digest(&original.payload()),
        payload_digest(&changed.payload())
    );
}

#[test]
fn equal_name_edges_have_order_independent_full_identity() {
    let mut original = sample_lockfile();
    let mut sibling = original.packages[1].edges[0].clone();
    sibling.target_key.source_id = "private-npm".into();
    sibling.target_key.variant.target = Some("linux-x86_64-gnu".into());
    sibling.marker = Some("platform == 'linux'".into());
    original.packages[1].edges.push(sibling);
    let mut shuffled = original.clone();
    shuffled.packages[1].edges.reverse();
    assert_eq!(
        canonical_toml(&original.payload()),
        canonical_toml(&shuffled.payload())
    );
}

#[test]
fn feature_set_order_does_not_change_package_order_or_digest() {
    let mut original = sample_lockfile();
    original.packages[1].key.variant.feature_set = Some(vec!["b".into(), "a".into()]);
    let mut sibling = original.packages[1].clone();
    sibling.key.variant.feature_set = Some(vec!["a".into(), "c".into()]);
    original.packages.push(sibling);
    let mut shuffled = original.clone();
    shuffled.packages[1]
        .key
        .variant
        .feature_set
        .as_mut()
        .unwrap()
        .reverse();
    assert_eq!(
        payload_digest(&original.payload()),
        payload_digest(&shuffled.payload())
    );
}

#[test]
fn out_of_payload_fields_do_not_change_bytes() {
    let clean = canonical_toml(&sample_lockfile().payload());
    let mut dirty = sample_lockfile();
    dirty.metadata.lockfile_hash = "tampered".to_string();
    dirty.metadata.generated_at = "1999-01-01T00:00:00Z".to_string();
    assert_eq!(clean, canonical_toml(&dirty.payload()));
}

#[test]
fn digest_is_stable_and_sensitive() {
    let payload = sample_lockfile().payload();
    let first = payload_digest(&payload);
    let second = payload_digest(&sample_lockfile().payload());
    assert_eq!(first, second);
    assert!(first.starts_with("blake3-"));
    let mut changed = sample_lockfile();
    changed.packages[0].key.version = "2.32.0".to_string();
    assert_ne!(first, payload_digest(&changed.payload()));
}

#[test]
fn owner_scoped_root_edges_are_signed_payload() {
    let baseline = sample_lockfile();
    let baseline_digest = payload_digest(&baseline.payload());
    let mut changed = baseline;
    changed
        .root_dependencies_by_owner
        .insert("lib".into(), vec!["requests@2.32.0".into()]);
    assert_ne!(baseline_digest, payload_digest(&changed.payload()));
}

#[test]
fn workspace_topology_and_optimizer_profile_are_signed_payload() {
    let baseline = sample_lockfile();
    let baseline_digest = payload_digest(&baseline.payload());

    let mut changed_workspace = baseline.clone();
    changed_workspace.workspace = Some(WorkspaceTopology {
        members: vec!["web-app".into(), "python-api".into()],
        cross_core_edges: vec![CrossEdge {
            from_core: "web".into(),
            to_core: "ai".into(),
            package: "shared-model@1.0.0".into(),
        }],
    });
    assert_ne!(
        baseline_digest,
        payload_digest(&changed_workspace.payload()),
        "cross-core workspace topology must be authenticated"
    );

    let mut changed_optimizer = baseline;
    changed_optimizer.optimizer_profile = Some("gpu-low-memory".into());
    assert_ne!(
        baseline_digest,
        payload_digest(&changed_optimizer.payload()),
        "the optimizer profile must be authenticated"
    );
}

#[test]
fn workspace_collection_order_does_not_change_canonical_bytes() {
    let mut original = sample_lockfile();
    original.workspace = Some(WorkspaceTopology {
        members: vec!["web-app".into(), "python-api".into()],
        cross_core_edges: vec![
            CrossEdge {
                from_core: "web".into(),
                to_core: "ai".into(),
                package: "z-model@1.0.0".into(),
            },
            CrossEdge {
                from_core: "app".into(),
                to_core: "lib".into(),
                package: "a-shared@2.0.0".into(),
            },
        ],
    });
    original.optimizer_profile = Some("balanced".into());

    let mut shuffled = original.clone();
    let workspace = shuffled.workspace.as_mut().unwrap();
    workspace.members.reverse();
    workspace.cross_core_edges.reverse();

    assert_eq!(
        canonical_toml(&original.payload()),
        canonical_toml(&shuffled.payload())
    );
}

#[test]
fn workspace_and_optimizer_payload_round_trip_with_matching_digest() {
    let mut document = sample_lockfile();
    document.workspace = Some(WorkspaceTopology {
        members: vec!["app".into(), "web".into()],
        cross_core_edges: vec![CrossEdge {
            from_core: "app".into(),
            to_core: "web".into(),
            package: "shared-ui@1.0.0".into(),
        }],
    });
    document.optimizer_profile = Some("balanced".into());
    document.metadata.lockfile_hash = payload_digest(&document.payload());

    let parsed = parse_v4_document(&write_v4_document(&document).unwrap()).unwrap();

    assert_eq!(parsed.workspace, document.workspace);
    assert_eq!(parsed.optimizer_profile, document.optimizer_profile);
    assert_eq!(
        payload_digest(&parsed.payload()),
        parsed.metadata.lockfile_hash
    );
}

#[test]
fn owner_scoped_root_edges_round_trip_through_v4_document() {
    let mut document = sample_lockfile();
    document.root_dependencies_by_owner.insert(
        "app".into(),
        vec!["flutter-pkg@1.2.3".into(), "swift-pkg@4.5.6".into()],
    );
    document.metadata.lockfile_hash = payload_digest(&document.payload());
    let bytes = write_v4_document(&document).unwrap();
    let parsed = parse_v4_document(&bytes).unwrap();
    assert_eq!(
        parsed.root_dependencies_by_owner["app"],
        ["flutter-pkg@1.2.3", "swift-pkg@4.5.6"]
    );
    assert_eq!(
        payload_digest(&parsed.payload()),
        parsed.metadata.lockfile_hash
    );
}

#[test]
fn canonical_output_parses_as_toml() {
    let text = canonical_toml(&sample_lockfile().payload());
    let value: toml::Value = toml::from_str(&text).expect("canonical output must parse");
    assert_eq!(value.get("version").and_then(|v| v.as_str()), Some("4"));
    assert_eq!(
        value
            .get("metadata")
            .and_then(|m| m.get("generator"))
            .and_then(|g| g.as_str()),
        Some("mgc/1.2.0")
    );
    assert!(value.get("lockfile_hash").is_none());
    assert!(value.get("generated_at").is_none());
    assert!(value.get("signature").is_none());
}

#[test]
fn unknown_v4_root_field_is_rejected() {
    let mut text = write_v4_document(&sample_lockfile()).unwrap();
    text.insert_str(0, "unrecognized_root_field = true\n");

    assert!(
        parse_v4_document(&text).is_err(),
        "v4 parsing must not silently discard an unknown root field"
    );
    assert!(
        parse_document(&text).is_err(),
        "version-dispatched production parsing must reject unknown v4 root fields"
    );
}

#[test]
fn unknown_v4_source_and_package_fields_are_rejected() {
    let mut extended = sample_lockfile();
    extended.packages[0].artifact = Some(ArtifactRef {
        url: "https://files.example.test/pkg.whl".into(),
        size_bytes: Some(512),
        content_hash: "blake3:fixture".into(),
        downloaded_from: "https://pypi.org/simple".into(),
    });
    extended.packages[0].provenance = Some(Provenance {
        source_kind: "native-resolve".into(),
        tool: Some("mgc-test".into()),
        imported_from: None,
    });
    extended.metadata.signature = Some(SignatureBlock {
        algorithm: "ed25519".into(),
        key_id: "0123456789abcdef".into(),
        public_key: "fixture".into(),
        digest: "blake3-fixture".into(),
        signed_at: "2026-10-01T00:00:00Z".into(),
        signature: "ed25519-fixture".into(),
    });
    extended.workspace = Some(WorkspaceTopology {
        members: vec!["web".into(), "ai".into()],
        cross_core_edges: vec![CrossEdge {
            from_core: "web".into(),
            to_core: "ai".into(),
            package: "model-client".into(),
        }],
    });
    let valid = write_v4_document(&extended).unwrap();
    assert_eq!(parse_v4_document(&valid).unwrap(), extended);
    let cases = [
        (
            "source",
            valid.replacen(
                "[[sources]]",
                "[[sources]]\nunrecognized_source_field = true",
                1,
            ),
        ),
        (
            "package",
            valid.replacen(
                "[[package]]",
                "[[package]]\nunrecognized_package_field = true",
                1,
            ),
        ),
        (
            "package key",
            valid.replacen(
                "[package.key]",
                "[package.key]\nunrecognized_key_field = true",
                1,
            ),
        ),
        (
            "package variant",
            valid.replacen(
                "[package.key.variant]",
                "[package.key.variant]\nunrecognized_variant_field = true",
                1,
            ),
        ),
        (
            "dependency edge",
            valid.replacen(
                "[[package.edges]]",
                "[[package.edges]]\nunrecognized_edge_field = true",
                1,
            ),
        ),
        (
            "artifact",
            valid.replacen(
                "[package.artifact]",
                "[package.artifact]\nunrecognized_artifact_field = true",
                1,
            ),
        ),
        (
            "provenance",
            valid.replacen(
                "[package.provenance]",
                "[package.provenance]\nunrecognized_provenance_field = true",
                1,
            ),
        ),
        (
            "signature",
            valid.replacen(
                "[metadata.signature]",
                "[metadata.signature]\nunrecognized_signature_field = true",
                1,
            ),
        ),
        (
            "workspace",
            valid.replacen(
                "[workspace]",
                "[workspace]\nunrecognized_workspace_field = true",
                1,
            ),
        ),
        (
            "workspace edge",
            valid.replacen(
                "[[workspace.cross_core_edges]]",
                "[[workspace.cross_core_edges]]\nunrecognized_workspace_edge_field = true",
                1,
            ),
        ),
    ];

    for (location, invalid) in cases {
        assert!(
            parse_v4_document(&invalid).is_err(),
            "v4 parsing must reject unknown {location} fields"
        );
        assert!(
            parse_document(&invalid).is_err(),
            "version-dispatched production parsing must reject unknown v4 {location} fields"
        );
    }
}

#[test]
fn unknown_transitive_origin_field_is_rejected() {
    let mut document = sample_lockfile();
    document.packages[1].edges[0].origin = EdgeOrigin::Transitive {
        from_key: Box::new(document.packages[0].key.clone()),
    };
    let serialized = write_v4_document(&document).unwrap();
    let mut value: toml::Value = toml::from_str(&serialized).unwrap();
    let packages = value
        .get_mut("package")
        .and_then(toml::Value::as_array_mut)
        .unwrap();
    let package_with_edge = packages
        .iter_mut()
        .find(|package| {
            package
                .get("edges")
                .and_then(toml::Value::as_array)
                .is_some_and(|edges| !edges.is_empty())
        })
        .unwrap();
    let edge = package_with_edge
        .get_mut("edges")
        .and_then(toml::Value::as_array_mut)
        .and_then(|edges| edges.first_mut())
        .unwrap();
    let transitive = edge
        .get_mut("origin")
        .and_then(toml::Value::as_table_mut)
        .and_then(|origin| origin.get_mut("Transitive"))
        .and_then(toml::Value::as_table_mut)
        .unwrap();
    transitive.insert(
        "unrecognized_origin_field".into(),
        toml::Value::Boolean(true),
    );
    let invalid = toml::to_string(&value).unwrap();

    assert!(
        parse_v4_document(&invalid).is_err(),
        "v4 parsing must reject unknown fields inside the transitive-origin variant"
    );
    assert!(
        parse_document(&invalid).is_err(),
        "version-dispatched production parsing must reject unknown fields inside the transitive-origin variant"
    );
}

#[test]
fn escaping_round_trips() {
    let mut lock = sample_lockfile();
    lock.packages[0].key.name = "we\"ird\nname".to_string();
    let text = canonical_toml(&lock.payload());
    let value: toml::Value = toml::from_str(&text).expect("escaped output must parse");
    let names: Vec<&str> = value
        .get("package")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.get("key")?.get("name")?.as_str())
                .collect()
        })
        .unwrap_or_default();
    assert!(names.contains(&"we\"ird\nname"));
}

#[test]
fn golden_canonical_bytes() {
    // Frozen 2026-09-17 after review: ANY byte change here means the
    // digest contract moved and every existing v4 signature breaks — that
    // requires a schema bump, never a silent edit.
    // (Đóng băng sau review: MỌI thay đổi byte nghĩa là hợp đồng digest
    // đã đổi và mọi chữ ký v4 cũ vỡ — phải bump schema, không bao giờ sửa
    // âm thầm.)
    let expected = "version = \"4\"\n\
        \n\
        [metadata]\n\
        generator = \"mgc/1.2.0\"\n\
        \n\
        [[sources]]\n\
        id = \"internal\"\n\
        url = \"https://nexus.corp/simple\"\n\
        ecosystem = \"python\"\n\
        priority = 10\n\
        claims = [\"corp-*\"]\n\
        trusted = true\n\
        allow_hosts = [\"nexus.corp\"]\n\
        allow_cidrs = []\n\
        allow_protocols = [\"https\"]\n\
        \n\
        [[sources]]\n\
        id = \"pypi\"\n\
        url = \"https://pypi.org/simple\"\n\
        ecosystem = \"python\"\n\
        priority = 100\n\
        claims = [\"*\"]\n\
        trusted = false\n\
        allow_hosts = []\n\
        allow_cidrs = []\n\
        allow_protocols = [\"https\"]\n\
        \n\
        [peer_contexts]\n\
        \"938aa82c8c0ffb065d64770cd5285ae6d940c06110e1d7f2777b512bce141eb7\" = [[\"react\", \"18.2.0\"], [\"react-dom\", \"18.2.0\"]]\n\
        \n\
        root_dependencies = [\"requests@2.31.0\"]\n\
        \n\
        [[package]]\n\
        [package.key]\n\
        ecosystem = \"python\"\n\
        name = \"requests\"\n\
        version = \"2.31.0\"\n\
        source_id = \"pypi\"\n\
        \n\
        [[package]]\n\
        [package.key]\n\
        ecosystem = \"web\"\n\
        name = \"button\"\n\
        version = \"1.0.0\"\n\
        source_id = \"npm\"\n\
        [package.key.variant]\n\
        peer_context = \"938aa82c8c0ffb065d64770cd5285ae6d940c06110e1d7f2777b512bce141eb7\"\n\
        [[package.edges]]\n\
        [package.edges.target_key]\n\
        ecosystem = \"web\"\n\
        name = \"react\"\n\
        version = \"18.2.0\"\n\
        source_id = \"npm\"\n\
        range = \"^18.0.0\"\n\
        kind = \"peer\"\n\
        origin = \"manifest\"\n\
        \n";
    assert_eq!(canonical_toml(&sample_lockfile().payload()), expected);
}

#[test]
fn payload_struct_covers_document() {
    let lock = sample_lockfile();
    let payload = lock.payload();
    assert_eq!(payload.version, "4");
    assert_eq!(payload.generator, "mgc/1.2.0");
    assert_eq!(payload.sources.len(), 2);
    assert_eq!(payload.packages.len(), 2);
}
