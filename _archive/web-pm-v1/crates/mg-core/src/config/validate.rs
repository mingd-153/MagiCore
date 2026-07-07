use std::path::PathBuf;
use super::{ConfigError, ConfigErrorDetail, MgpmConfig, ScriptConfig};

/// Run all validations on a parsed config
pub fn validate(config: &MgpmConfig) -> Result<(), ConfigError> {
    let mut errors = Vec::new();

    if let Some(ref ws) = config.workspace {
        validate_packages_patterns(ws.packages.as_slice(), &mut errors);
        validate_scripts(&ws.scripts, &mut errors);
        validate_security(&ws.security, &mut errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ConfigError::Validation(errors))
    }
}

fn validate_packages_patterns(patterns: &[String], errors: &mut Vec<ConfigErrorDetail>) {
    for pattern in patterns {
        // Check for basic glob validity using the glob crate's Pattern
        if let Err(e) = glob::Pattern::new(pattern) {
            errors.push(ConfigErrorDetail {
                path: PathBuf::new(),
                message: format!("invalid glob pattern '{}': {}", pattern, e),
                line: None,
                column: None,
                field: Some("workspace.packages".to_string()),
            });
        }

        // Reject empty patterns
        if pattern.is_empty() {
            errors.push(ConfigErrorDetail {
                path: PathBuf::new(),
                message: "empty package pattern".to_string(),
                line: None,
                column: None,
                field: Some("workspace.packages".to_string()),
            });
        }

        // Reject patterns with drive letters on Windows
        #[cfg(windows)]
        if pattern.len() > 1 && pattern.as_bytes()[1] == b':' {
            errors.push(ConfigErrorDetail {
                path: PathBuf::new(),
                message: format!("pattern '{}' looks like an absolute Windows path; use relative paths", pattern),
                line: None,
                column: None,
                field: Some("workspace.packages".to_string()),
            });
        }
    }
}

fn validate_scripts(scripts: &std::collections::HashMap<String, ScriptConfig>, errors: &mut Vec<ConfigErrorDetail>) {
    for (name, script) in scripts {
        // Check for empty script names
        if name.is_empty() {
            errors.push(ConfigErrorDetail {
                path: PathBuf::new(),
                message: "empty script name".to_string(),
                line: None,
                column: None,
                field: Some(format!("workspace.scripts.{}", name)),
            });
        }

        // Detect self-referencing dependencies: depends_on contains own name
        for dep in &script.depends_on {
            let dep_target = dep.trim_start_matches('^');
            if dep_target == name {
                errors.push(ConfigErrorDetail {
                    path: PathBuf::new(),
                    message: format!("script '{}' depends on itself", name),
                    line: None,
                    column: None,
                    field: Some(format!("workspace.scripts.{}.depends_on", name)),
                });
            }
        }
    }

    // Check for circular dependencies using a simple DFS
    let names: Vec<&String> = scripts.keys().collect();
    let mut visited = std::collections::HashSet::new();
    let mut in_stack = std::collections::HashSet::new();

    for name in &names {
        if !visited.contains(*name) {
            let mut path = Vec::new();
            detect_cycles(name, scripts, &mut visited, &mut in_stack, &mut path, errors);
        }
    }
}

fn detect_cycles(
    current: &str,
    scripts: &std::collections::HashMap<String, ScriptConfig>,
    visited: &mut std::collections::HashSet<String>,
    in_stack: &mut std::collections::HashSet<String>,
    path: &mut Vec<String>,
    errors: &mut Vec<ConfigErrorDetail>,
) {
    visited.insert(current.to_string());
    in_stack.insert(current.to_string());
    path.push(current.to_string());

    if let Some(script) = scripts.get(current) {
        for dep in &script.depends_on {
            let dep_target = dep.trim_start_matches('^');
            if !visited.contains(dep_target) {
                detect_cycles(dep_target, scripts, visited, in_stack, path, errors);
            } else if in_stack.contains(dep_target) {
                // Find the cycle in the path
                let cycle_start = path.iter().position(|p| p == dep_target).unwrap_or(0);
                let cycle: Vec<&str> = path[cycle_start..].iter().map(|s| s.as_str()).collect();
                errors.push(ConfigErrorDetail {
                    path: PathBuf::new(),
                    message: format!("circular script dependency: {}", cycle.join(" → ")),
                    line: None,
                    column: None,
                    field: Some("workspace.scripts".to_string()),
                });
            }
        }
    }

    path.pop();
    in_stack.remove(current);
}

