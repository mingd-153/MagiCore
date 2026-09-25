#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for scaffold template processor

use super::*;

/// Registry-first: template layer cần fetch/cache sẵn (~/.mgc/templates hoặc
/// MGC_TEMPLATES_DIR). Máy sạch offline → skip test materialize.
fn template_layer_ready(rel: &str) -> bool {
    let root = crate::scaffold::template_root::TemplateRoot::resolve(rel);
    root.exists("template.toml") && root.exists("sources")
}

#[test]
fn test_disk_template_root_reads_manifest() {
    use crate::scaffold::template_root::TemplateRoot;
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("sources")).unwrap();
    std::fs::write(dir.path().join("template.toml"), "[files]\n").unwrap();
    let root = TemplateRoot::disk(dir.path().to_path_buf());
    let bytes = root.read("template.toml").unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("files"), "manifest has files");
    assert!(root.exists("sources"), "sources dir visible");
}

#[test]
fn test_scaffold_writes_baseline_for_all_cores() {
    if !template_layer_ready("web/frontend/react-vite") {
        eprintln!(
            "skipped: web/frontend/react-vite template layer not available offline (registry-first)"
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let cases = [
        ("web", "react-vite", "package.json"),
        ("game", "bevy", "Cargo.toml"),
        ("ai", "python-agent", "pyproject.toml"),
        ("clo", "pulumi-aws", "Pulumi.yaml"),
        ("cicd", "github-actions", ".github/workflows/ci.yml"),
        ("iot", "esp32-rust", "Cargo.toml"),
        ("app", "flutter", "pubspec.yaml"),
        ("lib", "rust", "Cargo.toml"),
    ];

    for (core, framework, expected) in cases {
        let project_dir = root.path().join(format!("{core}-{framework}"));
        let config = ScaffoldConfig {
            core: core.to_string(),
            sub_type: String::new(),
            frameworks: vec![framework.to_string()],
            project_name: project_dir.to_string_lossy().to_string(),
            features: vec![],
            template_dir: PathBuf::new(),
        };

        let out = Scaffolder::scaffold(&config).unwrap();
        assert!(out.join(expected).exists(), "{} {}", core, expected);
        assert!(out.join("README.md").exists(), "{} README", core);
        if core == "web" {
            assert!(out.join("mgc.lock").exists(), "web mgc.lock");
            assert!(out.join("mgc.toml").exists(), "web mgc.toml");
        }
    }
}

#[test]
fn test_ai_python_agent_scaffold_is_syntactically_valid() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("ai-agent");

    crate::scaffold::processors::ai::AiProcessor::files(&target, "ai-agent", "python-agent")
        .unwrap();

    let output = std::process::Command::new("python3")
        .args(["-m", "py_compile"])
        .arg(target.join("src/agent.py"))
        .arg(target.join("src/compression.py"))
        .env("PYTHONPYCACHEPREFIX", target.join(".pycache"))
        .output()
        .expect("python3 is required by the AI scaffold lifecycle gate");

    assert!(
        output.status.success(),
        "generated AI Python must compile: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let pyproject = std::fs::read_to_string(target.join("pyproject.toml")).unwrap();
    assert!(
        pyproject.contains("build-backend = \"setuptools.build_meta\""),
        "AI scaffold must support mgc build"
    );
    assert!(
        pyproject.contains("package-dir = {\"\" = \"src\"}"),
        "AI scaffold must package modules from src"
    );
}

#[test]
fn test_flutter_scaffold_has_testable_package_contract() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("app-demo");

    crate::scaffold::processors::app::AppProcessor::files(&target, "app-demo", "flutter").unwrap();

    let pubspec = std::fs::read_to_string(target.join("pubspec.yaml")).unwrap();
    assert!(
        pubspec.contains("environment:"),
        "Flutter SDK range missing"
    );
    assert!(
        pubspec.contains("sdk: flutter"),
        "Flutter SDK dependency missing"
    );
    assert!(
        pubspec.contains("flutter_test:"),
        "Flutter test dependency missing"
    );
    assert!(
        target.join("test/widget_test.dart").exists(),
        "Flutter lifecycle needs a real test target"
    );
    let web_index = std::fs::read_to_string(target.join("web/index.html")).unwrap();
    assert!(web_index.contains("$FLUTTER_BASE_HREF"));
}

#[test]
fn embedded_flutter_scaffold_has_test_and_web_lifecycle_files() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("embedded-flutter");
    crate::scaffold::embedded::EmbeddedKernel::extract_layer("app", "flutter", &target).unwrap();

    let pubspec = std::fs::read_to_string(target.join("pubspec.yaml")).unwrap();
    assert!(pubspec.contains("environment:"));
    assert!(pubspec.contains("sdk: flutter"));
    assert!(
        pubspec.contains("flutter_test:"),
        "embedded Flutter scaffold must include flutter_test"
    );
    assert!(
        target.join("test/widget_test.dart").is_file(),
        "embedded Flutter scaffold must include a real test"
    );
    assert!(
        target.join("web/index.html").is_file(),
        "embedded Flutter scaffold must include web platform files"
    );
    assert!(
        target.join("web/manifest.json").is_file(),
        "embedded Flutter scaffold must include web manifest"
    );
    let web_index = std::fs::read_to_string(target.join("web/index.html")).unwrap();
    assert!(web_index.contains("$FLUTTER_BASE_HREF"));
}

#[test]
fn test_display_name_uses_last_path_segment() {
    let path = Path::new("/tmp/my-project");
    assert_eq!(Scaffolder::display_name(path), "my-project");
}

