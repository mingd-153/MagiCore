use serde::Serialize;

use super::super::{PackageInfo, PluginResult};

const KNOWN_VULNERABLE: &[(&str, &str, &str, &str)] = &[
    ("lodash", "< 4.17.21", "high", "Prototype Pollution in lodash (CVE-2024-23346)"),
    ("axios", "< 1.6.0", "high", "Server-Side Request Forgery in axios (CVE-2024-39338)"),
    ("express", "< 4.19.2", "moderate", "Path traversal in express static (CVE-2024-29041)"),
    ("minimatch", "< 3.0.5", "high", "ReDoS in minimatch (CVE-2022-3517)"),
    ("node-fetch", "< 2.6.7", "moderate", "URL parsing confusion in node-fetch (CVE-2022-2596)"),
    ("undici", "< 5.19.1", "high", "HTTP request smuggling in undici (CVE-2023-45198)"),
    ("follow-redirects", "< 1.15.4", "moderate", "Credentials leak via URL in follow-redirects (CVE-2024-28849)"),
    ("tar", "< 6.2.1", "high", "Arbitrary file overwrite in tar (CVE-2024-28849)"),
];

const POPULAR_PACKAGES: &[&str] = &[
    "react", "lodash", "express", "axios", "chalk", "commander",
    "typescript", "webpack", "babel", "eslint", "moment", "uuid",
    "body-parser", "cors", "dotenv", "nodemon", "yargs", "inquirer",
    "socket.io", "passport", "mongoose", "redux", "vue", "angular",
    "next", "nuxt", "gatsby", "jest", "mocha", "chai",
];

const SUSPICIOUS_NAME_KEYWORDS: &[&str] = &[
    "rnpm", "npm-", "-npm", "node_", "node-", "js-", "-js",
];

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct AuditWarning {
    pub package: String,
    pub warning_type: String,
    pub severity: String,
    pub message: String,
}

pub struct AuditPlugin;

impl AuditPlugin {
    pub fn name(&self) -> &'static str {
        "builtin:audit"
    }

    pub fn run_audit(packages: &[PackageInfo]) -> PluginResult {
        let mut warnings: Vec<AuditWarning> = Vec::new();

        for pkg in packages {
            let name = &pkg.name;
            let version = &pkg.version;

            // Check known vulnerabilities
            for (vuln_name, vuln_range, severity, desc) in KNOWN_VULNERABLE {
                if name == vuln_name && version_is_affected(version, vuln_range) {
                    warnings.push(AuditWarning {
                        package: format!("{}@{}", name, version),
                        warning_type: "known-vulnerability".into(),
                        severity: severity.to_string(),
                        message: desc.to_string(),
                    });
                }
            }

            // Check missing integrity hash
            if pkg.integrity.as_deref().unwrap_or("").is_empty() {
                warnings.push(AuditWarning {
                    package: format!("{}@{}", name, version),
                    warning_type: "missing-integrity".into(),
                    severity: "low".into(),
                    message: format!("Package {}@{} has no integrity hash", name, version),
                });
            }

            // Check typosquatting
            for popular in POPULAR_PACKAGES {
                if name != *popular && levenshtein_distance(name, popular) <= 2 {
                    warnings.push(AuditWarning {
                        package: format!("{}@{}", name, version),
                        warning_type: "typosquatting".into(),
                        severity: "high".into(),
                        message: format!(
                            "Package '{}' is within Levenshtein distance 2 of popular package '{}'",
                            name, popular
                        ),
                    });
                }
            }

            // Check suspicious name patterns
            for keyword in SUSPICIOUS_NAME_KEYWORDS {
                if name.contains(keyword) && !is_legitimate_package(name) {
                    warnings.push(AuditWarning {
                        package: format!("{}@{}", name, version),
                        warning_type: "suspicious-name".into(),
                        severity: "medium".into(),
                        message: format!(
                            "Package '{}' contains suspicious pattern '{}'",
                            name, keyword
                        ),
                    });
                }
            }
        }

        let has_critical = warnings.iter().any(|w| w.severity == "high");
        let data = serde_json::to_string(&warnings).unwrap_or_default();

        PluginResult {
            success: !has_critical,
            message: format!(
                "Audit complete: {} warnings ({} high, {} medium, {} low)",
                warnings.len(),
                warnings.iter().filter(|w| w.severity == "high").count(),
                warnings.iter().filter(|w| w.severity == "medium").count(),
                warnings.iter().filter(|w| w.severity == "low").count(),
            ),
            data: Some(data),
        }
    }
}

