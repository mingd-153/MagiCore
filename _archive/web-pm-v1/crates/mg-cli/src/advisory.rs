use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Advisory {
    pub package: String,
    pub severity: String,
    pub description: String,
    pub vulnerable_versions: String,
    pub vulnerable_ranges: Vec<String>,
    pub patched_versions: String,
    pub cve: Option<String>,
}

#[derive(Debug)]
pub struct AdvisoryDb {
    advisories: Vec<Advisory>,
}

impl AdvisoryDb {
    pub fn builtin() -> Self {
        Self {
            advisories: vec![
                Advisory {
                    package: "lodash".into(),
                    severity: "high".into(),
                    description: "Prototype Pollution in lodash".into(),
                    vulnerable_versions: "< 4.17.21".into(),
                    vulnerable_ranges: vec!["<4.17.21".into()],
                    patched_versions: ">= 4.17.21".into(),
                    cve: Some("CVE-2020-28500".into()),
                },
                Advisory {
                    package: "minimist".into(),
                    severity: "high".into(),
                    description: "Prototype Pollution in minimist".into(),
                    vulnerable_versions: "< 1.2.6".into(),
                    vulnerable_ranges: vec!["<1.2.6".into()],
                    patched_versions: ">= 1.2.6".into(),
                    cve: Some("CVE-2021-44906".into()),
                },
                Advisory {
                    package: "node-fetch".into(),
                    severity: "high".into(),
                    description: "URL parsing vulnerability in node-fetch".into(),
                    vulnerable_versions: "< 2.6.7, < 3.1.1".into(),
                    vulnerable_ranges: vec!["<2.6.7".into(), "<3.1.1".into()],
                    patched_versions: ">= 2.6.7, >= 3.1.1".into(),
                    cve: Some("CVE-2022-0235".into()),
                },
                Advisory {
                    package: "cross-fetch".into(),
                    severity: "high".into(),
                    description: "URL parsing vulnerability in cross-fetch".into(),
                    vulnerable_versions: "< 3.1.5".into(),
                    vulnerable_ranges: vec!["<3.1.5".into()],
                    patched_versions: ">= 3.1.5".into(),
                    cve: Some("CVE-2022-1365".into()),
                },
                Advisory {
                    package: "tmpl".into(),
                    severity: "high".into(),
                    description: "Prototype Pollution in tmpl".into(),
                    vulnerable_versions: "< 1.0.5".into(),
                    vulnerable_ranges: vec!["<1.0.5".into()],
                    patched_versions: ">= 1.0.5".into(),
                    cve: Some("CVE-2021-3777".into()),
                },
                Advisory {
                    package: "follow-redirects".into(),
                    severity: "high".into(),
                    description: "Credentials leak via forwarded request headers".into(),
                    vulnerable_versions: "< 1.14.8".into(),
                    vulnerable_ranges: vec!["<1.14.8".into()],
                    patched_versions: ">= 1.14.8".into(),
                    cve: Some("CVE-2022-0536".into()),
                },
                Advisory {
                    package: "semver-regex".into(),
                    severity: "high".into(),
                    description: "ReDoS via semver regex".into(),
                    vulnerable_versions: "< 3.1.4".into(),
                    vulnerable_ranges: vec!["<3.1.4".into()],
                    patched_versions: ">= 3.1.4".into(),
                    cve: Some("CVE-2021-3795".into()),
                },
                Advisory {
                    package: "ansi-regex".into(),
                    severity: "high".into(),
                    description: "ReDoS in ansi-regex".into(),
                    vulnerable_versions: "< 3.0.1, < 5.0.1, < 6.0.1".into(),
                    vulnerable_ranges: vec!["<3.0.1".into(), "<5.0.1".into(), "<6.0.1".into()],
                    patched_versions: ">= 3.0.1, >= 5.0.1, >= 6.0.1".into(),
                    cve: Some("CVE-2021-3807".into()),
                },
                Advisory {
                    package: "trim-newlines".into(),
                    severity: "high".into(),
                    description: "ReDoS in trim-newlines".into(),
                    vulnerable_versions: "< 3.0.1, < 4.0.1".into(),
                    vulnerable_ranges: vec!["<3.0.1".into(), "<4.0.1".into()],
                    patched_versions: ">= 3.0.1, >= 4.0.1".into(),
                    cve: Some("CVE-2021-33623".into()),
                },
                Advisory {
                    package: "glob-parent".into(),
                    severity: "low".into(),
                    description: "ReDoS in glob-parent".into(),
                    vulnerable_versions: "< 5.1.2".into(),
                    vulnerable_ranges: vec!["<5.1.2".into()],
                    patched_versions: ">= 5.1.2".into(),
                    cve: Some("CVE-2021-35065".into()),
                },
            ],
        }
    }

    pub fn check(&self, name: &str, version: &str) -> Vec<&Advisory> {
        let ver = match semver::Version::parse(version) {
            Ok(v) => v,
            Err(_) => return vec![],
        };
        self.advisories
            .iter()
            .filter(|a| a.package == name)
            .filter(|a| {
                a.vulnerable_ranges.iter().any(|range| {
                    semver::VersionReq::parse(range)
                        .ok()
                        .map_or(false, |req| req.matches(&ver))
                })
            })
            .collect()
    }

    #[allow(dead_code)]
    pub fn all(&self) -> &[Advisory] {
        &self.advisories
    }
}