#[test]
fn test_lib_templates_materialize_all_languages() {
    if !template_layer_ready("lib/ts") {
        eprintln!("skipped: lib/ts template layer not available offline (registry-first)");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    for (language, manifest, marker) in [
        ("ts", "package.json", "\"core\""),
        ("rust", "Cargo.toml", "core = \"lib\""),
        ("python", "pyproject.toml", "core = \"lib\""),
    ] {
        let project_dir = root.path().join(format!("demo-{language}"));
        let config = ScaffoldConfig {
            core: "lib".to_string(),
            sub_type: String::new(),
            frameworks: vec![language.to_string()],
            project_name: project_dir.to_string_lossy().to_string(),
            features: vec![],
            template_dir: PathBuf::new(),
        };

        let out = Scaffolder::scaffold(&config).unwrap();
        assert_eq!(out, project_dir);
        assert!(out.join(manifest).exists(), "{language} manifest");
        let mgc = std::fs::read_to_string(out.join("mgc.toml")).unwrap();
        assert!(
            mgc.contains("ecosystem = \"lib\""),
            "{language} mgc.toml ecosystem"
        );
        assert!(
            mgc.contains(&format!("language = \"{language}\"")),
            "{language} language"
        );
        let native = std::fs::read_to_string(out.join(manifest)).unwrap();
        assert!(native.contains(marker), "{language} marker");
        if language == "ts" {
            assert!(
                native.contains("\"typescript\": \"^5\""),
                "ts scaffold devDeps typescript"
            );
        }
    }
    let py_src = root
        .path()
        .join("demo-python")
        .join("src")
        .join("demo_python")
        .join("__init__.py");
    assert!(py_src.exists(), "python package source");
}

#[test]
fn test_game_templates_materialize_all_engines() {
    if !template_layer_ready("game/bevy") {
        eprintln!("skipped: game/bevy template layer not available offline (registry-first)");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    for (framework, manifest) in [
        ("bevy", "Cargo.toml"),
        ("godot", "project.godot"),
        ("unity", "Packages/manifest.json"),
        ("unreal", "demo-unreal.uproject"),
    ] {
        let project_dir = root.path().join(format!("demo-{framework}"));
        let config = ScaffoldConfig {
            core: "game".to_string(),
            sub_type: String::new(),
            frameworks: vec![framework.to_string()],
            project_name: project_dir.to_string_lossy().to_string(),
            features: vec![],
            template_dir: PathBuf::new(),
        };

        let out = Scaffolder::scaffold(&config).unwrap();
        assert_eq!(out, project_dir);
        assert!(out.join(manifest).exists(), "{framework} manifest");
        let mgc = std::fs::read_to_string(out.join("mgc.toml")).unwrap();
        assert!(
            mgc.contains("ecosystem = \"game\""),
            "{framework} mgc.toml ecosystem"
        );
        assert!(
            mgc.contains(&format!("engine = \"{framework}\"")),
            "{framework} engine"
        );
    }
    let bevy_src = root.path().join("demo-bevy").join("src").join("main.rs");
    assert!(bevy_src.exists(), "bevy source");
}

#[test]
fn test_iot_templates_materialize_all_frameworks() {
    if !template_layer_ready("iot/esp32-rust") {
        eprintln!("skipped: iot/esp32-rust template layer not available offline (registry-first)");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    for (framework, manifest, marker, board, fw_check) in [
        (
            "esp32-rust",
            "Cargo.toml",
            "esp32-hal",
            "esp32c3",
            "esp32-rust",
        ),
        (
            "platformio",
            "platformio.ini",
            "esp32dev",
            "esp32dev",
            "platformio",
        ),
        (
            "zephyr-arm",
            "west.yml",
            "zephyr",
            "nrf52dk_nrf52832",
            "zephyr",
        ),
    ] {
        let project_dir = root.path().join(format!("demo-{framework}"));
        let config = ScaffoldConfig {
            core: "iot".to_string(),
            sub_type: String::new(),
            frameworks: vec![framework.to_string()],
            project_name: project_dir.to_string_lossy().to_string(),
            features: vec![board.to_string()],
            template_dir: PathBuf::new(),
        };

        let out = Scaffolder::scaffold(&config).unwrap();
        assert_eq!(out, project_dir);
        assert!(out.join(manifest).exists(), "{framework} manifest");
        let mgc = std::fs::read_to_string(out.join("mgc.toml")).unwrap();
        assert!(
            mgc.contains("ecosystem = \"iot\""),
            "{framework} mgc.toml ecosystem"
        );
        assert!(
            mgc.contains(&format!("framework = \"{fw_check}\"")),
            "{framework} framework"
        );
        assert!(mgc.contains(board), "{framework} board");
        if framework == "esp32-rust" {
            assert!(
                mgc.contains("riscv32imac-unknown-none-elf"),
                "{framework} target"
            );
        }
        let native = std::fs::read_to_string(out.join(manifest)).unwrap();
        assert!(native.contains(marker), "{framework} marker");
    }
    let esp32_src = root
        .path()
        .join("demo-esp32-rust")
        .join("src")
        .join("main.rs");
    assert!(esp32_src.exists(), "esp32-rust source");
}

#[test]
fn test_optimizer_template_materializes() {
    if !template_layer_ready("hardware/optimizer") {
        eprintln!(
            "skipped: hardware/optimizer template layer not available offline (registry-first)"
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let optimizer_dir = root.path().join("optimizer");
    let config = ScaffoldConfig {
        core: "hardware".to_string(),
        sub_type: String::new(),
        frameworks: vec!["optimizer".to_string()],
        project_name: optimizer_dir.to_string_lossy().to_string(),
        features: vec![],
        template_dir: PathBuf::new(),
    };

    let out = Scaffolder::scaffold(&config).unwrap();
    assert_eq!(out, optimizer_dir);
    assert!(out.join("Cargo.toml").exists(), "optimizer Cargo.toml");
    assert!(out.join("src").join("lib.rs").exists(), "optimizer lib.rs");
    assert!(out.join("build.rs").exists(), "optimizer build.rs");
    assert!(
        out.join("shaders").join("compute.wgsl").exists(),
        "optimizer shader"
    );
    let cargo = std::fs::read_to_string(out.join("Cargo.toml")).unwrap();
    assert!(
        cargo.contains("name = \"mgc-optimizer\""),
        "fixed package name"
    );
    assert!(
        cargo.contains("[workspace]"),
        "workspace opt-out for nested crates"
    );
    let lib = std::fs::read_to_string(out.join("src").join("lib.rs")).unwrap();
    assert!(lib.contains("mgc_optimizer_init"), "FFI init export");
    assert!(
        lib.contains("mgc_optimizer_optimize_mesh"),
        "FFI mesh export"
    );
}

#[test]
fn test_web_monorepo_uses_template_layers() {
    if !["web/frontend/react-vite", "web/backend/node/fastify"]
        .iter()
        .all(|rel| template_layer_ready(rel))
    {
        eprintln!("skipped: web template layers not available offline (registry-first)");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let project_dir = root.path().join("web-monorepo");
    let config = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "monorepo".to_string(),
        frameworks: vec!["react-vite".to_string(), "fastify".to_string()],
        project_name: project_dir.to_string_lossy().to_string(),
        features: vec!["schema".to_string()],
        template_dir: PathBuf::new(),
    };

    let out = Scaffolder::scaffold(&config).unwrap();
    assert!(out.join("mgc.lock").exists());
    assert!(out.join("mgc.toml").exists());
    assert!(out.join("magicore.workspace.toml").exists());
    let root_package = std::fs::read_to_string(out.join("package.json")).unwrap();
    assert!(root_package.contains("\"dev\": \"mgc --core web dev\""));
    assert!(!root_package.contains("mgc web build"));
    assert!(!root_package.contains("mgc web check"));
    let readme = std::fs::read_to_string(out.join("README.md")).unwrap();
    assert!(readme.contains("mgc install"));
    assert!(out.join("apps").join("frontend").join("README.md").exists());
    assert!(out.join("apps").join("backend").join("README.md").exists());
    assert!(out.join("packages").join("README.md").exists());
    assert!(
        out.join("apps")
            .join("frontend")
            .join("crates")
            .join("engine")
            .join("Cargo.toml")
            .exists()
    );
    assert!(
        out.join("apps")
            .join("frontend")
            .join("src")
            .join("bridges")
            .join("engine.js")
            .exists()
    );
    assert!(
        out.join("packages")
            .join("contracts")
            .join("package.json")
            .exists()
    );
    assert!(
        out.join("apps")
            .join("frontend")
            .join("vite.config.js")
            .exists()
    );
    assert!(
        out.join("apps")
            .join("backend")
            .join("src")
            .join("server.js")
            .exists()
    );
}

#[test]
fn test_fullstack_axum_falls_back_to_monorepo_composite() {
    if ![
        "web/frontend/react-vite",
        "web/monorepo/base",
        "web/monorepo/frontend/react-vite",
        "web/monorepo/backend/rust/axum",
    ]
    .iter()
    .all(|rel| template_layer_ready(rel))
    {
        eprintln!("skipped: web template layers not available offline (registry-first)");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let project_dir = root.path().join("react-axum-app");
    let config = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "fullstack".to_string(),
        frameworks: vec!["react-axum".to_string()],
        project_name: project_dir.to_string_lossy().to_string(),
        features: vec![],
        template_dir: PathBuf::new(),
    };

    let out = Scaffolder::scaffold(&config).unwrap();
    assert!(out.join("magicore.workspace.toml").exists());
    assert!(
        out.join("apps")
            .join("frontend")
            .join("package.json")
            .exists()
    );
    let back_cargo =
        std::fs::read_to_string(out.join("apps").join("backend").join("Cargo.toml")).unwrap();
    assert!(
        back_cargo.contains("axum"),
        "backend Cargo.toml should pin axum, got: {back_cargo}"
    );
    assert!(
        out.join("apps")
            .join("backend")
            .join("src")
            .join("main.rs")
            .exists()
    );
    assert!(
        !out.join("templates")
            .join("web")
            .join("fullstack")
            .join("split")
            .join("react-axum")
            .exists(),
        "no hardcoded split leaf was added"
    );
}

#[test]
fn test_fullstack_gin_falls_back_to_monorepo_composite() {
    if ![
        "web/frontend/react-vite",
        "web/monorepo/base",
        "web/monorepo/frontend/react-vite",
        "web/monorepo/backend/go/gin",
    ]
    .iter()
    .all(|rel| template_layer_ready(rel))
    {
        eprintln!("skipped: web template layers not available offline (registry-first)");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let project_dir = root.path().join("react-gin-app");
    let config = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "fullstack".to_string(),
        frameworks: vec!["react-gin".to_string()],
        project_name: project_dir.to_string_lossy().to_string(),
        features: vec![],
        template_dir: PathBuf::new(),
    };

    let out = Scaffolder::scaffold(&config).unwrap();
    assert!(out.join("magicore.workspace.toml").exists());
    assert!(out.join("apps").join("backend").join("go.mod").exists());
    let go_mod = std::fs::read_to_string(out.join("apps").join("backend").join("go.mod")).unwrap();
    assert!(
        go_mod.contains("gin"),
        "backend go.mod should pin gin, got: {go_mod}"
    );
}

#[test]
fn test_web_leaf_templates_materialize_framework_specific_files() {
    if ![
        "web/frontend/react-vite",
        "web/frontend/nextjs",
        "web/frontend/vue-vite",
        "web/frontend/vanilla",
        "web/frontend/solidjs",
        "web/fullstack/split/react-express",
    ]
    .iter()
    .all(|rel| template_layer_ready(rel))
    {
        eprintln!("skipped: web template layers not available offline (registry-first)");
        return;
    }
    let root = tempfile::tempdir().unwrap();

    let react_dir = root.path().join("react-vite-app");
    let react = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "frontend".to_string(),
        frameworks: vec!["react-vite".to_string()],
        project_name: react_dir.to_string_lossy().to_string(),
        features: vec![],
        template_dir: PathBuf::new(),
    };
    let react_out = Scaffolder::scaffold(&react).unwrap();
    assert!(react_out.join("package.json").exists());
    assert!(react_out.join("vite.config.js").exists());
    assert!(react_out.join("index.html").exists());
    assert!(
        react_out
            .join("crates")
            .join("engine")
            .join("Cargo.toml")
            .exists()
    );
    assert!(react_out.join("src").join("main.jsx").exists());
    assert!(react_out.join("src").join("App.jsx").exists());
    assert!(
        react_out
            .join("src")
            .join("bridges")
            .join("engine.js")
            .exists()
    );
    assert!(
        react_out
            .join("src")
            .join("styles")
            .join("theme.css")
            .exists()
    );
    assert!(
        react_out
            .join("src")
            .join("assets")
            .join("magicore-grid.svg")
            .exists()
    );
    assert!(!react_out.join("tsconfig.json").exists());

    let next_dir = root.path().join("next-app");
    let next = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "frontend".to_string(),
        frameworks: vec!["nextjs".to_string()],
        project_name: next_dir.to_string_lossy().to_string(),
        features: vec![],
        template_dir: PathBuf::new(),
    };
    let next_out = Scaffolder::scaffold(&next).unwrap();
    assert!(next_out.join("next.config.mjs").exists());
    assert!(
        next_out
            .join("crates")
            .join("engine")
            .join("Cargo.toml")
            .exists()
    );
    assert!(next_out.join("src").join("app").join("page.jsx").exists());
    assert!(
        next_out
            .join("src")
            .join("bridges")
            .join("engine.js")
            .exists()
    );
    assert!(next_out.join("jsconfig.json").exists());
    assert!(!next_out.join("src").join("main.tsx").exists());

    let vue_dir = root.path().join("vue-app");
    let vue = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "frontend".to_string(),
        frameworks: vec!["vue-vite".to_string()],
        project_name: vue_dir.to_string_lossy().to_string(),
        features: vec!["typescript".to_string()],
        template_dir: PathBuf::new(),
    };
    let vue_out = Scaffolder::scaffold(&vue).unwrap();
    assert!(vue_out.join("package.json").exists());
    assert!(vue_out.join("vite.config.ts").exists());
    assert!(vue_out.join("src").join("main.ts").exists());
    assert!(vue_out.join("src").join("App.vue").exists());
    assert!(
        vue_out
            .join("src")
            .join("components")
            .join("AppShell.vue")
            .exists()
    );
    assert!(
        vue_out
            .join("src")
            .join("router")
            .join("AppRouter.vue")
            .exists()
    );
    assert!(
        vue_out
            .join("src")
            .join("hooks")
            .join("useProjectLinks.ts")
            .exists()
    );
    assert!(
        !vue_out
            .join("src")
            .join("components")
            .join("AppShell.tsx")
            .exists()
    );

    let vanilla_dir = root.path().join("vanilla-app");
    let vanilla = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "frontend".to_string(),
        frameworks: vec!["vanilla".to_string()],
        project_name: vanilla_dir.to_string_lossy().to_string(),
        features: vec!["typescript".to_string()],
        template_dir: PathBuf::new(),
    };
    let vanilla_out = Scaffolder::scaffold(&vanilla).unwrap();
    assert!(vanilla_out.join("package.json").exists());
    assert!(vanilla_out.join("vite.config.ts").exists());
    assert!(vanilla_out.join("src").join("main.ts").exists());
    assert!(vanilla_out.join("src").join("App.ts").exists());
    assert!(
        vanilla_out
            .join("src")
            .join("components")
            .join("AppShell.ts")
            .exists()
    );
    assert!(
        vanilla_out
            .join("src")
            .join("router")
            .join("AppRouter.ts")
            .exists()
    );
    assert!(!vanilla_out.join("src").join("main.tsx").exists());

    let react_express_dir = root.path().join("react-express-app");
    let react_express = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "fullstack".to_string(),
        frameworks: vec!["react-express".to_string()],
        project_name: react_express_dir.to_string_lossy().to_string(),
        features: vec!["typescript".to_string()],
        template_dir: PathBuf::new(),
    };
    let react_express_out = Scaffolder::scaffold(&react_express).unwrap();
    assert!(react_express_out.join("package.json").exists());
    assert!(react_express_out.join("vite.config.ts").exists());
    assert!(react_express_out.join("src").join("main.tsx").exists());
    assert!(
        react_express_out
            .join("src")
            .join("styles")
            .join("theme.css")
            .exists()
    );
    assert!(
        react_express_out
            .join("server")
            .join("src")
            .join("server.ts")
            .exists()
    );

    let solid_dir = root.path().join("solid-app");
    let solid = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "frontend".to_string(),
        frameworks: vec!["solidjs".to_string()],
        project_name: solid_dir.to_string_lossy().to_string(),
        features: vec!["typescript".to_string()],
        template_dir: PathBuf::new(),
    };
    let solid_out = Scaffolder::scaffold(&solid).unwrap();
    assert!(solid_out.join("package.json").exists());
    assert!(solid_out.join("vite.config.ts").exists());
    assert!(solid_out.join("src").join("main.tsx").exists());
    assert!(solid_out.join("src").join("App.tsx").exists());
    assert!(
        solid_out
            .join("src")
            .join("components")
            .join("AppShell.tsx")
            .exists()
    );
    assert!(
        solid_out
            .join("src")
            .join("router")
            .join("AppRouter.tsx")
            .exists()
    );

    let fastify_dir = root.path().join("fastify-api");
    let fastify = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "backend".to_string(),
        frameworks: vec!["node".to_string(), "fastify".to_string()],
        project_name: fastify_dir.to_string_lossy().to_string(),
        features: vec![],
        template_dir: PathBuf::new(),
    };
    let fastify_out = Scaffolder::scaffold(&fastify).unwrap();
    assert!(fastify_out.join("src").join("server.js").exists());
    assert!(
        fastify_out
            .join("src")
            .join("config")
            .join("app.js")
            .exists()
    );
    assert!(
        fastify_out
            .join("src")
            .join("routes")
            .join("health.js")
            .exists()
    );
    assert!(
        fastify_out
            .join("src")
            .join("services")
            .join("status.js")
            .exists()
    );
    assert!(!fastify_out.join("tsconfig.json").exists());
}

