//! Integration tests for built-in plugins (Audit, LicenseCheck, SizeReport, DepGraph).

use mg_plugins::{
    AuditPlugin, AuditWarning, DepGraph, DepGraphPlugin, GraphReport, LicenseCheckPlugin,
    LicenseWarning, PackageInfo, SizeReport, SizeReportPlugin,
};

fn make_pkg(
    name: &str,
    version: &str,
    integrity: Option<&str>,
    size: Option<i64>,
    license: Option<&str>,
    deps: Vec<String>,
) -> PackageInfo {
    PackageInfo {
        id: format!("{}@{}", name, version),
        name: name.to_string(),
        version: version.to_string(),
        dependencies: deps,
        integrity: integrity.map(|s| s.to_string()),
        size,
        license: license.map(|s| s.to_string()),
    }
}

mod audit_tests {
    use super::*;

    #[test]
    fn test_audit_clean_package_succeeds() {
        let pkgs = vec![make_pkg(
            "react",
            "18.2.0",
            Some("sha512-abc"),
            Some(1024),
            None,
            vec![],
        )];
        let result = AuditPlugin::run_audit(&pkgs);
        assert!(result.success, "Clean package should pass audit");
    }

    #[test]
    fn test_audit_known_vulnerability_lodash() {
        let pkgs = vec![make_pkg(
            "lodash",
            "4.17.20",
            Some("sha512-abc"),
            Some(1024),
            None,
            vec![],
        )];
        let result = AuditPlugin::run_audit(&pkgs);
        assert!(!result.success, "Vulnerable lodash should fail audit");
        let warnings: Vec<AuditWarning> = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(warnings
            .iter()
            .any(|w| w.warning_type == "known-vulnerability"));
    }

    #[test]
    fn test_audit_known_vulnerability_safe_version_passes() {
        let pkgs = vec![make_pkg(
            "lodash",
            "4.17.21",
            Some("sha512-abc"),
            Some(1024),
            None,
            vec![],
        )];
        let result = AuditPlugin::run_audit(&pkgs);
        let warnings: Vec<AuditWarning> =
            serde_json::from_str(&result.data.clone().unwrap_or_default()).unwrap();
        assert!(!warnings
            .iter()
            .any(|w| w.warning_type == "known-vulnerability"));
    }

    #[test]
    fn test_audit_multiple_vulnerabilities() {
        let pkgs = vec![
            make_pkg(
                "lodash",
                "4.17.20",
                Some("sha512-abc"),
                Some(1024),
                None,
                vec![],
            ),
            make_pkg(
                "axios",
                "1.5.0",
                Some("sha512-def"),
                Some(2048),
                None,
                vec![],
            ),
        ];
        let result = AuditPlugin::run_audit(&pkgs);
        assert!(!result.success);
        let warnings: Vec<AuditWarning> = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert_eq!(
            warnings
                .iter()
                .filter(|w| w.warning_type == "known-vulnerability")
                .count(),
            2
        );
    }

    #[test]
    fn test_audit_missing_integrity() {
        let pkgs = vec![make_pkg("test-pkg", "1.0.0", None, Some(512), None, vec![])];
        let result = AuditPlugin::run_audit(&pkgs);
        let warnings: Vec<AuditWarning> = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(warnings
            .iter()
            .any(|w| w.warning_type == "missing-integrity"));
    }

    #[test]
    fn test_audit_typosquatting_detected() {
        let pkgs = vec![make_pkg(
            "recat",
            "1.0.0",
            Some("sha512-abc"),
            Some(1024),
            None,
            vec![],
        )];
        let result = AuditPlugin::run_audit(&pkgs);
        let warnings: Vec<AuditWarning> = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(
            warnings.iter().any(|w| w.warning_type == "typosquatting"),
            "recat should be flagged as typo of react"
        );
    }