fn version_is_affected(version: &str, range: &str) -> bool {
    let parts: Vec<&str> = range.split_whitespace().collect();
    if parts.len() != 2 {
        return false;
    }
    let op = parts[0];
    let target = parts[1];

    let cmp = |a: &str, b: &str| -> std::cmp::Ordering {
        let a_parts: Vec<u64> = a.split('.').filter_map(|s| s.parse().ok()).collect();
        let b_parts: Vec<u64> = b.split('.').filter_map(|s| s.parse().ok()).collect();
        for (x, y) in a_parts.iter().zip(b_parts.iter()) {
            match x.cmp(y) {
                std::cmp::Ordering::Equal => continue,
                other => return other,
            }
        }
        a_parts.len().cmp(&b_parts.len())
    };

    match op {
        "<" => cmp(version, target).is_lt(),
        "<=" => cmp(version, target).is_le(),
        ">" => cmp(version, target).is_gt(),
        ">=" => cmp(version, target).is_ge(),
        "=" => cmp(version, target).is_eq(),
        _ => false,
    }
}

fn is_legitimate_package(name: &str) -> bool {
    let legit = ["npm", "node-fetch", "node-fs", "js-yaml", "js-tokens"];
    legit.contains(&name)
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    let mut prev: Vec<usize> = (0..=b_len).collect();
    let mut curr: Vec<usize> = vec![0; b_len + 1];

    for i in 1..=a_len {
        curr[0] = i;
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            curr[j] = std::cmp::min(
                std::cmp::min(curr[j - 1] + 1, prev[j] + 1),
                prev[j - 1] + cost,
            );
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b_len]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein_distance("react", "react"), 0);
    }

    #[test]
    fn test_levenshtein_typo() {
        assert_eq!(levenshtein_distance("react", "reactt"), 1);
        assert_eq!(levenshtein_distance("lodash", "lodas"), 1);
    }

    #[test]
    fn test_levenshtein_completely_different() {
        assert!(levenshtein_distance("react", "express") > 2);
    }

    #[test]
    fn test_audit_clean_package() {
        let pkgs = vec![PackageInfo {
            id: "safe-pkg@1.0.0".into(),
            name: "safe-pkg".into(),
            version: "1.0.0".into(),
            dependencies: vec![],
            integrity: Some("sha512-abc".into()),
            size: Some(1024),
            license: None,
        }];
        let result = AuditPlugin::run_audit(&pkgs);
        assert!(result.success);
    }

    #[test]
    fn test_audit_known_vulnerable() {
        let pkgs = vec![PackageInfo {
            id: "lodash@4.17.20".into(),
            name: "lodash".into(),
            version: "4.17.20".into(),
            dependencies: vec![],
            integrity: Some("sha512-xyz".into()),
            size: Some(1024),
            license: None,
        }];
        let result = AuditPlugin::run_audit(&pkgs);
        assert!(!result.success);
        let data: Vec<AuditWarning> = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(data.iter().any(|w| w.warning_type == "known-vulnerability"));
    }

    #[test]
    fn test_audit_typosquatting() {
        let pkgs = vec![PackageInfo {
            id: "recat@1.0.0".into(),
            name: "recat".into(),
            version: "1.0.0".into(),
            dependencies: vec![],
            integrity: Some("sha512-xyz".into()),
            size: Some(1024),
            license: None,
        }];
        let result = AuditPlugin::run_audit(&pkgs);
        let data: Vec<AuditWarning> = serde_json::from_str(&result.data.unwrap()).unwrap();
        assert!(data.iter().any(|w| w.warning_type == "typosquatting"));
    }

    #[test]
    fn test_version_affected() {
        assert!(version_is_affected("4.17.20", "< 4.17.21"));
        assert!(!version_is_affected("4.17.21", "< 4.17.21"));
    }
}