#[test]
fn test_web_typescript_feature_switches_extensions() {
    if !template_layer_ready("web/frontend/react-vite") {
        eprintln!(
            "skipped: web/frontend/react-vite template layer not available offline (registry-first)"
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let project_dir = root.path().join("react-ts");
    let config = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "frontend".to_string(),
        frameworks: vec!["react-vite".to_string()],
        project_name: project_dir.to_string_lossy().to_string(),
        features: vec!["typescript".to_string()],
        template_dir: PathBuf::new(),
    };

    let out = Scaffolder::scaffold(&config).unwrap();
    assert!(out.join("tsconfig.json").exists());
    assert!(out.join("vite.config.ts").exists());
    assert!(
        out.join("crates")
            .join("engine")
            .join("Cargo.toml")
            .exists()
    );
    assert!(out.join("src").join("main.tsx").exists());
    assert!(out.join("src").join("App.tsx").exists());
    assert!(out.join("src").join("bridges").join("engine.ts").exists());
    assert!(!out.join("src").join("main.jsx").exists());
}

#[test]
fn test_unknown_frameworks_fail_fast() {
    let root = tempfile::tempdir().unwrap();

    let unsupported_dir = root.path().join("ember-app");
    let unsupported = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "frontend".to_string(),
        frameworks: vec!["ember".to_string()],
        project_name: unsupported_dir.to_string_lossy().to_string(),
        features: vec!["typescript".to_string()],
        template_dir: PathBuf::new(),
    };
    let unsupported_err = Scaffolder::scaffold(&unsupported).unwrap_err();
    assert!(unsupported_err.to_string().contains("Web template path"));

    let broken_mono_dir = root.path().join("broken-mono");
    let broken_mono = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "monorepo".to_string(),
        frameworks: vec!["ember".to_string(), "fastify".to_string()],
        project_name: broken_mono_dir.to_string_lossy().to_string(),
        features: vec!["typescript".to_string()],
        template_dir: PathBuf::new(),
    };
    let broken_mono_err = Scaffolder::scaffold(&broken_mono).unwrap_err();
    assert!(
        broken_mono_err
            .to_string()
            .contains("Scaffold for monorepo frontend framework 'ember' is not implemented yet")
    );
}