    #[test]
    fn test_audit_suspicious_name_detected() {
        let pkgs = vec![make_pkg(
            "rnpm-evil",
            "1.0.0",
            Some("sha512-abc"),
            Some(1024),
            None,
            vec![],
        )];
        let result = AuditPlugin::run_audit(&pkgs);
        let warnings: Vec<AuditWarning> = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(
            warnings.iter().any(|w| w.warning_type == "suspicious-name"),
            "rnpm-evil should be flagged as suspicious"
        );
    }

    #[test]
    fn test_audit_empty_packages() {
        let result = AuditPlugin::run_audit(&[]);
        assert!(result.success);
        assert!(result.message.contains("0 warnings"));
    }

    #[test]
    fn test_audit_warning_severity_classification() {
        let pkgs = vec![make_pkg("lodash", "4.17.20", None, Some(512), None, vec![])];
        let result = AuditPlugin::run_audit(&pkgs);
        let warnings: Vec<AuditWarning> = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(warnings.iter().any(|w| w.severity == "high"));
        assert!(warnings.iter().any(|w| w.severity == "low"));
    }

    #[test]
    fn test_audit_result_json_payload() {
        let pkgs = vec![make_pkg("lodash", "4.17.20", None, Some(512), None, vec![])];
        let result = AuditPlugin::run_audit(&pkgs);
        let data = result.data.unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&data).unwrap();
        assert!(!parsed.is_empty());
        assert!(parsed[0].get("package").is_some());
        assert!(parsed[0].get("warning_type").is_some());
        assert!(parsed[0].get("severity").is_some());
        assert!(parsed[0].get("message").is_some());
    }
}

mod license_check_tests {
    use super::*;

    #[test]
    fn test_license_mit_allowed() {
        let plugin = LicenseCheckPlugin::new(vec![]);
        let pkgs = vec![make_pkg(
            "test-pkg",
            "1.0.0",
            Some("sha512"),
            Some(100),
            Some("MIT"),
            vec![],
        )];
        let result = plugin.check_licenses(&pkgs);
        assert!(result.success);
    }

    #[test]
    fn test_license_apache_allowed() {
        let plugin = LicenseCheckPlugin::new(vec![]);
        let pkgs = vec![make_pkg(
            "apache-pkg",
            "1.0.0",
            Some("sha512"),
            Some(100),
            Some("Apache-2.0"),
            vec![],
        )];
        let result = plugin.check_licenses(&pkgs);
        assert!(result.success);
    }

    #[test]
    fn test_license_gpl3_copyleft_detected() {
        let plugin = LicenseCheckPlugin::new(vec![]);
        let pkgs = vec![make_pkg(
            "gpl-pkg",
            "1.0.0",
            Some("sha512"),
            Some(100),
            Some("GPL-3.0"),
            vec![],
        )];
        let result = plugin.check_licenses(&pkgs);
        assert!(!result.success, "Copyleft license should fail");
        let warnings: Vec<LicenseWarning> = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(warnings.iter().any(|w| w.warning_type == "copyleft"));
    }

    #[test]
    fn test_license_agpl_copyleft_detected() {
        let plugin = LicenseCheckPlugin::new(vec![]);
        let pkgs = vec![make_pkg(
            "agpl-pkg",
            "1.0.0",
            Some("sha512"),
            Some(100),
            Some("AGPL-3.0"),
            vec![],
        )];
        let result = plugin.check_licenses(&pkgs);
        assert!(!result.success);
        let warnings: Vec<LicenseWarning> = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(warnings.iter().any(|w| w.warning_type == "copyleft"));
    }

    #[test]
    fn test_license_mpl_copyleft_detected() {
        let plugin = LicenseCheckPlugin::new(vec![]);
        let pkgs = vec![make_pkg(
            "mpl-pkg",
            "1.0.0",
            Some("sha512"),
            Some(100),
            Some("MPL-2.0"),
            vec![],
        )];
        let result = plugin.check_licenses(&pkgs);
        assert!(!result.success);
    }

