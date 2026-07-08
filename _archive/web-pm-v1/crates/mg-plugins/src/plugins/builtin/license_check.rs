use serde::Serialize;

use super::super::{PackageInfo, PluginResult};

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct LicenseWarning {
    pub package: String,
    pub license: Option<String>,
    pub warning_type: String,
    pub severity: String,
    pub message: String,
}

const COPLEFT_LICENSES: &[&str] = &[
    "GPL-2.0",
    "GPL-2.0-only",
    "GPL-2.0-or-later",
    "GPL-3.0",
    "GPL-3.0-only",
    "GPL-3.0-or-later",
    "AGPL-3.0",
    "AGPL-3.0-only",
    "AGPL-3.0-or-later",
    "LGPL-2.0",
    "LGPL-2.0-only",
    "LGPL-2.0-or-later",
    "LGPL-2.1",
    "LGPL-2.1-only",
    "LGPL-2.1-or-later",
    "LGPL-3.0",
    "LGPL-3.0-only",
    "LGPL-3.0-or-later",
    "MPL-2.0",
    "EUPL-1.2",
    "CC-BY-SA-4.0",
];

const DEFAULT_ALLOWED: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unlicense",
    "CC0-1.0",
];

pub struct LicenseCheckPlugin {
    allowed: Vec<String>,
}

impl LicenseCheckPlugin {
    pub fn new(allowed: Vec<String>) -> Self {
        Self { allowed }
    }

    pub fn name(&self) -> &'static str {
        "builtin:license-check"
    }

    pub fn check_licenses(&self, packages: &[PackageInfo]) -> PluginResult {
        let allowed = if self.allowed.is_empty() {
            DEFAULT_ALLOWED
                .iter()
                .map(|s| (*s).to_string())
                .collect::<Vec<_>>()
        } else {
            self.allowed.clone()
        };

        let mut warnings: Vec<LicenseWarning> = Vec::new();

        for pkg in packages {
            let license = pkg.license.as_deref();

            match license {
                None | Some("") | Some("UNKNOWN") => {
                    warnings.push(LicenseWarning {
                        package: format!("{}@{}", pkg.name, pkg.version),
                        license: license.map(|s| s.to_string()),
                        warning_type: "missing-license".into(),
                        severity: "medium".into(),
                        message: format!(
                            "Package '{}@{}' has no declared license",
                            pkg.name, pkg.version
                        ),
                    });
                }
                Some(l) => {
                    let lc = l.trim();

                    // Check copyleft
                    if COPLEFT_LICENSES
                        .iter()
                        .any(|cl| lc.eq_ignore_ascii_case(cl))
                    {
                        warnings.push(LicenseWarning {
                            package: format!("{}@{}", pkg.name, pkg.version),
                            license: Some(lc.to_string()),
                            warning_type: "copyleft".into(),
                            severity: "high".into(),
                            message: format!(
                                "Package '{}@{}' uses copyleft license '{}'",
                                pkg.name, pkg.version, lc
                            ),
                        });
                        continue;
                    }

                    // Check if not in allowlist
                    if !allowed.iter().any(|a| lc.eq_ignore_ascii_case(a)) {
                        warnings.push(LicenseWarning {
                            package: format!("{}@{}", pkg.name, pkg.version),
                            license: Some(lc.to_string()),
                            warning_type: "unusual-license".into(),
                            severity: "low".into(),
                            message: format!(
                                "Package '{}@{}' uses non-standard license '{}'",
                                pkg.name, pkg.version, lc
                            ),
                        });
                    }
                }
            }
        }

        let has_copyleft = warnings.iter().any(|w| w.warning_type == "copyleft");
        let data = serde_json::to_string(&warnings).unwrap_or_default();

        PluginResult {
            success: !has_copyleft,
            message: format!(
                "License check complete: {} warnings ({} copyleft, {} missing, {} unusual)",
                warnings.len(),
                warnings
                    .iter()
                    .filter(|w| w.warning_type == "copyleft")
                    .count(),
                warnings
                    .iter()
                    .filter(|w| w.warning_type == "missing-license")
                    .count(),
                warnings
                    .iter()
                    .filter(|w| w.warning_type == "unusual-license")
                    .count(),
            ),
            data: Some(data),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pkg(name: &str, version: &str, license: Option<&str>) -> PackageInfo {
        PackageInfo {
            id: format!("{}@{}", name, version),
            name: name.to_string(),
            version: version.to_string(),
            dependencies: vec![],
            integrity: None,
            size: None,
            license: license.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_mit_license_allowed() {
        let plugin = LicenseCheckPlugin::new(vec![]);
        let pkgs = vec![make_pkg("test-pkg", "1.0.0", Some("MIT"))];
        let result = plugin.check_licenses(&pkgs);
        assert!(result.success);
    }

    #[test]
    fn test_copyleft_detected() {
        let plugin = LicenseCheckPlugin::new(vec![]);
        let pkgs = vec![make_pkg("gpl-pkg", "1.0.0", Some("GPL-3.0"))];
        let result = plugin.check_licenses(&pkgs);
        assert!(!result.success);
        let data: Vec<LicenseWarning> = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(data.iter().any(|w| w.warning_type == "copyleft"));
    }

    #[test]
    fn test_missing_license() {
        let plugin = LicenseCheckPlugin::new(vec![]);
        let pkgs = vec![make_pkg("no-license", "1.0.0", None)];
        let result = plugin.check_licenses(&pkgs);
        let data: Vec<LicenseWarning> = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(data.iter().any(|w| w.warning_type == "missing-license"));
    }

    #[test]
    fn test_unusual_license() {
        let plugin = LicenseCheckPlugin::new(vec![]);
        let pkgs = vec![make_pkg("weird-pkg", "1.0.0", Some("Beerware"))];
        let result = plugin.check_licenses(&pkgs);
        let data: Vec<LicenseWarning> = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(data.iter().any(|w| w.warning_type == "unusual-license"));
    }
}