#[test]
fn test_web_feature_gated_templates_materialize_only_when_active() {
    if !template_layer_ready("web/frontend/nextjs") {
        eprintln!(
            "skipped: web/frontend/nextjs template layer not available offline (registry-first)"
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();

    // Next.js with prisma + tailwindcss + eslint + prettier + vitest
    let with_features = root.path().join("next-features");
    let config = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "frontend".to_string(),
        frameworks: vec!["nextjs".to_string()],
        project_name: with_features.to_string_lossy().to_string(),
        features: vec![
            "typescript".to_string(),
            "prisma".to_string(),
            "tailwindcss".to_string(),
            "daisyui".to_string(),
            "eslint".to_string(),
            "prettier".to_string(),
            "vitest".to_string(),
        ],
        template_dir: PathBuf::new(),
    };
    let out = Scaffolder::scaffold(&config).unwrap();
    assert!(out.join("prisma").join("schema.prisma").exists());
    assert!(out.join("tailwind.config.ts").exists());
    assert!(out.join("postcss.config.mjs").exists());
    assert!(out.join(".eslintrc.json").exists());
    assert!(out.join(".prettierrc").exists());
    assert!(out.join("vitest.config.ts").exists());

    // Next.js without features — feature files must NOT exist
    let no_features = root.path().join("next-bare");
    let config_bare = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "frontend".to_string(),
        frameworks: vec!["nextjs".to_string()],
        project_name: no_features.to_string_lossy().to_string(),
        features: vec!["typescript".to_string()],
        template_dir: PathBuf::new(),
    };
    let out_bare = Scaffolder::scaffold(&config_bare).unwrap();
    assert!(!out_bare.join("prisma").join("schema.prisma").exists());
    assert!(!out_bare.join("tailwind.config.ts").exists());
    assert!(!out_bare.join(".eslintrc.json").exists());
    assert!(!out_bare.join(".prettierrc").exists());
    assert!(!out_bare.join("vitest.config.ts").exists());
}

