//! Tests for scaffold resolver (cli/src/scaffold/resolver.rs)

#![allow(clippy::unwrap_used)]

use mgc::scaffold::resolver::{MissingLayersReport, ScaffoldResolveStatus};
use mgc::scaffold::spec::{parse_scaffold_spec, CoreKind};
use std::path::PathBuf;

#[test]
fn test_resolve_status_embedded_is_available() {
    let status = ScaffoldResolveStatus::Embedded {
        layer: "web/frontend/vanilla".to_string(),
    };
    assert!(status.is_available());
    assert_eq!(status.layer(), "web/frontend/vanilla");
}

#[test]
fn test_resolve_status_cache_hit_is_available() {
    let status = ScaffoldResolveStatus::CacheHit {
        layer: "web/frontend/nextjs".to_string(),
        version: Some("15.5.0".to_string()),
        path: PathBuf::from("/tmp/cache/nextjs"),
    };
    assert!(status.is_available());
    assert_eq!(status.layer(), "web/frontend/nextjs");
}

#[test]
fn test_resolve_status_fetched_is_available() {
    let status = ScaffoldResolveStatus::Fetched {
        layer: "ai/fastapi".to_string(),
        version: "0.115.0".to_string(),
        path: PathBuf::from("/tmp/cache/fastapi"),
    };
    assert!(status.is_available());
    assert_eq!(status.layer(), "ai/fastapi");
}

#[test]
fn test_resolve_status_optional_missing_not_available() {
    let status = ScaffoldResolveStatus::OptionalMissing {
        layer: "web/partial/extra".to_string(),
        reason: "not in cache".to_string(),
    };
    assert!(!status.is_available());
    assert_eq!(status.layer(), "web/partial/extra");
}

#[test]
fn test_missing_layers_report_empty() {
    let report = MissingLayersReport::new();
    assert!(!report.has_required_missing());
    assert!(report.is_empty());
}

#[test]
fn test_missing_layers_report_required_only() {
    let mut report = MissingLayersReport::new();
    report.add_required("web/frontend/nextjs".to_string());
    report.add_required("web/base/runtime".to_string());

    assert!(report.has_required_missing());
    assert!(!report.is_empty());
    assert_eq!(report.required.len(), 2);
    assert_eq!(report.optional.len(), 0);
}

#[test]
fn test_missing_layers_report_optional_only() {
    let mut report = MissingLayersReport::new();
    report.add_optional("web/partial/extra".to_string());

    assert!(!report.has_required_missing());
    assert!(!report.is_empty());
    assert_eq!(report.required.len(), 0);
    assert_eq!(report.optional.len(), 1);
}

#[test]
fn test_missing_layers_report_mixed() {
    let mut report = MissingLayersReport::new();
    report.add_required("web/frontend/nextjs".to_string());
    report.add_optional("web/partial/extra".to_string());

    assert!(report.has_required_missing());
    assert!(!report.is_empty());

    let msg = report.format_error("web", "nextjs");
    assert!(
        msg.contains("Required scaffold layers missing"),
        "Should mention required layers"
    );
    assert!(
        msg.contains("web/frontend/nextjs"),
        "Should list required layer"
    );
    assert!(
        msg.contains("mgc template fetch"),
        "Should suggest fetch command"
    );
    assert!(
        msg.contains("Optional layers not found"),
        "Should mention optional layers"
    );
}

#[test]
fn test_missing_layers_report_format_shows_count() {
    let mut report = MissingLayersReport::new();
    report.add_required("layer1".to_string());
    report.add_required("layer2".to_string());
    report.add_required("layer3".to_string());

    let msg = report.format_error("web", "nextjs");
    assert!(msg.contains("(3 total)"), "Should show count of layers");
}

#[test]
fn test_spec_to_layer_path_web() {
    use mgc::scaffold::resolver::spec_to_layer_path;

    let spec = parse_scaffold_spec(CoreKind::Web, "nextjs@latest").unwrap();
    let path = spec_to_layer_path(&spec);
    assert_eq!(path, "web/frontend/nextjs");
}

#[test]
fn test_spec_to_layer_path_ai() {
    use mgc::scaffold::resolver::spec_to_layer_path;

    let spec = parse_scaffold_spec(CoreKind::Ai, "fastapi").unwrap();
    let path = spec_to_layer_path(&spec);
    assert_eq!(path, "ai/fastapi");
}

#[test]
fn test_spec_to_layer_path_app() {
    use mgc::scaffold::resolver::spec_to_layer_path;

    let spec = parse_scaffold_spec(CoreKind::App, "flutter").unwrap();
    let path = spec_to_layer_path(&spec);
    assert_eq!(path, "app/flutter");
}

#[test]
fn test_spec_to_layer_path_lib() {
    use mgc::scaffold::resolver::spec_to_layer_path;

    let spec = parse_scaffold_spec(CoreKind::Lib, "rust").unwrap();
    let path = spec_to_layer_path(&spec);
    assert_eq!(path, "lib/rust");
}

#[test]
fn test_spec_to_layer_path_normalizes_name() {
    use mgc::scaffold::resolver::spec_to_layer_path;

    let spec = parse_scaffold_spec(CoreKind::App, "React_Native").unwrap();
    let path = spec_to_layer_path(&spec);
    assert_eq!(
        path, "app/react-native",
        "Should normalize to kebab-case lowercase"
    );
}