    #[test]
    fn test_license_missing_detected() {
        let plugin = LicenseCheckPlugin::new(vec![]);
        let pkgs = vec![make_pkg(
            "no-license",
            "1.0.0",
            Some("sha512"),
            Some(100),
            None,
            vec![],
        )];
        let result = plugin.check_licenses(&pkgs);
        let warnings: Vec<LicenseWarning> = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(warnings.iter().any(|w| w.warning_type == "missing-license"));
    }

    #[test]
    fn test_license_unusual_detected() {
        let plugin = LicenseCheckPlugin::new(vec![]);
        let pkgs = vec![make_pkg(
            "weird",
            "1.0.0",
            Some("sha512"),
            Some(100),
            Some("Beerware"),
            vec![],
        )];
        let result = plugin.check_licenses(&pkgs);
        let warnings: Vec<LicenseWarning> = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(warnings.iter().any(|w| w.warning_type == "unusual-license"));
    }

    #[test]
    fn test_license_custom_allowed() {
        let plugin = LicenseCheckPlugin::new(vec!["Beerware".to_string()]);
        let pkgs = vec![make_pkg(
            "weird",
            "1.0.0",
            Some("sha512"),
            Some(100),
            Some("Beerware"),
            vec![],
        )];
        let result = plugin.check_licenses(&pkgs);
        assert!(result.success, "Beerware should pass when in allowlist");
    }

    #[test]
    fn test_license_empty_packages() {
        let plugin = LicenseCheckPlugin::new(vec![]);
        let result = plugin.check_licenses(&[]);
        assert!(result.success);
    }

    #[test]
    fn test_license_case_insensitive() {
        let plugin = LicenseCheckPlugin::new(vec![]);
        let pkgs = vec![make_pkg(
            "mit-pkg",
            "1.0.0",
            Some("sha512"),
            Some(100),
            Some("mit"),
            vec![],
        )];
        let result = plugin.check_licenses(&pkgs);
        assert!(result.success, "Lowercase 'mit' should be allowed");
    }

    #[test]
    fn test_license_mixed_packages() {
        let plugin = LicenseCheckPlugin::new(vec![]);
        let pkgs = vec![
            make_pkg(
                "good",
                "1.0.0",
                Some("sha512"),
                Some(100),
                Some("MIT"),
                vec![],
            ),
            make_pkg(
                "bad",
                "1.0.0",
                Some("sha512"),
                Some(100),
                Some("GPL-3.0"),
                vec![],
            ),
            make_pkg("unknown", "1.0.0", Some("sha512"), Some(100), None, vec![]),
        ];
        let result = plugin.check_licenses(&pkgs);
        assert!(!result.success);
        let warnings: Vec<LicenseWarning> = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert_eq!(warnings.len(), 2);
    }
}

mod size_report_tests {
    use super::*;

    #[test]
    fn test_empty_size_report() {
        let result = SizeReportPlugin::analyze_sizes(&[]);
        assert!(result.success);
        let report: SizeReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert_eq!(report.total_size, 0);
        assert_eq!(report.package_count, 0);
        assert_eq!(report.top_10_largest.len(), 0);
    }

    #[test]
    fn test_single_package_size() {
        let pkgs = vec![make_pkg("tiny", "1.0.0", None, Some(512), None, vec![])];
        let result = SizeReportPlugin::analyze_sizes(&pkgs);
        let report: SizeReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert_eq!(report.total_size, 512);
        assert_eq!(report.package_count, 1);
        assert_eq!(report.size_categories.under_1kb, 1);
    }

    #[test]
    fn test_multiple_packages_cumulative_size() {
        let pkgs = vec![
            make_pkg("a", "1.0.0", None, Some(1000), None, vec![]),
            make_pkg("b", "1.0.0", None, Some(2000), None, vec![]),
            make_pkg("c", "1.0.0", None, Some(3000), None, vec![]),
        ];
        let result = SizeReportPlugin::analyze_sizes(&pkgs);
        let report: SizeReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert_eq!(report.total_size, 6000);
        assert_eq!(report.package_count, 3);
    }

