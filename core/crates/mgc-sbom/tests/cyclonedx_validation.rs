//! W6 T6.5: CycloneDX validation tests

use mgc_lockfile::{Lockfile, LockfileMetadata, Package};
use mgc_sbom::{SbomGenerator, SbomOptions};

fn create_test_lockfile() -> Lockfile {
    Lockfile {
        version: "2".to_string(),
        metadata: LockfileMetadata {
            generated_at: "2026-08-21T00:00:00Z".to_string(),
            generator: "mgc/0.4.0".to_string(),
            lockfile_hash: "test_hash".to_string(),
            signer: None,
        },
        packages: vec![
            Package {
                name: "react".to_string(),
                version: "18.2.0".to_string(),
                resolved: "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
                integrity: "blake3:abc123".to_string(),
                dependencies: vec!["loose-envify".to_string()],
            },
            Package {
                name: "loose-envify".to_string(),
                version: "1.4.0".to_string(),
                resolved: "https://registry.npmjs.org/loose-envify/-/loose-envify-1.4.0.tgz"
                    .to_string(),
                integrity: "blake3:def456".to_string(),
                dependencies: vec![],
            },
        ],
    }
}

#[test]
fn test_cyclonedx_schema_structure() {
    let lockfile = create_test_lockfile();
    let generator = SbomGenerator::new(SbomOptions::default());
    let json = generator.generate_json(&lockfile).unwrap();

    // Parse and validate structure
    let bom: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Required CycloneDX fields
    assert_eq!(bom["bomFormat"], "CycloneDX");
    assert_eq!(bom["specVersion"], "1.5");
    assert!(bom["serialNumber"]
        .as_str()
        .unwrap()
        .starts_with("urn:uuid:"));
    assert!(bom["version"].as_i64().unwrap() >= 1);

    // Metadata
    assert!(bom["metadata"].is_object());
    assert!(bom["metadata"]["timestamp"].is_string());

    // Components
    assert!(bom["components"].is_array());
    let components = bom["components"].as_array().unwrap();
    assert_eq!(components.len(), 2);

    // First component structure
    let first = &components[0];
    assert_eq!(first["type"], "library");
    assert_eq!(first["name"], "react");
    assert_eq!(first["version"], "18.2.0");
    assert!(first["purl"].as_str().unwrap().contains("react"));
}

#[test]
fn test_cyclonedx_dependencies() {
    let lockfile = create_test_lockfile();
    let generator = SbomGenerator::new(SbomOptions::default());
    let json = generator.generate_json(&lockfile).unwrap();

    let bom: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Dependencies array
    assert!(bom["dependencies"].is_array());
    let deps = bom["dependencies"].as_array().unwrap();

    // Find react dependency
    let react_dep = deps
        .iter()
        .find(|d| d["ref"].as_str().unwrap().contains("react@18.2.0"))
        .unwrap();

    assert!(react_dep["dependsOn"].is_array());
    let depends_on = react_dep["dependsOn"].as_array().unwrap();
    assert_eq!(depends_on.len(), 1);
    assert!(depends_on[0].as_str().unwrap().contains("loose-envify"));
}

#[test]
fn test_metadata_structure() {
    let lockfile = create_test_lockfile();
    let generator = SbomGenerator::new(SbomOptions::default());
    let json = generator.generate_json(&lockfile).unwrap();

    let bom: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Check metadata exists
    assert!(bom["metadata"].is_object());
    assert!(bom["metadata"]["timestamp"].is_string());
    assert!(bom["metadata"]["tools"].is_array());
}

#[test]
fn test_purl_format() {
    let lockfile = create_test_lockfile();
    let generator = SbomGenerator::new(SbomOptions::default());
    let json = generator.generate_json(&lockfile).unwrap();

    let bom: serde_json::Value = serde_json::from_str(&json).unwrap();
    let components = bom["components"].as_array().unwrap();

    // Validate PackageURL format (https://github.com/package-url/purl-spec)
    for component in components {
        if let Some(purl_str) = component["purl"].as_str() {
            assert!(purl_str.starts_with("pkg:npm/"));
            assert!(purl_str.contains("@"));
        }
    }
}

#[test]
fn test_unique_component_refs() {
    let lockfile = create_test_lockfile();
    let generator = SbomGenerator::new(SbomOptions::default());
    let json = generator.generate_json(&lockfile).unwrap();

    let bom: serde_json::Value = serde_json::from_str(&json).unwrap();
    let components = bom["components"].as_array().unwrap();

    // All component refs must be unique
    let mut refs = std::collections::HashSet::new();
    for component in components {
        if let Some(purl) = component["purl"].as_str() {
            assert!(refs.insert(purl), "Duplicate component ref: {}", purl);
        }
    }
}

#[test]
fn test_empty_lockfile() {
    let lockfile = Lockfile {
        version: "2".to_string(),
        metadata: LockfileMetadata {
            generated_at: "2026-08-21T00:00:00Z".to_string(),
            generator: "mgc/0.4.0".to_string(),
            lockfile_hash: "empty".to_string(),
            signer: None,
        },
        packages: vec![],
    };

    let generator = SbomGenerator::new(SbomOptions::default());
    let json = generator.generate_json(&lockfile).unwrap();

    let bom: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Empty lockfile should still have valid BOM structure
    assert_eq!(bom["bomFormat"], "CycloneDX");
    assert!(bom["components"].is_array());
    assert_eq!(bom["components"].as_array().unwrap().len(), 0);
    // dependencies field may be None (omitted in JSON) for empty lockfile
}
