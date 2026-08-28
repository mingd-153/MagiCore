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
        },
        Package {
            name: "axios".to_string(),
            version: "1.0.0".to_string(),
            resolved: "https://registry.npmjs.org/axios/-/axios-1.0.0.tgz".to_string(),
            integrity: "blake3:def456".to_string(),
            dependencies: vec!["lodash@4.17.21".to_string()],
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
    assert_eq!(deps[0].dependency_ref, "pkg:axios@1.0.0");
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
fn test_sbom_empty_lockfile() {
    use mgc_lockfile::Lockfile;

    // Empty lockfile test — Test lockfile rỗng
    let lockfile = Lockfile::new();
    let generator = SbomGenerator::default();
    let bom = generator.generate(&lockfile).unwrap();

    assert_eq!(bom.components.len(), 0);
    assert!(bom.dependencies.is_none() || bom.dependencies.as_ref().unwrap().is_empty());
}