    #[test]
    fn test_excessive_size_detection() {
        let pkgs = vec![
            make_pkg("huge", "1.0.0", None, Some(2_000_000), None, vec![]),
            make_pkg("small", "1.0.0", None, Some(500), None, vec![]),
        ];
        let result = SizeReportPlugin::analyze_sizes(&pkgs);
        let report: SizeReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert_eq!(report.size_categories.over_1mb, 1);
        assert_eq!(report.excessive_size_packages.len(), 1);
        assert_eq!(report.excessive_size_packages[0].name, "huge");
    }

    #[test]
    fn test_unknown_size() {
        let pkgs = vec![make_pkg("unknown", "1.0.0", None, None, None, vec![])];
        let result = SizeReportPlugin::analyze_sizes(&pkgs);
        let report: SizeReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert_eq!(report.size_categories.unknown, 1);
    }

    #[test]
    fn test_top_10_largest_ordering() {
        let mut pkgs: Vec<PackageInfo> = (0..15)
            .map(|i| {
                make_pkg(
                    &format!("pkg-{}", i),
                    "1.0.0",
                    None,
                    Some((15 - i) * 100),
                    None,
                    vec![],
                )
            })
            .collect();
        // Insert a real large one
        pkgs.push(make_pkg(
            "biggest",
            "1.0.0",
            None,
            Some(999_999),
            None,
            vec![],
        ));
        let result = SizeReportPlugin::analyze_sizes(&pkgs);
        let report: SizeReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert_eq!(report.top_10_largest.len(), 10);
        assert_eq!(report.top_10_largest[0].name, "biggest");
    }

    #[test]
    fn test_size_categories_all_levels() {
        let pkgs = vec![
            make_pkg("t1", "1.0.0", None, Some(500), None, vec![]),
            make_pkg("t2", "1.0.0", None, Some(5000), None, vec![]),
            make_pkg("t3", "1.0.0", None, Some(50000), None, vec![]),
            make_pkg("t4", "1.0.0", None, Some(500000), None, vec![]),
            make_pkg("t5", "1.0.0", None, Some(2_000_000), None, vec![]),
        ];
        let result = SizeReportPlugin::analyze_sizes(&pkgs);
        let report: SizeReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert_eq!(report.size_categories.under_1kb, 1);
        assert_eq!(report.size_categories.under_10kb, 1);
        assert_eq!(report.size_categories.under_100kb, 1);
        assert_eq!(report.size_categories.under_1mb, 1);
        assert_eq!(report.size_categories.over_1mb, 1);
    }

    #[test]
    fn test_size_report_json_has_all_fields() {
        let pkgs = vec![make_pkg("p", "1.0.0", None, Some(100), None, vec![])];
        let result = SizeReportPlugin::analyze_sizes(&pkgs);
        let data: serde_json::Value = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(data.get("total_size").is_some());
        assert!(data.get("package_count").is_some());
        assert!(data.get("top_10_largest").is_some());
        assert!(data.get("excessive_size_packages").is_some());
        assert!(data.get("size_categories").is_some());
    }
}

mod dep_graph_tests {
    use super::*;

    #[test]
    fn test_dep_graph_empty() {
        let graph = DepGraph {
            nodes: vec![],
            edges: vec![],
        };
        let result = DepGraphPlugin::generate_graph(&graph);
        assert!(result.success);
        let report: GraphReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(report.circular_dependencies.is_empty());
        assert!(report.degree_counts.is_empty());
        assert_eq!(report.max_depth, 0);
    }

    #[test]
    fn test_dep_graph_simple_chain() {
        let graph = DepGraph {
            nodes: vec!["a".into(), "b".into(), "c".into()],
            edges: vec![("a".into(), "b".into()), ("b".into(), "c".into())],
        };
        let result = DepGraphPlugin::generate_graph(&graph);
        let report: GraphReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(!report.dot_format.is_empty());
        assert_eq!(report.max_depth, 2);
        assert!(report.circular_dependencies.is_empty());
    }

