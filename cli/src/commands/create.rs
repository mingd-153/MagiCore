use anyhow::Result;
use mg_ui::info;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Default)]
pub struct WebCreateOptions {
    pub typescript: bool,
    pub tailwindcss: bool,
    pub monorepo: bool,
    pub backend: Option<String>,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FrameworkRequest {
    raw: String,
    normalized: String,
    version: Option<String>,
}

/// mg create-<core> — scaffold a new project non-interactively
#[allow(dead_code)]
pub async fn run(core: &str, framework: &str, project_name: &str) -> Result<()> {
    run_with_options(core, framework, project_name, None).await
}

pub async fn run_with_options(
    core: &str,
    framework: &str,
    project_name: &str,
    web: Option<WebCreateOptions>,
) -> Result<()> {
    info(&format!(
        "Creating new {} project '{}' with {}",
        core, project_name, framework
    ));

    let web_options = web.unwrap_or_default();

    let config = if core == "web" {
        build_web_config(framework, project_name, &web_options)?
    } else {
        crate::wizard::engine::ScaffoldConfig {
            core: core.to_string(),
            sub_type: String::new(),
            frameworks: vec![framework.to_string()],
            project_name: project_name.to_string(),
            features: vec![],
            template_dir: std::path::PathBuf::new(),
        }
    };
    let project_dir = crate::scaffold::Scaffolder::scaffold(&config)?;

    let proj_config = mg_config::project::ProjectConfig::new(
        crate::scaffold::Scaffolder::display_name(&project_dir),
        core,
    );
    proj_config.save(&project_dir)?;

    if core == "web" {
        let frontend = parse_framework_request(framework);
        let backend = web_options.backend.as_deref().map(parse_framework_request);
        enrich_web_project_manifest(&project_dir, &frontend, backend.as_ref(), &web_options)?;
    }

    info(&format!("Project '{}' created!", project_dir.display()));
    info(&format!("  cd {} && mg install", project_name));

    Ok(())
}

fn build_web_config(
    framework: &str,
    project_name: &str,
    options: &WebCreateOptions,
) -> Result<crate::wizard::engine::ScaffoldConfig> {
    let frontend = parse_framework_request(framework);

    let mut config = if options.monorepo {
        let backend = options
            .backend
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--monorepo requires --backend <framework>"))?;
        let backend = parse_framework_request(backend);
        crate::wizard::engine::ScaffoldConfig {
            core: "web".to_string(),
            sub_type: "monorepo".to_string(),
            frameworks: vec![frontend.normalized.clone(), backend.normalized],
            project_name: project_name.to_string(),
            features: vec![],
            template_dir: std::path::PathBuf::new(),
        }
    } else {
        crate::scaffold::Scaffolder::infer_web_create_config(&frontend.normalized, project_name)?
    };

    config.features = web_features(options);
    Ok(config)
}

fn web_features(options: &WebCreateOptions) -> Vec<String> {
    let mut features = Vec::new();
    if options.typescript {
        features.push("typescript".to_string());
    }
    if options.tailwindcss {
        features.push("tailwindcss".to_string());
    }
    for feature in &options.features {
        if !features.iter().any(|existing| existing == feature) {
            features.push(feature.clone());
        }
    }
    features
}

fn parse_framework_request(input: &str) -> FrameworkRequest {
    let (framework, version) = match input.rsplit_once('@') {
        Some((name, version)) if !name.is_empty() && !version.is_empty() => {
            (name.to_string(), Some(version.to_string()))
        }
        _ => (input.to_string(), None),
    };

    FrameworkRequest {
        raw: input.to_string(),
        normalized: normalize_cli_web_framework(&framework),
        version,
    }
}

fn normalize_cli_web_framework(framework: &str) -> String {
    match framework {
        "react" | "react-app" => "react-vite".to_string(),
        "vue" | "vue-app" => "vue-vite".to_string(),
        "next" | "next-app" => "nextjs".to_string(),
        "svelte" => "sveltekit".to_string(),
        other => other.to_string(),
    }
}

fn enrich_web_project_manifest(
    project_dir: &std::path::Path,
    frontend: &FrameworkRequest,
    backend: Option<&FrameworkRequest>,
    options: &WebCreateOptions,
) -> Result<()> {
    if options.monorepo {
        apply_web_manifest_seed(
            &project_dir
                .join("apps")
                .join("frontend")
                .join("package.json"),
            frontend,
            options,
        )?;
        if let Some(backend) = backend {
            apply_web_manifest_seed(
                &project_dir
                    .join("apps")
                    .join("backend")
                    .join("package.json"),
                backend,
                options,
            )?;
        }
        return Ok(());
    }

    apply_web_manifest_seed(&project_dir.join("package.json"), frontend, options)
}

fn apply_web_manifest_seed(
    package_json_path: &std::path::Path,
    request: &FrameworkRequest,
    options: &WebCreateOptions,
) -> Result<()> {
    if !package_json_path.exists() {
        return Ok(());
    }

    let mut value: Value = serde_json::from_str(&std::fs::read_to_string(package_json_path)?)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("package.json root must be an object"))?;

