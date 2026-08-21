use super::*;
use std::sync::{Mutex, OnceLock};

fn scaffold_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn lock_scaffold_env() -> std::sync::MutexGuard<'static, ()> {
    scaffold_env_lock()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Registry-first: template layer cần fetch/cache sẵn (~/.mg/templates hoặc
/// MG_TEMPLATES_DIR). Máy sạch offline → skip test materialize.
fn template_layer_ready(rel: &str) -> bool {
    let root = crate::scaffold::template_root::TemplateRoot::resolve(rel);
    root.exists("template.toml") && root.exists("sources")
}

#[test]
fn test_parse_framework_request_supports_alias_and_version() {
    let request = parse_framework_request("react@latest");
    assert_eq!(request.normalized, "react-vite");
    assert_eq!(request.version.as_deref(), Some("latest"));
}

#[test]
fn test_create_web_with_flags_seeds_package_json() {
    if !template_layer_ready("web/frontend/react-vite") {
        eprintln!("skipped: web/frontend/react-vite template layer not available offline (registry-first)");
        return;
    }
    let _guard = lock_scaffold_env();
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("cli-react");
    let flags = ScaffoldFlags {
        ts: true,
        tailwindcss: true,
        ..Default::default()
    };
    std::env::set_var(
            SCAFFOLD_VERSION_OVERRIDES_ENV,
             "vite=^8.1.4,@vitejs/plugin-react=^6.0.3,typescript=^5.9.2,@types/react=^19.2.17,@types/react-dom=^19.2.3,tailwindcss=^4.3.2",
        );

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let result = runtime
        .block_on(run_create_with_options(
            "react@18.3.1",
            &project.to_string_lossy(),
            Some(flags),
        ))
        .map(|_| ());
    std::env::remove_var(SCAFFOLD_VERSION_OVERRIDES_ENV);
    result.unwrap();

    let package_json = std::fs::read_to_string(project.join("package.json")).unwrap();
    assert!(package_json.contains("\"react\": \"18.3.1\""));
    assert!(package_json.contains("\"react-dom\": \"18.3.1\""));
    assert!(package_json.contains("\"vite\": \"^8.1.4\""));
    assert!(package_json.contains("\"@vitejs/plugin-react\": \"^6.0.3\""));
    assert!(package_json.contains("\"tailwindcss\": \"^4.3.2\""));
}

#[test]
fn test_parse_latest_version_response_errors_without_version_field() {
    let err =
        parse_latest_version_response("vite", &serde_json::json!({ "name": "vite" })).unwrap_err();
    assert!(err.to_string().contains("no version field"));
}

#[test]
fn test_scaffold_version_override_short_circuits_network_resolution() {
    let _guard = lock_scaffold_env();
    std::env::set_var(
        SCAFFOLD_VERSION_OVERRIDES_ENV,
        "vite=^8.1.4,tailwindcss=^4.3.2",
    );
    let vite = scaffold_version_override("vite");
    let react = scaffold_version_override("react");
    std::env::remove_var(SCAFFOLD_VERSION_OVERRIDES_ENV);

    assert_eq!(vite.as_deref(), Some("^8.1.4"));
    assert_eq!(react, None);
}

#[test]
fn test_scaffold_baseline_version_covers_core_web_seed_packages() {
    assert_eq!(scaffold_baseline_version("react"), Some("^19.2.7"));
    assert_eq!(scaffold_baseline_version("vue"), Some("^3.5.39"));
    assert_eq!(scaffold_baseline_version("vite"), Some("^8.1.4"));
    assert_eq!(scaffold_baseline_version("next"), Some("^16.2.10"));
    assert_eq!(scaffold_baseline_version("typescript"), Some("^5.9.2"));
    assert_eq!(
        scaffold_baseline_version("@angular-devkit/build-angular"),
        Some("^22.0.6")
    );
    assert_eq!(scaffold_baseline_version("unknown-package"), None);
}

#[test]
fn test_validate_flags_rejects_external_package_manager() {
    let flags = ScaffoldFlags {
        pm: Some("pnpm".to_string()),
        ..Default::default()
    };
    let err = validate_flags(&flags, "react-vite").unwrap_err();
    assert!(
        err.to_string().contains("native mg installer"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_validate_flags_rejects_ts_and_js_together() {
    let flags = ScaffoldFlags {
        ts: true,
        js: true,
        ..Default::default()
    };
    let err = validate_flags(&flags, "react-vite").unwrap_err();
    assert!(
        err.to_string().contains("mutually exclusive"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_create_qwik_uses_framework_specific_vite_pin() {
    if !template_layer_ready("web/frontend/qwik") {
        eprintln!(
            "skipped: web/frontend/qwik template layer not available offline (registry-first)"
        );
        return;
    }
    let _guard = lock_scaffold_env();
    std::env::remove_var(SCAFFOLD_VERSION_OVERRIDES_ENV);

    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("offline-qwik");
    let flags = ScaffoldFlags {
        ts: true,
        ..Default::default()
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(run_create_with_options(
            "qwik",
            &project.to_string_lossy(),
            Some(flags),
        ))
        .unwrap();

    let package_json = std::fs::read_to_string(project.join("package.json")).unwrap();
    assert!(package_json.contains("\"@builder.io/qwik\": \"^1.20.0\""));
    assert!(package_json.contains("\"@builder.io/qwik-city\": \"^1.20.0\""));
    assert!(package_json.contains("\"vite\": \"^7.3.6\""));
    assert!(package_json.contains("\"dev\": \"vite --mode ssr\""));
    assert!(!package_json.contains("\"vite\": \"^8.1.4\""));
}

#[test]
fn test_create_web_without_overrides_uses_curated_baseline_when_registry_is_unavailable() {
    if !template_layer_ready("web/frontend/react-vite") {
        eprintln!("skipped: web/frontend/react-vite template layer not available offline (registry-first)");
        return;
    }
    let _guard = lock_scaffold_env();
    std::env::set_var(
        SCAFFOLD_VERSION_OVERRIDES_ENV,
        "vite=^8.1.4,@vitejs/plugin-react=^6.0.3,typescript=^5.9.2,tailwindcss=^4.3.2",
    );

    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("offline-react");
    let flags = ScaffoldFlags {
        ts: true,
        tailwindcss: true,
        ..Default::default()
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(run_create_with_options(
            "react@18.3.1",
            &project.to_string_lossy(),
            Some(flags),
        ))
        .unwrap();

    let package_json = std::fs::read_to_string(project.join("package.json")).unwrap();
    assert!(package_json.contains("\"react\": \"18.3.1\""));
    assert!(package_json.contains("\"react-dom\": \"18.3.1\""));
    assert!(package_json.contains("\"vite\": \"^8.1.4\""));
    assert!(package_json.contains("\"@vitejs/plugin-react\": \"^6.0.3\""));
    assert!(package_json.contains("\"typescript\": \"^5.9.2\""));
    assert!(package_json.contains("\"tailwindcss\": \"^4.3.2\""));
}

#[test]
fn test_create_vanilla_web_without_primary_dependency_uses_toolchain_seed_only() {
    if !template_layer_ready("web/frontend/vanilla") {
        eprintln!(
            "skipped: web/frontend/vanilla template layer not available offline (registry-first)"
        );
        return;
    }
    let _guard = lock_scaffold_env();
    std::env::remove_var(SCAFFOLD_VERSION_OVERRIDES_ENV);

    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("offline-vanilla");
    let flags = ScaffoldFlags {
        ts: true,
        ..Default::default()
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(run_create_with_options(
            "vanilla",
            &project.to_string_lossy(),
            Some(flags),
        ))
        .unwrap();

    let package_json = std::fs::read_to_string(project.join("package.json")).unwrap();
    assert!(package_json.contains("\"vite\": \"^8.1.4\""));
    assert!(package_json.contains("\"typescript\": \"^5.9.2\""));
    assert!(!package_json.contains("\"vanilla\""));
}

#[test]
fn test_create_nextjs_uses_baseline_typescript_instead_of_registry_latest() {
    if !template_layer_ready("web/frontend/nextjs") {
        eprintln!(
            "skipped: web/frontend/nextjs template layer not available offline (registry-first)"
        );
        return;
    }
    let _guard = lock_scaffold_env();
    std::env::remove_var(SCAFFOLD_VERSION_OVERRIDES_ENV);

    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("offline-next");
    let flags = ScaffoldFlags {
        ts: true,
        ..Default::default()
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(run_create_with_options(
            "nextjs",
            &project.to_string_lossy(),
            Some(flags),
        ))
        .unwrap();

    let package_json = std::fs::read_to_string(project.join("package.json")).unwrap();
    assert!(
        package_json.contains("\"next\": \""),
        "nextjs project should include next as a dependency:\n{package_json}"
    );
    assert!(package_json.contains("\"typescript\": \"^5.9.2\""));
    assert!(package_json.contains("\"@types/node\": \"^26.1.1\""));
}

#[test]
fn test_build_dev_launch_for_vite_adds_host_and_port() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("node_modules/.bin")).unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        serde_json::json!({
            "name": "demo",
            "version": "0.1.0",
            "scripts": { "dev": "vite" }
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(dir.path().join("node_modules/.bin/vite"), "").unwrap();

    let launch = build_dev_launch(dir.path(), "dev", Some("localhost".into()), Some(4315)).unwrap();

    assert!(launch.program.ends_with("vite"));
    assert_eq!(
        launch
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        vec!["--host", "localhost", "--port", "4315"]
    );
}

#[test]
fn test_build_dev_launch_for_vite_does_not_duplicate_host_and_port() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("node_modules/.bin")).unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        serde_json::json!({
            "name": "demo",
            "version": "0.1.0",
            "scripts": { "dev": "vite --host localhost --port 4315" }
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(dir.path().join("node_modules/.bin/vite"), "").unwrap();

    let launch = build_dev_launch(dir.path(), "dev", Some("localhost".into()), Some(4315)).unwrap();

    assert_eq!(
        launch
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        vec!["--host", "localhost", "--port", "4315"]
    );
}

#[test]
fn test_build_dev_launch_rejects_external_package_manager_wrappers() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        serde_json::json!({
            "name": "demo",
            "version": "0.1.0",
            "scripts": { "dev": "npm run dev:inner" }
        })
        .to_string(),
    )
    .unwrap();

    let err =
        build_dev_launch(dir.path(), "dev", Some("localhost".into()), Some(4315)).unwrap_err();
    assert!(err.to_string().contains("delegates to 'npm'"));
}

#[test]
fn test_build_dev_launch_rejects_external_pm_after_separator() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        serde_json::json!({
            "name": "demo",
            "version": "0.1.0",
            "scripts": { "dev": "vite --host localhost --port 4315 && bun install" }
        })
        .to_string(),
    )
    .unwrap();

    let err =
        build_dev_launch(dir.path(), "dev", Some("localhost".into()), Some(4315)).unwrap_err();
    assert!(err.to_string().contains("delegates to 'bun'"));
}

#[test]
fn test_build_dev_launch_falls_back_to_start_for_angular() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("node_modules/.bin")).unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        serde_json::json!({
            "name": "demo",
            "version": "0.1.0",
            "scripts": { "start": "ng serve" }
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(dir.path().join("node_modules/.bin/ng"), "").unwrap();

    let launch = build_dev_launch(dir.path(), "dev", Some("localhost".into()), Some(4315)).unwrap();

    assert!(launch.program.ends_with("ng"));
    assert_eq!(
        launch
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        vec!["serve", "--host", "localhost", "--port", "4315"]
    );
}

#[test]
fn test_build_dev_launch_supports_nuxt_and_astro() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("node_modules/.bin")).unwrap();
    std::fs::write(dir.path().join("node_modules/.bin/nuxt"), "").unwrap();
    std::fs::write(dir.path().join("node_modules/.bin/astro"), "").unwrap();

    std::fs::write(
        dir.path().join("package.json"),
        serde_json::json!({
            "name": "demo",
            "version": "0.1.0",
            "scripts": { "dev": "nuxt dev" }
        })
        .to_string(),
    )
    .unwrap();
    let nuxt_launch =
        build_dev_launch(dir.path(), "dev", Some("localhost".into()), Some(4315)).unwrap();
    assert!(nuxt_launch.program.ends_with("nuxt"));
    assert_eq!(
        nuxt_launch
            .envs
            .iter()
            .map(|(key, value)| (
                key.to_string_lossy().to_string(),
                value.to_string_lossy().to_string()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("NUXT_TELEMETRY_DISABLED".to_string(), "1".to_string()),
            ("NUXT_TELEMETRY_CONSENT".to_string(), "0".to_string()),
        ]
    );

    std::fs::write(
        dir.path().join("package.json"),
        serde_json::json!({
            "name": "demo",
            "version": "0.1.0",
            "scripts": { "dev": "astro dev" }
        })
        .to_string(),
    )
    .unwrap();
    let astro_launch =
        build_dev_launch(dir.path(), "dev", Some("localhost".into()), Some(4315)).unwrap();
    assert!(astro_launch.program.ends_with("astro"));
    assert!(astro_launch.envs.is_empty());

    std::fs::write(
        dir.path().join("package.json"),
        serde_json::json!({
            "name": "demo",
            "version": "0.1.0",
            "scripts": { "dev": "remix vite:dev" }
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(dir.path().join("node_modules/.bin/remix"), "").unwrap();
    let remix_launch =
        build_dev_launch(dir.path(), "dev", Some("localhost".into()), Some(4315)).unwrap();
    assert!(remix_launch.program.ends_with("remix"));
    assert_eq!(
        remix_launch
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        vec![
            "vite:dev".to_string(),
            "--host".to_string(),
            "localhost".to_string(),
            "--port".to_string(),
            "4315".to_string(),
        ]
    );
}

#[test]
fn test_build_dev_launch_supports_native_go_python_and_rust_backends() {
    let go_dir = tempfile::tempdir().unwrap();
    std::fs::write(go_dir.path().join("go.mod"), "module demo\n").unwrap();
    let go_launch = build_dev_launch(go_dir.path(), "dev", None, Some(4401)).unwrap();
    assert_eq!(go_launch.program, PathBuf::from("go"));
    assert_eq!(
        go_launch
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        vec!["run".to_string(), ".".to_string()]
    );

    let py_dir = tempfile::tempdir().unwrap();
    std::fs::write(py_dir.path().join("main.py"), "print('ok')\n").unwrap();
    let py_launch = build_dev_launch(py_dir.path(), "dev", None, Some(4402)).unwrap();
    assert_eq!(py_launch.program, PathBuf::from("python3"));
    assert_eq!(
        py_launch
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        vec!["main.py".to_string()]
    );

    let rust_dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(rust_dir.path().join("src")).unwrap();
    std::fs::write(
        rust_dir.path().join("Cargo.toml"),
        "[package]\nname='demo'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();
    std::fs::write(rust_dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    let rust_launch = build_dev_launch(rust_dir.path(), "dev", None, Some(4403)).unwrap();
    assert_eq!(rust_launch.program, PathBuf::from("cargo"));
    assert_eq!(
        rust_launch
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        vec!["run".to_string()]
    );
}

#[test]
fn test_build_dev_launch_supports_native_symfony_and_quarkus_backends() {
    let symfony_dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(symfony_dir.path().join("public")).unwrap();
    std::fs::create_dir_all(symfony_dir.path().join("bin")).unwrap();
    std::fs::write(symfony_dir.path().join("composer.json"), "{}").unwrap();
    std::fs::write(symfony_dir.path().join("public/index.php"), "<?php\n").unwrap();
    std::fs::write(
        symfony_dir.path().join("bin/console"),
        "#!/usr/bin/env php\n",
    )
    .unwrap();

    let symfony_launch = build_dev_launch(symfony_dir.path(), "dev", None, Some(4315)).unwrap();
    assert_eq!(symfony_launch.program, PathBuf::from("php"));
    assert_eq!(
        symfony_launch
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        vec![
            "-S".to_string(),
            "localhost:4315".to_string(),
            "-t".to_string(),
            "public".to_string(),
            "public/index.php".to_string(),
        ]
    );

    let quarkus_dir = tempfile::tempdir().unwrap();
    std::fs::write(
            quarkus_dir.path().join("pom.xml"),
            r#"<project><properties><quarkus.platform.version>3.6.0</quarkus.platform.version></properties></project>"#,
        )
        .unwrap();
    let quarkus_launch = build_dev_launch(quarkus_dir.path(), "dev", None, Some(4135)).unwrap();
    assert_eq!(quarkus_launch.program, PathBuf::from("mvn"));
    assert_eq!(
        quarkus_launch
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        vec![
            "quarkus:dev".to_string(),
            "-Dquarkus.http.host=localhost".to_string(),
            "-Dquarkus.analytics.disabled=true".to_string(),
            "-Dquarkus.http.port=4135".to_string(),
        ]
    );

    let spring_dir = tempfile::tempdir().unwrap();
    std::fs::write(spring_dir.path().join("pom.xml"), "<project></project>").unwrap();
    let spring_launch = build_dev_launch(
        spring_dir.path(),
        "dev",
        Some("localhost".into()),
        Some(3415),
    )
    .unwrap();
    assert_eq!(spring_launch.program, PathBuf::from("mvn"));
    assert_eq!(
        spring_launch
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        vec![
            "spring-boot:run".to_string(),
            "-Dspring-boot.run.arguments=--server.port=3415 --server.address=localhost".to_string(),
        ]
    );
}

#[test]
fn test_detect_project_mode_supports_non_node_split_backends() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("server")).unwrap();
    std::fs::write(dir.path().join("server/go.mod"), "module demo\n").unwrap();

    let mode = detect_project_mode(dir.path()).unwrap();
    assert!(matches!(mode, WebProjectMode::FullstackSplit));
}

#[test]
fn test_detect_dev_target_prefers_monorepo_frontend_when_root_script_is_mg() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("apps/frontend")).unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        serde_json::json!({
            "name": "root",
            "version": "0.1.0",
            "scripts": { "dev": "mg --core web dev" }
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("megagate.workspace.toml"),
        r#"
mode = "monorepo"

[layout]
apps_dir = "apps"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("apps/frontend/package.json"),
        serde_json::json!({
            "name": "frontend",
            "version": "0.1.0",
            "scripts": { "dev": "vite" }
        })
        .to_string(),
    )
    .unwrap();

    let target = detect_dev_target(dir.path()).unwrap();
    assert_eq!(target, dir.path().join("apps/frontend"));
}

#[test]
fn test_install_targets_cover_fullstack_and_monorepo_children() {
    let fullstack = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(fullstack.path().join("server")).unwrap();
    std::fs::write(fullstack.path().join("package.json"), "{}").unwrap();
    std::fs::write(fullstack.path().join("server/package.json"), "{}").unwrap();

    let fullstack_targets = install_targets(fullstack.path()).unwrap();
    assert_eq!(
        fullstack_targets,
        vec![
            fullstack.path().to_path_buf(),
            fullstack.path().join("server")
        ]
    );

    let monorepo = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(monorepo.path().join("apps/frontend")).unwrap();
    std::fs::create_dir_all(monorepo.path().join("apps/backend")).unwrap();
    std::fs::create_dir_all(monorepo.path().join("packages/contracts")).unwrap();
    std::fs::write(monorepo.path().join("apps/frontend/package.json"), "{}").unwrap();
    std::fs::write(monorepo.path().join("apps/backend/package.json"), "{}").unwrap();
    std::fs::write(
        monorepo.path().join("packages/contracts/package.json"),
        "{}",
    )
    .unwrap();
    std::fs::write(
        monorepo.path().join("megagate.workspace.toml"),
        r#"
mode = "monorepo"

[layout]
apps_dir = "apps"
"#,
    )
    .unwrap();

    let monorepo_targets = install_targets(monorepo.path()).unwrap();
    assert_eq!(
        monorepo_targets,
        vec![
            monorepo.path().join("apps/backend"),
            monorepo.path().join("apps/frontend"),
            monorepo.path().join("packages/contracts")
        ]
    );
}

#[test]
fn test_install_targets_cover_monorepo_native_backend_children() {
    let monorepo = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(monorepo.path().join("apps/frontend")).unwrap();
    std::fs::create_dir_all(monorepo.path().join("apps/backend")).unwrap();
    std::fs::write(monorepo.path().join("apps/frontend/package.json"), "{}").unwrap();
    std::fs::write(
        monorepo.path().join("apps/backend/main.py"),
        "print('backend')\n",
    )
    .unwrap();
    std::fs::write(
        monorepo.path().join("apps/backend/requirements.txt"),
        "fastapi>=0.115.0\n",
    )
    .unwrap();
    std::fs::write(
        monorepo.path().join("megagate.workspace.toml"),
        r#"
mode = "monorepo"

[layout]
apps_dir = "apps"
"#,
    )
    .unwrap();

    let monorepo_targets = install_targets(monorepo.path()).unwrap();
    assert_eq!(
        monorepo_targets,
        vec![
            monorepo.path().join("apps/backend"),
            monorepo.path().join("apps/frontend")
        ]
    );
}

#[test]
fn test_install_targets_detect_workspace_manifest_without_mode_flag() {
    let monorepo = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(monorepo.path().join("apps/frontend")).unwrap();
    std::fs::create_dir_all(monorepo.path().join("apps/backend")).unwrap();
    std::fs::create_dir_all(monorepo.path().join("packages/shared")).unwrap();
    std::fs::write(monorepo.path().join("apps/frontend/package.json"), "{}").unwrap();
    std::fs::write(monorepo.path().join("apps/backend/package.json"), "{}").unwrap();
    std::fs::write(monorepo.path().join("packages/shared/package.json"), "{}").unwrap();
    std::fs::write(
        monorepo.path().join("megagate.workspace.toml"),
        r#"
version = 1

[workspace]
apps = ["apps/*"]
packages = ["packages/*"]
"#,
    )
    .unwrap();

    let targets = install_targets(monorepo.path()).unwrap();
    assert_eq!(
        targets,
        vec![
            monorepo.path().join("apps/backend"),
            monorepo.path().join("apps/frontend"),
            monorepo.path().join("packages/shared"),
        ]
    );
}

#[test]
fn test_monorepo_install_concurrency_is_serial_for_cold_multi_target_install() {
    let monorepo = tempfile::tempdir().unwrap();
    let frontend = monorepo.path().join("apps/frontend");
    let backend = monorepo.path().join("apps/backend");
    std::fs::create_dir_all(&frontend).unwrap();
    std::fs::create_dir_all(&backend).unwrap();
    std::fs::write(frontend.join("package.json"), "{}").unwrap();
    std::fs::write(backend.join("package.json"), "{}").unwrap();

    let targets = vec![frontend, backend];
    assert!(looks_like_cold_monorepo_install(&targets));
    assert_eq!(monorepo_install_concurrency(&targets), 1);
}

#[test]
fn test_monorepo_install_concurrency_reopens_parallelism_after_warmup() {
    let monorepo = tempfile::tempdir().unwrap();
    let frontend = monorepo.path().join("apps/frontend");
    let backend = monorepo.path().join("apps/backend");
    std::fs::create_dir_all(frontend.join("node_modules")).unwrap();
    std::fs::create_dir_all(&backend).unwrap();
    std::fs::write(frontend.join("package.json"), "{}").unwrap();
    std::fs::write(backend.join("package.json"), "{}").unwrap();

    let targets = vec![frontend, backend];
    assert!(!looks_like_cold_monorepo_install(&targets));
    assert!(monorepo_install_concurrency(&targets) >= 1);
}

#[test]
fn test_link_monorepo_workspace_packages_symlinks_workspace_deps() {
    let monorepo = tempfile::tempdir().unwrap();
    let frontend = monorepo.path().join("apps/frontend");
    let shared = monorepo.path().join("packages/shared");
    std::fs::create_dir_all(&frontend).unwrap();
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::write(
        frontend.join("package.json"),
        serde_json::json!({
            "name": "@core/frontend",
            "version": "0.1.0",
            "dependencies": {
                "@core/shared": "workspace:*"
            }
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        shared.join("package.json"),
        serde_json::json!({
            "name": "@core/shared",
            "version": "0.1.0"
        })
        .to_string(),
    )
    .unwrap();

    link_monorepo_workspace_packages(monorepo.path(), &[frontend.clone(), shared.clone()]).unwrap();

    let linked = frontend.join("node_modules").join("@core").join("shared");
    assert!(linked.exists());
    let metadata = std::fs::symlink_metadata(&linked).unwrap();
    assert!(metadata.file_type().is_symlink() || metadata.is_dir());
}

#[test]
fn test_write_monorepo_root_lockfile_aggregates_child_workspace_locks() {
    let monorepo = tempfile::tempdir().unwrap();
    let frontend = monorepo.path().join("apps/frontend");
    let shared = monorepo.path().join("packages/shared");
    std::fs::create_dir_all(&frontend).unwrap();
    std::fs::create_dir_all(&shared).unwrap();

    std::fs::write(
        frontend.join("package.json"),
        serde_json::json!({
            "name": "@core/frontend",
            "version": "0.1.0"
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        shared.join("package.json"),
        serde_json::json!({
            "name": "@core/shared",
            "version": "0.1.0"
        })
        .to_string(),
    )
    .unwrap();

    let mut frontend_lock = Lockfile::new("web", "frontend");
    frontend_lock.frameworks = vec!["react-vite".to_string()];
    frontend_lock.resolution = ResolutionMeta {
        state: "locked".to_string(),
        store: "megagate".to_string(),
        package_count: 2,
    };
    frontend_lock.packages = vec![
        LockPackage {
            name: "react".to_string(),
            version: "19.2.0".to_string(),
            integrity: Some("sha512-react".to_string()),
            direct: true,
            dev: false,
            dependencies: vec![],
            peer_deps: vec![],
        },
        LockPackage {
            name: "@core/shared".to_string(),
            version: "0.1.0".to_string(),
            integrity: None,
            direct: true,
            dev: false,
            dependencies: vec![],
            peer_deps: vec![],
        },
    ];
    let frontend_lock_toml = serialization::to_toml(&frontend_lock).unwrap();
    std::fs::write(frontend.join("mg.lock"), &frontend_lock_toml).unwrap();
    mg_lockfile::write_lockfile_checksum(&frontend, frontend_lock_toml.as_bytes()).unwrap();

    let mut shared_lock = Lockfile::new("web", "package");
    shared_lock.frameworks = vec!["library".to_string()];
    shared_lock.resolution = ResolutionMeta {
        state: "locked".to_string(),
        store: "megagate".to_string(),
        package_count: 1,
    };
    shared_lock.packages = vec![LockPackage {
        name: "zod".to_string(),
        version: "4.0.0".to_string(),
        integrity: Some("sha512-zod".to_string()),
        direct: true,
        dev: false,
        dependencies: vec![],
        peer_deps: vec![],
    }];
    let shared_lock_toml = serialization::to_toml(&shared_lock).unwrap();
    std::fs::write(shared.join("mg.lock"), &shared_lock_toml).unwrap();
    mg_lockfile::write_lockfile_checksum(&shared, shared_lock_toml.as_bytes()).unwrap();

    write_monorepo_root_lockfile(monorepo.path(), &[frontend.clone(), shared.clone()]).unwrap();

    let root_lock = std::fs::read_to_string(monorepo.path().join("mg.lock")).unwrap();
    let parsed: Lockfile = serialization::from_toml(&root_lock).unwrap();
    assert_eq!(parsed.mode, "monorepo");
    assert_eq!(parsed.workspaces.len(), 2);
    assert_eq!(parsed.workspaces[0].path, "apps/frontend");
    assert_eq!(parsed.workspaces[1].path, "packages/shared");
    assert_eq!(parsed.resolution.package_count, 3);
    assert!(parsed.frameworks.contains(&"react-vite".to_string()));
    assert!(parsed.frameworks.contains(&"library".to_string()));
    assert_eq!(parsed.packages.len(), 3);
    assert!(monorepo.path().join("mg.lock.sha256").exists());
}

#[test]
fn test_write_monorepo_root_lockfile_rejects_tampered_child_lock() {
    let monorepo = tempfile::tempdir().unwrap();
    let frontend = monorepo.path().join("apps/frontend");
    std::fs::create_dir_all(&frontend).unwrap();
    std::fs::write(
        frontend.join("package.json"),
        serde_json::json!({
            "name": "@core/frontend",
            "version": "0.1.0"
        })
        .to_string(),
    )
    .unwrap();

    let mut frontend_lock = Lockfile::new("web", "frontend");
    frontend_lock.packages = vec![LockPackage {
        name: "react".to_string(),
        version: "19.2.0".to_string(),
        integrity: Some("sha512-react".to_string()),
        direct: true,
        dev: false,
        dependencies: vec![],
        peer_deps: vec![],
    }];
    let frontend_lock_toml = serialization::to_toml(&frontend_lock).unwrap();
    std::fs::write(frontend.join("mg.lock"), &frontend_lock_toml).unwrap();
    std::fs::write(frontend.join("mg.lock.sha256"), "not-the-real-checksum").unwrap();

    let err = write_monorepo_root_lockfile(monorepo.path(), &[frontend])
        .expect_err("tampered child lockfile must fail monorepo aggregation");
    assert!(
        err.to_string().contains("failed to verify child lockfile"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_native_python_program_prefers_local_venv_layout() {
    let dir = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    let venv_python = dir.path().join(".venv").join("Scripts").join("python.exe");
    #[cfg(not(windows))]
    let venv_python = dir.path().join(".venv").join("bin").join("python");
    std::fs::create_dir_all(venv_python.parent().unwrap()).unwrap();
    std::fs::write(&venv_python, "").unwrap();

    assert_eq!(native_python_program(dir.path()), venv_python);
}

#[test]
fn test_native_pip_program_prefers_local_venv_layout() {
    let dir = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    let venv_pip = dir.path().join(".venv").join("Scripts").join("pip.exe");
    #[cfg(not(windows))]
    let venv_pip = dir.path().join(".venv").join("bin").join("pip");
    std::fs::create_dir_all(venv_pip.parent().unwrap()).unwrap();
    std::fs::write(&venv_pip, "").unwrap();

    assert_eq!(native_pip_program(dir.path()), venv_pip);
}

#[test]
fn test_native_runtime_env_provides_project_local_go_cache_for_dev() {
    let dir = tempfile::tempdir().unwrap();
    let env = native_runtime_env(dir.path(), Path::new("go")).unwrap();

    let env = env.into_iter().collect::<std::collections::HashMap<_, _>>();
    let go_root = dir.path().join(".megagate/cache/go");
    let mod_cache = dir.path().join(".megagate/cache/go/pkg/mod");
    let build_cache = dir.path().join(".megagate/cache/go/build");
    assert_eq!(
        env.get("GOPATH").map(String::as_str),
        Some(go_root.to_string_lossy().as_ref())
    );
    assert_eq!(
        env.get("GOMODCACHE").map(String::as_str),
        Some(mod_cache.to_string_lossy().as_ref())
    );
    assert_eq!(
        env.get("GOCACHE").map(String::as_str),
        Some(build_cache.to_string_lossy().as_ref())
    );
    assert!(dir.path().join(".megagate/cache/go/pkg/mod").exists());
    assert!(dir.path().join(".megagate/cache/go/build").exists());
}

#[test]
fn test_create_web_writes_project_toml_for_monorepo() {
    if !template_layer_ready("web/frontend/react-vite") {
        eprintln!("skipped: web/frontend/react-vite template layer not available offline (registry-first)");
        return;
    }
    let _guard = lock_scaffold_env();
    std::env::remove_var(SCAFFOLD_VERSION_OVERRIDES_ENV);

    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("monorepo-fastapi");
    let flags = ScaffoldFlags {
        ts: true,
        monorepo: true,
        express: true,
        ..Default::default()
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(run_create_with_options(
            "react",
            &project.to_string_lossy(),
            Some(flags),
        ))
        .unwrap();

    let mg_toml = project.join("mg.toml");
    assert!(mg_toml.exists());
    let contents = std::fs::read_to_string(mg_toml).unwrap();
    assert!(contents.contains("ecosystem = \"web\""));
    assert!(contents.contains("[execution]"));
    assert!(contents.contains("architecture = \"rust-first\""));
    assert!(contents.contains("lane = \"compatibility-shell\""));
    assert!(contents.contains("compatibility_layer = \"ts\""));
    assert!(contents.contains("frontend-executable"));
}

#[test]
fn test_dev_targets_for_fullstack_include_frontend_and_backend() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("server")).unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        serde_json::json!({
            "name": "root",
            "version": "0.1.0",
            "scripts": { "dev": "mg --core web dev" }
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("server/package.json"),
        serde_json::json!({
            "name": "backend",
            "version": "0.1.0",
            "scripts": { "dev": "tsx watch src/server.ts" }
        })
        .to_string(),
    )
    .unwrap();

    let targets = dev_targets(dir.path(), Some("localhost".into()), Some(4318)).unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].dir, dir.path());
    assert_eq!(targets[0].port, Some(4318));
    assert_eq!(targets[1].dir, dir.path().join("server"));
    assert_eq!(targets[1].port, Some(3415));
}

#[tokio::test]
async fn test_install_monorepo_targets_topo_parallel_two_apps_shared() {
    let monorepo = tempfile::tempdir().unwrap();
    let frontend = monorepo.path().join("apps/frontend");
    let backend = monorepo.path().join("apps/backend");
    let shared = monorepo.path().join("packages/shared");
    for dir in [&frontend, &backend, &shared] {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(
        frontend.join("package.json"),
        "{\"name\":\"frontend\",\"version\":\"0.1.0\"}",
    )
    .unwrap();
    std::fs::write(
        backend.join("package.json"),
        "{\"name\":\"backend\",\"version\":\"0.1.0\"}",
    )
    .unwrap();
    std::fs::write(
        shared.join("package.json"),
        "{\"name\":\"@core/shared\",\"version\":\"0.1.0\"}",
    )
    .unwrap();

    let adapter = web_adapter();
    install_monorepo_targets(
        &adapter,
        &[frontend.clone(), backend, shared.clone()],
        false,
        false,
        false,
        false,
        false,
    )
    .await
    .unwrap();

    // workspace:* dep trỏ package có manifest → link được (không phải "not found")
    std::fs::write(
        frontend.join("package.json"),
        serde_json::json!({
            "name": "frontend",
            "version": "0.1.0",
            "dependencies": { "@core/shared": "workspace:*" }
        })
        .to_string(),
    )
    .unwrap();
    link_monorepo_workspace_packages(monorepo.path(), &[frontend, shared]).unwrap();
}

#[tokio::test]
async fn test_install_monorepo_targets_cycle_detects_error() {
    let monorepo = tempfile::tempdir().unwrap();
    let a = monorepo.path().join("apps/a");
    let b = monorepo.path().join("apps/b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    std::fs::write(
        a.join("package.json"),
        serde_json::json!({
            "name": "a",
            "dependencies": { "b": "workspace:*" }
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        b.join("package.json"),
        serde_json::json!({
            "name": "b",
            "dependencies": { "a": "workspace:*" }
        })
        .to_string(),
    )
    .unwrap();

    let adapter = web_adapter();
    let err = install_monorepo_targets(&adapter, &[a, b], false, false, false, false, false)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("cycle"),
        "expected cycle error, got: {err}"
    );
}
