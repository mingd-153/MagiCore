#![cfg(test)]
#![allow(clippy::unwrap_used)]

use mgc_sbom::{Bom, Component, ComponentType, SbomFormat, SbomGenerator, SbomOptions};

#[test]
fn test_sbom_format_cyclonedx() {
    let format = SbomFormat::CycloneDx;
    assert_eq!(format, SbomFormat::CycloneDx);
}

#[test]
fn test_component_can_be_created() {
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
    let options = SbomOptions {
        include_dev: false,
        include_licenses: true,
        include_hashes: true,
        format: SbomFormat::CycloneDx,
    };
    let _generator = SbomGenerator::new(options);
    // Generator created successfully
}
