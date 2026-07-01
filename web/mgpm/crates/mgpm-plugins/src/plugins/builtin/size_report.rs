use serde::Serialize;

use super::super::{PackageInfo, PluginResult};

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct SizeReport {
    pub total_size: i64,
    pub package_count: usize,
    pub top_10_largest: Vec<PackageSize>,
    pub excessive_size_packages: Vec<PackageSize>,
    pub size_categories: SizeCategories,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct PackageSize {
    pub name: String,
    pub version: String,
    pub size_bytes: i64,
    pub size_label: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct SizeCategories {
    pub under_1kb: usize,
    pub under_10kb: usize,
    pub under_100kb: usize,
    pub under_1mb: usize,
    pub over_1mb: usize,
    pub unknown: usize,
}

const EXCESSIVE_THRESHOLD: i64 = 1_048_576; // 1 MB

pub struct SizeReportPlugin;

impl SizeReportPlugin {
    pub fn name(&self) -> &'static str {
        "builtin:size-report"
    }

    pub fn analyze_sizes(packages: &[PackageInfo]) -> PluginResult {
        let package_count = packages.len();
        let mut total_size: i64 = 0;
        let mut packages_with_sizes: Vec<PackageSize> = Vec::new();
        let mut under_1kb: usize = 0;
        let mut under_10kb: usize = 0;
        let mut under_100kb: usize = 0;
        let mut under_1mb: usize = 0;
        let mut over_1mb: usize = 0;
        let mut unknown: usize = 0;

        for pkg in packages {
            match pkg.size {
                Some(size) => {
                    total_size += size;
                    packages_with_sizes.push(PackageSize {
                        name: pkg.name.clone(),
                        version: pkg.version.clone(),
                        size_bytes: size,
                        size_label: format_size(size),
                    });

                    match size {
                        s if s < 1024 => under_1kb += 1,
                        s if s < 10_240 => under_10kb += 1,
                        s if s < 102_400 => under_100kb += 1,
                        s if s < 1_048_576 => under_1mb += 1,
                        _ => over_1mb += 1,
                    }
                }
                None => {
                    unknown += 1;
                }
            }
        }

        packages_with_sizes.sort_by_key(|b| std::cmp::Reverse(b.size_bytes));

        let top_10_largest: Vec<PackageSize> = packages_with_sizes.iter()
            .take(10)
            .cloned()
            .collect();

        let excessive_size_packages: Vec<PackageSize> = packages_with_sizes.iter()
            .filter(|p| p.size_bytes > EXCESSIVE_THRESHOLD)
            .cloned()
            .collect();

        let size_categories = SizeCategories {
            under_1kb,
            under_10kb,
            under_100kb,
            under_1mb,
            over_1mb,
            unknown,
        };

        let report = SizeReport {
            total_size,
            package_count,
            top_10_largest,
            excessive_size_packages,
            size_categories,
        };

        let data = serde_json::to_string(&report).unwrap_or_default();

        PluginResult {
            success: true,
            message: format!(
                "Size report: {} packages, total {}, top package {}, {} packages > 1MB",
                package_count,
                format_size(total_size),
                packages_with_sizes.first().map(|p| format!("{}@{} ({})", p.name, p.version, p.size_label)).unwrap_or_else(|| "N/A".into()),
                over_1mb,
            ),
            data: Some(data),
        }
    }
}

fn format_size(bytes: i64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    format!("{:.2} {}", size, UNITS[unit_idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pkg(name: &str, version: &str, size: Option<i64>) -> PackageInfo {
        PackageInfo {
            id: format!("{}@{}", name, version),
            name: name.to_string(),
            version: version.to_string(),
            dependencies: vec![],
            integrity: None,
            size,
            license: None,
        }
    }

    #[test]
    fn test_empty_report() {
        let result = SizeReportPlugin::analyze_sizes(&[]);
        assert!(result.success);
        let report: SizeReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert_eq!(report.total_size, 0);
        assert_eq!(report.package_count, 0);
    }

    #[test]
    fn test_single_package() {
        let pkgs = vec![make_pkg("tiny", "1.0.0", Some(512))];
        let result = SizeReportPlugin::analyze_sizes(&pkgs);
        let report: SizeReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert_eq!(report.total_size, 512);
        assert_eq!(report.top_10_largest.len(), 1);
        assert_eq!(report.size_categories.under_1kb, 1);
    }

    #[test]
    fn test_excessive_size() {
        let pkgs = vec![make_pkg("huge", "1.0.0", Some(2_000_000))];
        let result = SizeReportPlugin::analyze_sizes(&pkgs);
        let report: SizeReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert_eq!(report.size_categories.over_1mb, 1);
        assert_eq!(report.excessive_size_packages.len(), 1);
    }

    #[test]
    fn test_unknown_size() {
        let pkgs = vec![make_pkg("unknown", "1.0.0", None)];
        let result = SizeReportPlugin::analyze_sizes(&pkgs);
        let report: SizeReport = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert_eq!(report.size_categories.unknown, 1);
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500.00 B");
        assert_eq!(format_size(2048), "2.00 KB");
        assert_eq!(format_size(1_048_576), "1.00 MB");
    }
}
