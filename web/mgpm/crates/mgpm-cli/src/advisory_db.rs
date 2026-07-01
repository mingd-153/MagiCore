use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Advisory {
    pub package: String,
    pub ecosystem: String,
    pub severity: String,
    pub description: String,
    pub vulnerable_versions: String,
    pub patched_versions: String,
    pub cve: Option<String>,
    pub ghsa: Option<String>,
    pub published_at: String,
}

pub struct AdvisoryDb {
    builtin: Vec<Advisory>,
    remote: Vec<Advisory>,
    last_fetch: Option<u64>,
}

impl AdvisoryDb {
    pub fn new() -> Self {
        Self {
            builtin: Self::builtin_list(),
            remote: Vec::new(),
            last_fetch: None,
        }
    }

    #[allow(dead_code)]
    pub fn builtin() -> Vec<Advisory> {
        Self::builtin_list()
    }

    fn builtin_list() -> Vec<Advisory> {
        vec![
            Advisory {
                package: "lodash".into(),
                ecosystem: "npm".into(),
                severity: "high".into(),
                description: "Prototype Pollution in lodash".into(),
                vulnerable_versions: "< 4.17.21".into(),
                patched_versions: ">= 4.17.21".into(),
                cve: Some("CVE-2020-28500".into()),
                ghsa: Some("GHSA-xxxx-xxxx-xxxx".into()),
                published_at: "2020-08-01T00:00:00Z".into(),
            },
            Advisory {
                package: "minimist".into(),
                ecosystem: "npm".into(),
                severity: "high".into(),
                description: "Prototype Pollution in minimist".into(),
                vulnerable_versions: "< 1.2.6".into(),
                patched_versions: ">= 1.2.6".into(),
                cve: Some("CVE-2021-44906".into()),
                ghsa: Some("GHSA-yyyy-yyyy-yyyy".into()),
                published_at: "2021-12-01T00:00:00Z".into(),
            },
            Advisory {
                package: "node-fetch".into(),
                ecosystem: "npm".into(),
                severity: "high".into(),
                description: "URL parsing vulnerability in node-fetch".into(),
                vulnerable_versions: "< 2.6.7, < 3.1.1".into(),
                patched_versions: ">= 2.6.7, >= 3.1.1".into(),
                cve: Some("CVE-2022-0235".into()),
                ghsa: Some("GHSA-zzzz-zzzz-zzzz".into()),
                published_at: "2022-01-01T00:00:00Z".into(),
            },
            Advisory {
                package: "cross-fetch".into(),
                ecosystem: "npm".into(),
                severity: "high".into(),
                description: "URL parsing vulnerability in cross-fetch".into(),
                vulnerable_versions: "< 3.1.5".into(),
                patched_versions: ">= 3.1.5".into(),
                cve: Some("CVE-2022-1365".into()),
                ghsa: None,
                published_at: "2022-03-01T00:00:00Z".into(),
            },
            Advisory {
                package: "tmpl".into(),
                ecosystem: "npm".into(),
                severity: "high".into(),
                description: "Prototype Pollution in tmpl".into(),
                vulnerable_versions: "< 1.0.5".into(),
                patched_versions: ">= 1.0.5".into(),
                cve: Some("CVE-2021-3777".into()),
                ghsa: None,
                published_at: "2021-07-01T00:00:00Z".into(),
            },
            Advisory {
                package: "follow-redirects".into(),
                ecosystem: "npm".into(),
                severity: "high".into(),
                description: "Credentials leak via forwarded request headers".into(),
                vulnerable_versions: "< 1.14.8".into(),
                patched_versions: ">= 1.14.8".into(),
                cve: Some("CVE-2022-0536".into()),
                ghsa: None,
                published_at: "2022-02-01T00:00:00Z".into(),
            },
            Advisory {
                package: "semver-regex".into(),
                ecosystem: "npm".into(),
                severity: "high".into(),
                description: "ReDoS via semver regex".into(),
                vulnerable_versions: "< 3.1.4".into(),
                patched_versions: ">= 3.1.4".into(),
                cve: Some("CVE-2021-3795".into()),
                ghsa: None,
                published_at: "2021-06-01T00:00:00Z".into(),
            },
            Advisory {
                package: "ansi-regex".into(),
                ecosystem: "npm".into(),
                severity: "high".into(),
                description: "ReDoS in ansi-regex".into(),
                vulnerable_versions: "< 3.0.1, < 5.0.1, < 6.0.1".into(),
                patched_versions: ">= 3.0.1, >= 5.0.1, >= 6.0.1".into(),
                cve: Some("CVE-2021-3807".into()),
                ghsa: None,
                published_at: "2021-08-01T00:00:00Z".into(),
            },
            Advisory {
                package: "trim-newlines".into(),
                ecosystem: "npm".into(),
                severity: "high".into(),
                description: "ReDoS in trim-newlines".into(),
                vulnerable_versions: "< 3.0.1, < 4.0.1".into(),
                patched_versions: ">= 3.0.1, >= 4.0.1".into(),
                cve: Some("CVE-2021-33623".into()),
                ghsa: None,
                published_at: "2021-07-01T00:00:00Z".into(),
            },
            Advisory {
                package: "glob-parent".into(),
                ecosystem: "npm".into(),
                severity: "low".into(),
                description: "ReDoS in glob-parent".into(),
                vulnerable_versions: "< 5.1.2".into(),
                patched_versions: ">= 5.1.2".into(),
                cve: Some("CVE-2021-35065".into()),
                ghsa: None,
                published_at: "2021-08-01T00:00:00Z".into(),
            },
        ]
    }