    #[test]
    fn test_dep_graph_circular_detection() {
        let graph = DepGraph {
            nodes: vec!["a".into(), "b".into(), "c".into()],
            edges: vec![
                ("a".into(), "b".into()),
                ("b".into(), "c".into()),
                ("c".into(), "a".into()),
            ],
        };
        let result = DepGraphPlugin::generate_graph(&graph);
        let report: GraphReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(
            !report.circular_dependencies.is_empty(),
            "Expected circular dependency detected"
        );
    }

    #[test]
    fn test_dep_graph_self_loop() {
        let graph = DepGraph {
            nodes: vec!["a".into()],
            edges: vec![("a".into(), "a".into())],
        };
        let result = DepGraphPlugin::generate_graph(&graph);
        let report: GraphReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(
            !report.circular_dependencies.is_empty(),
            "Self-loop should be detected"
        );
    }

    #[test]
    fn test_dep_graph_degree_counts() {
        let graph = DepGraph {
            nodes: vec!["a".into(), "b".into(), "c".into()],
            edges: vec![("a".into(), "b".into()), ("a".into(), "c".into())],
        };
        let result = DepGraphPlugin::generate_graph(&graph);
        let report: GraphReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        let a_deg = report.degree_counts.iter().find(|d| d.name == "a").unwrap();
        assert_eq!(a_deg.out_degree, 2);
        assert_eq!(a_deg.in_degree, 0);
        let b_deg = report.degree_counts.iter().find(|d| d.name == "b").unwrap();
        assert_eq!(b_deg.out_degree, 0);
        assert_eq!(b_deg.in_degree, 1);
    }

    #[test]
    fn test_dep_graph_dot_format() {
        let graph = DepGraph {
            nodes: vec!["a".into(), "b".into()],
            edges: vec![("a".into(), "b".into())],
        };
        let result = DepGraphPlugin::generate_graph(&graph);
        let report: GraphReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(report.dot_format.contains("\"a\" -> \"b\""));
        assert!(report.dot_format.starts_with("digraph G"));
        assert!(report.dot_format.ends_with("}\n"));
    }

    #[test]
    fn test_dep_graph_adjacency_list() {
        let graph = DepGraph {
            nodes: vec!["a".into(), "b".into()],
            edges: vec![("a".into(), "b".into())],
        };
        let result = DepGraphPlugin::generate_graph(&graph);
        let report: GraphReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert_eq!(
            report.adjacency_list.get("a").unwrap(),
            &vec!["b".to_string()]
        );
        assert!(report.adjacency_list.get("b").unwrap().is_empty());
    }

    #[test]
    fn test_dep_graph_max_depth_disconnected() {
        let graph = DepGraph {
            nodes: vec!["a".into(), "b".into()],
            edges: vec![],
        };
        let result = DepGraphPlugin::generate_graph(&graph);
        let report: GraphReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert_eq!(report.max_depth, 0);
    }

    #[test]
    fn test_dep_graph_multi_level_depth() {
        let graph = DepGraph {
            nodes: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            edges: vec![
                ("a".into(), "b".into()),
                ("b".into(), "c".into()),
                ("c".into(), "d".into()),
            ],
        };
        let result = DepGraphPlugin::generate_graph(&graph);
        let report: GraphReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert_eq!(report.max_depth, 3);
    }

    #[test]
    fn test_dep_graph_multiple_circular_groups() {
        let graph = DepGraph {
            nodes: vec!["a".into(), "b".into(), "c".into(), "x".into(), "y".into()],
            edges: vec![
                ("a".into(), "b".into()),
                ("b".into(), "c".into()),
                ("c".into(), "a".into()),
                ("x".into(), "y".into()),
                ("y".into(), "x".into()),
            ],
        };
        let result = DepGraphPlugin::generate_graph(&graph);
        let report: GraphReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(
            report.circular_dependencies.len() >= 2,
            "Expected multiple circular dependencies"
        );
    }
}