#[test]
fn test_web_docker_templates_materialize_in_base_layer() {
    if !template_layer_ready("web/frontend/nextjs") {
        eprintln!(
            "skipped: web/frontend/nextjs template layer not available offline (registry-first)"
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();

    // Frontend with docker feature
    let docker_dir = root.path().join("docker-app");
    let config = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "frontend".to_string(),
        frameworks: vec!["nextjs".to_string()],
        project_name: docker_dir.to_string_lossy().to_string(),
        features: vec!["docker".to_string()],
        template_dir: PathBuf::new(),
    };
    let out = Scaffolder::scaffold(&config).unwrap();
    assert!(out.join("Dockerfile").exists());
    assert!(out.join("docker-compose.yml").exists());
    assert!(out.join(".dockerignore").exists());

    // Without docker — no docker files
    let no_docker = root.path().join("no-docker");
    let config_no = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "frontend".to_string(),
        frameworks: vec!["nextjs".to_string()],
        project_name: no_docker.to_string_lossy().to_string(),
        features: vec!["typescript".to_string()],
        template_dir: PathBuf::new(),
    };
    let out_no = Scaffolder::scaffold(&config_no).unwrap();
    assert!(!out_no.join("Dockerfile").exists());
    assert!(!out_no.join("docker-compose.yml").exists());
}

#[test]
fn test_web_postgres_env_template_materializes_with_feature() {
    if !template_layer_ready("web/frontend/nextjs") {
        eprintln!(
            "skipped: web/frontend/nextjs template layer not available offline (registry-first)"
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();

    let pg_dir = root.path().join("pg-app");
    let config = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "frontend".to_string(),
        frameworks: vec!["nextjs".to_string()],
        project_name: pg_dir.to_string_lossy().to_string(),
        features: vec!["postgres".to_string()],
        template_dir: PathBuf::new(),
    };
    let out = Scaffolder::scaffold(&config).unwrap();
    assert!(out.join(".env").exists());

    let no_pg = root.path().join("no-pg");
    let config_no = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "frontend".to_string(),
        frameworks: vec!["nextjs".to_string()],
        project_name: no_pg.to_string_lossy().to_string(),
        features: vec![],
        template_dir: PathBuf::new(),
    };
    let out_no = Scaffolder::scaffold(&config_no).unwrap();
    assert!(!out_no.join(".env").exists());
}