    match request.normalized.as_str() {
        "react-vite" => {
            let react_spec = requested_version_or_latest(request);
            ensure_package(object, "dependencies", "react", &react_spec);
            ensure_package(object, "dependencies", "react-dom", &react_spec);
            ensure_package(
                object,
                "devDependencies",
                "vite",
                stable_web_toolchain_version("vite"),
            );
            ensure_package(
                object,
                "devDependencies",
                "@vitejs/plugin-react",
                stable_web_toolchain_version("@vitejs/plugin-react"),
            );
            ensure_package(object, "devDependencies", "typescript", "*");
            ensure_package(object, "devDependencies", "@types/react", "*");
            ensure_package(object, "devDependencies", "@types/react-dom", "*");
        }
        "nextjs" => {
            ensure_package(
                object,
                "dependencies",
                "next",
                &requested_version_or_latest(request),
            );
            ensure_package(object, "dependencies", "react", "*");
            ensure_package(object, "dependencies", "react-dom", "*");
            ensure_package(object, "devDependencies", "typescript", "*");
            ensure_package(object, "devDependencies", "@types/node", "*");
            ensure_package(object, "devDependencies", "@types/react", "*");
            ensure_package(object, "devDependencies", "@types/react-dom", "*");
        }
        "fastify" => {
            ensure_package(
                object,
                "dependencies",
                "fastify",
                &requested_version_or_latest(request),
            );
            ensure_package(object, "devDependencies", "typescript", "*");
            ensure_package(object, "devDependencies", "@types/node", "*");
        }
        "vue-vite" => {
            ensure_package(
                object,
                "dependencies",
                "vue",
                &requested_version_or_latest(request),
            );
            ensure_package(object, "devDependencies", "vite", "*");
            ensure_package(object, "devDependencies", "@vitejs/plugin-vue", "*");
            ensure_package(object, "devDependencies", "typescript", "*");
        }
        _ => {}
    }

    if options.tailwindcss {
        ensure_package(object, "devDependencies", "tailwindcss", "*");
    }

    std::fs::write(package_json_path, serde_json::to_string_pretty(&value)?)?;
    Ok(())
}

fn requested_version_or_latest(request: &FrameworkRequest) -> String {
    match request.version.as_deref() {
        Some("latest") => "*".to_string(),
        Some(version) => version.to_string(),
        None => "*".to_string(),
    }
}

fn ensure_package(root: &mut Map<String, Value>, section: &str, package: &str, version: &str) {
    let entry = root
        .entry(section.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(map) = entry {
        map.insert(package.to_string(), Value::String(version.to_string()));
    }
}

fn stable_web_toolchain_version(package: &str) -> &str {
    match package {
        "vite" => "^7.3.6",
        "@vitejs/plugin-react" => "^5.1.4",
        _ => "*",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_framework_request_supports_alias_and_version() {
        let request = parse_framework_request("react@latest");
        assert_eq!(request.normalized, "react-vite");
        assert_eq!(request.version.as_deref(), Some("latest"));
    }

    #[test]
    fn test_create_web_with_flags_seeds_package_json() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("cli-react");
        let options = WebCreateOptions {
            typescript: true,
            tailwindcss: true,
            monorepo: false,
            backend: None,
            features: vec![],
        };

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime
            .block_on(run_with_options(
                "web",
                "react@latest",
                &project.to_string_lossy(),
                Some(options),
            ))
            .unwrap();

        let package_json = std::fs::read_to_string(project.join("package.json")).unwrap();
        assert!(package_json.contains("\"react\": \"*\""));
        assert!(package_json.contains("\"react-dom\": \"*\""));
        assert!(package_json.contains("\"vite\": \"^7.3.6\""));
        assert!(package_json.contains("\"@vitejs/plugin-react\": \"^5.1.4\""));
        assert!(package_json.contains("\"tailwindcss\": \"*\""));
    }
}