fn validate_security(security: &super::SecurityConfig, errors: &mut Vec<ConfigErrorDetail>) {
    // Validate trusted registry URLs
    for url in &security.trusted_registries {
        if !url.starts_with("https://") && !url.starts_with("http://") {
            errors.push(ConfigErrorDetail {
                path: PathBuf::new(),
                message: format!("trusted registry '{}' is not a valid URL (must start with http:// or https://)", url),
                line: None,
                column: None,
                field: Some("workspace.security.trusted_registries".to_string()),
            });
        }
    }

    // Validate min_release_age format (simple check: ends with h, m, s, or d)
    let age = &security.min_release_age;
    if !age.ends_with('h') && !age.ends_with('m') && !age.ends_with('s') && !age.ends_with('d') {
        errors.push(ConfigErrorDetail {
            path: PathBuf::new(),
            message: format!("invalid min_release_age '{}': must end with 'h', 'm', 's', or 'd'", age),
            line: None,
            column: None,
            field: Some("workspace.security.min_release_age".to_string()),
        });
    }
}

#[cfg(test)]
mod tests {
use super::*;
use std::collections::HashMap;

    #[test]
    fn test_validate_valid_config() {
        let config = MgpmConfig {
            workspace: Some(super::super::WorkspaceConfig {
                packages: vec!["packages/*".to_string()],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_validate_invalid_glob() {
        let config = MgpmConfig {
            workspace: Some(super::super::WorkspaceConfig {
                packages: vec!["[invalid".to_string()],
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = validate(&config);
        assert!(result.is_err());
        let details = match result.unwrap_err() {
            ConfigError::Validation(d) => d,
            _ => panic!("expected validation error"),
        };
        assert!(details[0].message.contains("invalid glob"));
    }

    #[test]
    fn test_validate_empty_pattern() {
        let config = MgpmConfig {
            workspace: Some(super::super::WorkspaceConfig {
                packages: vec!["".to_string()],
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = validate(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_self_depending_script() {
        let mut scripts = HashMap::new();
        scripts.insert("build".to_string(), ScriptConfig {
            depends_on: vec!["build".to_string()],
            ..Default::default()
        });
        let config = MgpmConfig {
            workspace: Some(super::super::WorkspaceConfig {
                packages: vec!["packages/*".to_string()],
                scripts,
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = validate(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_circular_scripts() {
        let mut scripts = HashMap::new();
        scripts.insert("build".to_string(), ScriptConfig {
            depends_on: vec!["test".to_string()],
            ..Default::default()
        });
        scripts.insert("test".to_string(), ScriptConfig {
            depends_on: vec!["build".to_string()],
            ..Default::default()
        });
        let config = MgpmConfig {
            workspace: Some(super::super::WorkspaceConfig {
                packages: vec!["packages/*".to_string()],
                scripts,
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = validate(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_security_url() {
        let config = MgpmConfig {
            workspace: Some(super::super::WorkspaceConfig {
                packages: vec!["packages/*".to_string()],
                security: super::super::SecurityConfig {
                    trusted_registries: vec!["not-a-url".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = validate(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_security_age() {
        let config = MgpmConfig {
            workspace: Some(super::super::WorkspaceConfig {
                packages: vec!["packages/*".to_string()],
                security: super::super::SecurityConfig {
                    min_release_age: "foo".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = validate(&config);
        assert!(result.is_err());
    }
}