#[test]
fn test_multi_app_scaffold_writes_shared_and_all_platforms() {
    let root = tempfile::tempdir().unwrap();
    let project_dir = root.path().join("demo-multi");
    let config = ScaffoldConfig {
        core: "app".to_string(),
        sub_type: String::new(),
        frameworks: vec!["multi".to_string()],
        project_name: project_dir.to_string_lossy().to_string(),
        features: vec![],
        template_dir: PathBuf::new(),
    };

    let out = Scaffolder::scaffold(&config).unwrap();
    let proj_config = mgc_config::project::ProjectConfig::from_scaffold(
        Scaffolder::display_name(&out),
        "app",
        "",
        config.frameworks.clone(),
        "",
        config.features.clone(),
    );
    proj_config.save(&out).unwrap();
    for expected in [
        "mgc.toml",
        "shared/build.gradle.kts",
        "shared/src/commonMain/kotlin/demo_multi/Shared.kt",
        "android/build.gradle.kts",
        "android/app/build.gradle.kts",
        "android/settings.gradle.kts",
        "android/app/src/main/kotlin/Main.kt",
        "ios/Package.swift",
        "ios/Sources/demo-multi/main.swift",
        "ios/ObjcBridge/ObjcBridge.h",
        "ios/ObjcBridge/ObjcBridge.m",
        "react-native/package.json",
        "react-native/App.js",
        "flutter/pubspec.yaml",
        "flutter/lib/main.dart",
    ] {
        assert!(out.join(expected).exists(), "missing {expected}");
    }
    let shared_build = std::fs::read_to_string(out.join("shared/build.gradle.kts")).unwrap();
    assert!(
        shared_build.contains("baseName = \"demo-multi\""),
        "shared framework baseName"
    );
    let android_build = std::fs::read_to_string(out.join("android/app/build.gradle.kts")).unwrap();
    assert!(
        android_build.contains("implementation(project(\":shared\"))"),
        "android depends on shared"
    );
    let android_settings =
        std::fs::read_to_string(out.join("android/settings.gradle.kts")).unwrap();
    assert!(
        android_settings.contains("include(\":app\", \":shared\")"),
        "android includes shared"
    );
    let mgc = std::fs::read_to_string(out.join("mgc.toml")).unwrap();
    assert!(mgc.contains("ecosystem = \"app\""), "app ecosystem");
    assert!(mgc.contains("language = \"multi\""), "multi language");
    for platform in ["android", "ios", "react-native", "flutter"] {
        assert!(
            mgc.contains(&format!("\"{platform}\"")),
            "platform {platform}"
        );
    }
}

// ===== Atomic scaffold regression (release contract §5 "fail atomically") =====
// Regression cho hợp đồng atomic scaffold: project phải xuất hiện nguyên vẹn
// hoặc KHÔNG xuất hiện — không bao giờ partial. Rename từ staging cùng FS.

#[test]
fn test_scaffold_is_atomic_no_partial_on_failure() {
    // A template that fails mid-write (unsupported core file) must leave NO
    // directory at the target path — only the staging temp is cleaned up.
    // Template fail giữa chừng phải KHÔNG để lại target directory.
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("must-not-exist");
    let config = ScaffoldConfig {
        core: "nonexistent-core-xyz".to_string(),
        sub_type: String::new(),
        frameworks: vec!["vanilla".to_string()],
        project_name: target.to_string_lossy().to_string(),
        features: vec![],
        template_dir: PathBuf::new(),
    };
    let result = Scaffolder::scaffold(&config);
    assert!(result.is_err(), "scaffold with unknown core must fail");
    assert!(
        !target.exists(),
        "target must NOT exist after atomic failure"
    );
    // No staging leftovers inside the parent either — and the claim slot
    // must be RELEASED on failure so a retry can proceed.
    // Không staging rác trong parent — và claim-slot phải ĐƯỢC GIẢI PHÓNG
    // khi fail để lần retry sau chạy được.
    assert!(
        !root.path().join(".mgc-create-must-not-exist.lock").exists(),
        "claim slot must be released after atomic failure"
    );
    let leftovers: Vec<String> = std::fs::read_dir(root.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n != ".DS_Store")
        .collect();
    assert!(
        leftovers.is_empty(),
        "no staging dirs may leak, found: {leftovers:?}"
    );
}

#[test]
fn test_scaffold_rename_conflict_fails_closed() {
    // If the target directory appears between the exists-check and rename
    // (race window), scaffold must fail closed without touching the winner.
    // Nếu target xuất hiện trong cửa sổ race, scaffold phải fail-closed và
    // không đụng project đã thắng.
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("race-target");
    // Hermetic guard (CI fix 2026-09-11): this test drives the CONFLICT
    // logic, not template content — but scaffold still needs the vanilla
    // layer present. CI seeds an empty template cache (registry-first),
    // so the layer is absent there → skip honestly instead of failing.
    // Chốt hermetic: test này đo logic XUNG ĐỘT, không đo nội dung
    // template — nhưng scaffold vẫn cần layer vanilla. CI seed cache
    // template rỗng (registry-first) nên layer vắng → skip trung thực
    // thay vì fail.
    if !template_layer_ready("web/frontend/vanilla") {
        eprintln!(
            "SKIP (environment-unverified) test=test_scaffold_rename_conflict_fails_closed: web/frontend/vanilla template layer not available offline (registry-first)"
        );
        return;
    }
    let first = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "frontend".to_string(),
        frameworks: vec!["vanilla".to_string()],
        project_name: target.to_string_lossy().to_string(),
        features: vec![],
        template_dir: PathBuf::new(),
    };
    assert!(Scaffolder::scaffold(&first).is_ok());
    let before: Vec<String> = std::fs::read_dir(&target)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    let second = ScaffoldConfig {
        project_name: target.to_string_lossy().to_string(),
        ..first
    };
    let err = Scaffolder::scaffold(&second);
    assert!(err.is_err(), "double scaffold must fail");
    let after: Vec<String> = std::fs::read_dir(&target)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        before, after,
        "existing project content must be untouched after conflict"
    );
}

#[test]
fn test_scaffold_success_leaves_no_staging_sibling() {
    // After a successful scaffold, no hidden staging temp dir may remain
    // next to the target — only the project itself.
    // Scaffold thành công không để staging rác cạnh target.
    let root = tempfile::tempdir().unwrap();
    // Hermetic guard (CI fix 2026-09-11): see rename-conflict test —
    // CI seeds an empty template cache; without the layer this test
    // cannot drive the scaffold at all → skip honestly.
    // Chốt hermetic: xem test rename-conflict — CI seed cache template
    // rỗng; thiếu layer thì test không chạy được scaffold → skip
    // trung thực.
    if !template_layer_ready("web/frontend/vanilla") {
        eprintln!(
            "SKIP (environment-unverified) test=test_scaffold_success_leaves_no_staging_sibling: web/frontend/vanilla template layer not available offline (registry-first)"
        );
        return;
    }
    let target = root.path().join("clean-project");
    let config = ScaffoldConfig {
        core: "web".to_string(),
        sub_type: "frontend".to_string(),
        frameworks: vec!["vanilla".to_string()],
        project_name: target.to_string_lossy().to_string(),
        features: vec![],
        template_dir: PathBuf::new(),
    };
    let out = Scaffolder::scaffold(&config).unwrap();
    assert!(out.exists());
    let siblings: Vec<String> = std::fs::read_dir(root.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n != "clean-project" && n != ".DS_Store")
        .collect();
    assert!(
        siblings.is_empty(),
        "no staging siblings may remain, found: {siblings:?}"
    );
}