    pub async fn fetch_remote(&mut self) -> Result<(), String> {
        let advisories = Self::fetch_remote_inner().await?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.remote = advisories;
        self.last_fetch = Some(now);
        Ok(())
    }

    async fn fetch_remote_inner() -> Result<Vec<Advisory>, String> {
        let client = reqwest::Client::new();
        let resp = client
            .get("https://api.github.com/advisories?ecosystem=npm&per_page=100")
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "mgpm/0.1.0")
            .send()
            .await
            .map_err(|e| format!("network error: {e}"))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(format!("GitHub API returned HTTP {status}"));
        }

        let advisories: Vec<serde_json::Value> =
            resp.json().await.map_err(|e| format!("parse error: {e}"))?;

        let mut result = Vec::new();
        for adv in &advisories {
            let ghsa_id = adv
                .get("ghsa_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let cve_id = adv
                .get("cve_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let summary = adv
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let description = adv
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let severity = adv
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let published_at = adv
                .get("published_at")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if let Some(vulns) = adv.get("vulnerabilities").and_then(|v| v.as_array()) {
                for vuln in vulns {
                    let pkg_name = vuln
                        .get("package")
                        .and_then(|p| p.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let vulnerable_versions = vuln
                        .get("vulnerable_version_range")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let patched_versions = vuln
                        .get("patched_versions")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    result.push(Advisory {
                        package: pkg_name,
                        ecosystem: "npm".to_string(),
                        severity: severity.clone(),
                        description: if description.is_empty() {
                            summary.clone()
                        } else {
                            description.clone()
                        },
                        vulnerable_versions,
                        patched_versions,
                        cve: cve_id.clone(),
                        ghsa: Some(ghsa_id.clone()),
                        published_at: published_at.clone(),
                    });
                }
            }
        }

        Ok(result)
    }

    pub fn check(&self, name: &str, version: &str) -> Vec<&Advisory> {
        let ver = match semver::Version::parse(version) {
            Ok(v) => v,
            Err(_) => return vec![],
        };

        let mut results: Vec<&Advisory> = self
            .builtin
            .iter()
            .filter(|a| a.package == name)
            .filter(|a| is_version_vulnerable(&ver, &a.vulnerable_versions))
            .collect();

        results.extend(self.remote.iter().filter(|a| a.package == name).filter(|a| {
            is_version_vulnerable(&ver, &a.vulnerable_versions)
        }));

        results
    }

    #[allow(dead_code)]
    pub fn builtin_advisories(&self) -> &[Advisory] {
        &self.builtin
    }

    #[allow(dead_code)]
    pub fn remote_advisories(&self) -> &[Advisory] {
        &self.remote
    }

    #[allow(dead_code)]
    pub fn last_fetch(&self) -> Option<u64> {
        self.last_fetch
    }
}

fn is_version_vulnerable(version: &semver::Version, ranges: &str) -> bool {
    if ranges.is_empty() {
        return false;
    }
    ranges.split(',').any(|range| {
        let range = range.trim();
        if range.is_empty() {
            return false;
        }
        semver::VersionReq::parse(range)
            .ok()
            .is_some_and(|req| req.matches(version))
    })
}
