use megagate_types::error::Result;
use std::collections::HashSet;

pub struct SlopsquatDetector {
    official_scopes: HashSet<String>,
    official_registries: HashSet<String>,
}

impl SlopsquatDetector {
    pub fn new() -> Self {
        let mut official_scopes = HashSet::new();
        official_scopes.extend([
            "@types", "@babel", "@angular", "@vue", "@react", "@next",
            "@nestjs", "@prisma", "@supabase", "@vercel", "@aws-sdk",
            "@google-cloud", "@azure", "@microsoft", "@octokit",
        ].iter().map(|s| s.to_string()));

        let mut official_registries = HashSet::new();
        official_registries.extend([
            "registry.npmjs.org",
            "registry.yarnpkg.com",
        ].iter().map(|s| s.to_string()));

        Self {
            official_scopes,
            official_registries,
        }
    }

    pub fn check(&self, name: &str, registry: &str) -> Result<Vec<SlopsquatMatch>> {
        let mut matches = Vec::new();

        if let Some(scope_end) = name.find('/') {
            let scope = &name[..scope_end];
            if scope.starts_with('@') && !self.official_scopes.contains(scope) {
                matches.push(SlopsquatMatch {
                    name: name.to_string(),
                    reason: SlopsquatReason::UnofficialScope,
                    severity: Severity::High,
                    details: format!("Scope '{}' is not in official scopes list", scope),
                });
            }
        }

        if !self.official_registries.contains(registry) {
            matches.push(SlopsquatMatch {
                name: name.to_string(),
                reason: SlopsquatReason::UnofficialRegistry,
                severity: Severity::Medium,
                details: format!("Registry '{}' is not an official registry", registry),
            });
        }

        if name.contains("..") || name.starts_with('.') || name.ends_with('.') {
            matches.push(SlopsquatMatch {
                name: name.to_string(),
                reason: SlopsquatReason::SuspiciousName,
                severity: Severity::High,
                details: "Package name contains suspicious patterns".to_string(),
            });
        }

        Ok(matches)
    }
}

impl Default for SlopsquatDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct SlopsquatMatch {
    pub name: String,
    pub reason: SlopsquatReason,
    pub severity: Severity,
    pub details: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlopsquatReason {
    UnofficialScope,
    UnofficialRegistry,
    SuspiciousName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}