/// Remix template ships vite.config.ts with the vite plugin, so its
/// build script MUST be the vite-based compiler (`remix vite:build`) —
/// the classic `remix build` fails with "Missing output for entry
/// point" on this layout (proven live: fw-remix E2E).
/// Template remix có vite plugin thì script build phải là vite-based.
#[test]
fn test_remix_template_build_script_is_vite_based() {
    let files = crate::scaffold::embedded_kernel::get_embedded_template("web", "remix")
        .expect("remix embedded template exists");
    let pkg = files
        .iter()
        .find(|f| f.path == "package.json")
        .expect("remix template has package.json");
    let value: serde_json::Value =
        serde_json::from_str(pkg.content).expect("template package.json parses");
    assert_eq!(
        value
            .pointer("/scripts/build")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        "remix vite:build",
        "vite-plugin template must not use the classic compiler"
    );
    assert!(
        files.iter().any(|f| f.path == "vite.config.ts"),
        "vite plugin config present (why vite:build is required)"
    );
}

/// Embedded template ids with npm dependencies (mirrors the
/// ("web", ..) arms of embedded_kernel::get_embedded_template).
/// (ID template nhúng có dependency npm.)
const FRESHNESS_TEMPLATES: &[&str] = &[
    "react",
    "react-vite",
    "vue",
    "vue-vite",
    "express",
    "axum",
    "fastapi",
    "nextjs",
    "nuxt",
    "sveltekit",
    "angular",
    "solidjs",
    "qwik",
    "astro",
    "remix",
    "actix-web",
    "gin",
    "echo",
    "fiber",
    "django",
    "flask",
];

/// Migration backlog: (framework, dep, pairing reason). A dep whose
/// template major lags the registry latest major MUST be listed here —
/// unlisted lag FAILS the gate. As migrations land, entries are REMOVED
/// (never edited into silence).
/// (Tồn đọng migration: dep lag major phải có mặt kèm lý do.)
const FRESHNESS_BACKLOG: &[(&str, &str, &str)] = &[
    // Angular 19 stack: compiler/cli/core + TS ~5.6 are COUPLED —
    // Angular 20+ requires TS 5.8+/6 + zone 0.16 + new build
    // pipeline; bump = full template migration + E2E. Tracked, not silent.
    (
        "angular",
        "@angular-devkit/build-angular",
        "coupled Angular 19 stack; needs 20+ migration",
    ),
    (
        "angular",
        "@angular/cli",
        "coupled Angular 19 stack; needs 20+ migration",
    ),
    (
        "angular",
        "@angular/common",
        "coupled Angular 19 stack; needs 20+ migration",
    ),
    (
        "angular",
        "@angular/compiler",
        "coupled Angular 19 stack; needs 20+ migration",
    ),
    (
        "angular",
        "@angular/compiler-cli",
        "coupled Angular 19 stack; needs 20+ migration",
    ),
    (
        "angular",
        "@angular/core",
        "coupled Angular 19 stack; needs 20+ migration",
    ),
    (
        "angular",
        "@angular/platform-browser",
        "coupled Angular 19 stack; needs 20+ migration",
    ),
    (
        "angular",
        "typescript",
        "TS ~5.6 required by Angular 19; rides the Angular migration",
    ),
    // Vite 6 line: templates pin the 6.x plugin set (plugin-react 4,
    // plugin-vue 5); vite 7/8 + plugin majors need per-template build
    // E2E before bump.
    (
        "react",
        "@vitejs/plugin-react",
        "vite 6 plugin set; bump with vite major + build E2E",
    ),
    (
        "react",
        "typescript",
        "TS 5 line; TS 6/7 migration needs per-template tsc E2E",
    ),
    (
        "react",
        "vite",
        "vite 6 line; 7/8 needs per-template build E2E",
    ),
    (
        "react-vite",
        "@vitejs/plugin-react",
        "vite 6 plugin set; bump with vite major + build E2E",
    ),
    (
        "react-vite",
        "typescript",
        "TS 5 line; TS 6/7 migration needs per-template tsc E2E",
    ),
    (
        "react-vite",
        "vite",
        "vite 6 line; 7/8 needs per-template build E2E",
    ),
    (
        "vue-vite",
        "@vitejs/plugin-vue",
        "vite 6 plugin set; bump with vite major + build E2E",
    ),
    (
        "vue-vite",
        "typescript",
        "TS 5 line; TS 6/7 migration needs per-template tsc E2E",
    ),
    (
        "vue-vite",
        "vite",
        "vite 6 line; 7/8 needs per-template build E2E",
    ),
    (
        "vue-vite",
        "vue-tsc",
        "tsc 2 pairs vue 3.5 template; 3 needs tsc E2E",
    ),
    (
        "vue",
        "@vitejs/plugin-vue",
        "vite 6 plugin set; bump with vite major + build E2E",
    ),
    (
        "vue",
        "typescript",
        "TS 5 line; TS 6/7 migration needs per-template tsc E2E",
    ),
    (
        "vue",
        "vite",
        "vite 6 line; 7/8 needs per-template build E2E",
    ),
    (
        "vue",
        "vue-tsc",
        "tsc 2 pairs vue 3.5 template; 3 needs tsc E2E",
    ),
    (
        "solidjs",
        "typescript",
        "TS 5 line; TS 6/7 migration needs per-template tsc E2E",
    ),
    (
        "solidjs",
        "vite",
        "vite 6 line; 7/8 needs per-template build E2E",
    ),
    (
        "sveltekit",
        "@sveltejs/adapter-auto",
        "adapter 3 pairs kit 2 early era; 7 needs kit-compat + build E2E",
    ),
    (
        "sveltekit",
        "@sveltejs/vite-plugin-svelte",
        "plugin 4 pairs vite 6; 7 needs build E2E",
    ),
    (
        "sveltekit",
        "typescript",
        "TS 5 line; TS 6/7 migration needs per-template tsc E2E",
    ),
    (
        "sveltekit",
        "vite",
        "vite 6 line; 7/8 needs per-template build E2E",
    ),
    (
        "qwik",
        "typescript",
        "TS 5 line; TS 6/7 migration needs per-template tsc E2E",
    ),
    (
        "qwik",
        "vite",
        "vite 6 line; 7/8 needs per-template build E2E",
    ),
    // Astro 5 + Next 15 + Nuxt 3: framework-major migrations (config/
    // codemod level), each needs its template E2E green before bump.
    ("astro", "astro", "framework major migration + E2E"),
    (
        "astro",
        "typescript",
        "TS 5 line; TS 6/7 migration needs per-template tsc E2E",
    ),
    (
        "nextjs",
        "@types/node",
        "types major; bump with template build E2E",
    ),
    ("nextjs", "next", "framework major migration + E2E"),
    (
        "nextjs",
        "typescript",
        "TS 5 line; TS 6/7 migration needs per-template tsc E2E",
    ),
    ("nuxt", "nuxt", "framework major migration + E2E"),
    (
        "nuxt",
        "typescript",
        "TS 5 line; TS 6/7 migration needs per-template tsc E2E",
    ),
    (
        "nuxt",
        "vue-router",
        "router 5 needs compat E2E with vue 3 template",
    ),
    (
        "express",
        "@types/node",
        "types major; bump with template build E2E",
    ),
    (
        "express",
        "typescript",
        "TS 5 line; TS 6/7 migration needs per-template tsc E2E",
    ),
    // Remix 2 line: react 18 + types 18 PAIRED (Remix 2 supports React
    // 18); isbot 4 verified by green build E2E (5.x unevaluated).
    (
        "remix",
        "@types/react",
        "paired with React 18 (Remix 2 line)",
    ),
    (
        "remix",
        "@types/react-dom",
        "paired with React 18 (Remix 2 line)",
    ),
    (
        "remix",
        "isbot",
        "4.x verified by green build E2E; 5.x unevaluated",
    ),
    ("remix", "react", "paired React 18 (Remix 2 line)"),
    ("remix", "react-dom", "paired React 18 (Remix 2 line)"),
    (
        "remix",
        "typescript",
        "TS 5 line; TS 6/7 migration needs per-template tsc E2E",
    ),
    (
        "remix",
        "vite",
        "vite 6 line; 7/8 needs per-template build E2E",
    ),
];

