//! v4 canonical payload tests (design §1): determinism, golden bytes,
//! digest stability, TOML validity, out-of-payload exclusion.
//! (Test payload canonical v4: tất định, byte golden, digest ổn định,
//! TOML hợp lệ, loại trừ ngoài-payload.)

use std::collections::BTreeMap;

use mgc_lockfile::EcosystemTag;
use mgc_lockfile::canonical::{
    LockfileV4, LockfileV4Metadata, PackageV4, canonical_toml, payload_digest,
};
use mgc_lockfile::v4::{
    Edge, EdgeKind, EdgeOrigin, PackageKey, SourceRef, VariantKey, peer_context_digest,
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