fn template_major(range: &str) -> Option<u64> {
    let t = range.trim_start_matches(['^', '~', '>', '<', '=', ' ']);
    t.split('.').next()?.parse().ok()
}

async fn npm_latest_major(name: &str) -> Option<u64> {
    let url = format!("https://registry.npmjs.org/{}", name.replace('/', "%2f"));
    let body: serde_json::Value = reqwest::get(&url).await.ok()?.json().await.ok()?;
    body.pointer("/dist-tags/latest")?
        .as_str()?
        .split('.')
        .next()?
        .parse()
        .ok()
}

/// Template freshness gate (P0-bonus): every embedded npm dep either
/// tracks the registry latest major or sits in FRESHNESS_BACKLOG with
/// its pairing reason. Offline → loud skip (the gate needs connect;
/// "connect thì sao" = it runs, below).
/// (Cổng tươi template: dep lag major mà không trong backlog thì FAIL.
/// Offline thì skip ồn ào.)
#[tokio::test]
async fn test_embedded_template_dep_majors_track_latest() {
    // Probe connectivity first: offline machines skip LOUDLY (not silently).
    // (Mất mạng thì skip ồn ào.)
    if npm_latest_major("react").await.is_none() {
        eprintln!("SKIP template freshness: registry unreachable (offline)");
        return;
    }
    let mut lags = Vec::new();
    for fw in FRESHNESS_TEMPLATES {
        let files = crate::scaffold::embedded_kernel::get_embedded_template("web", fw)
            .unwrap_or_else(|| panic!("embedded template missing: {fw}"));
        for f in &files {
            if !f.path.ends_with("package.json") {
                continue;
            }
            let pkg: serde_json::Value =
                serde_json::from_str(f.content).expect("template package.json parses");
            for section in ["dependencies", "devDependencies"] {
                let Some(deps) = pkg.get(section).and_then(|v| v.as_object()) else {
                    continue;
                };
                for (name, ver) in deps {
                    let ver = ver.as_str().unwrap_or("");
                    let Some(tmajor) = template_major(ver) else {
                        continue;
                    };
                    // Exact pins (no range operator) freeze versions — banned.
                    if !ver.starts_with(['^', '~', '>', '<']) {
                        lags.push(format!("{fw}:{name} exact pin {ver} (ranges only)"));
                        continue;
                    }
                    let Some(latest) = npm_latest_major(name).await else {
                        continue;
                    };
                    if tmajor < latest {
                        lags.push(format!("{fw}:{name} template ^{tmajor} vs latest {latest}"));
                    }
                }
            }
        }
    }
    // Unlisted lag = lag without a backlog entry → FAIL.
    // (Lag không trong backlog thì FAIL.)
    let unlisted: Vec<_> = lags
        .iter()
        .filter(|u| {
            !FRESHNESS_BACKLOG.iter().any(|(b_fw, b_dep, _)| {
                u.starts_with(&format!("{b_fw}:{b_dep} "))
                    || u.starts_with(&format!("{b_fw}:{b_dep} exact"))
            })
        })
        .collect();
    // Second pass: every backlog entry must reference a REAL lag (no
    // stale entries hiding resolved migrations).
    for (fw, dep, _reason) in FRESHNESS_BACKLOG {
        assert!(
            lags.iter().any(|u| u.starts_with(&format!("{fw}:{dep} "))),
            "stale backlog entry (already current): {fw}:{dep}"
        );
    }
    assert!(
        unlisted.is_empty(),
        "template majors lagging latest without backlog entry:\n{}",
        unlisted
            .iter()
            .map(|u| u.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
