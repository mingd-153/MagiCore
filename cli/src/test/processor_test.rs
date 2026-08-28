#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for scaffold template processor

use super::*;

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
            eprintln!("skipped: web/frontend/react-vite template layer not available offline (registry-first)");
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
            eprintln!(
                "skipped: iot/esp32-rust template layer not available offline (registry-first)"
            );
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
        assert!(out
            .join("apps")
            .join("frontend")
            .join("crates")
            .join("engine")
            .join("Cargo.toml")
            .exists());
        assert!(out
            .join("apps")
            .join("frontend")
            .join("src")
            .join("bridges")
            .join("engine.js")
            .exists());
        assert!(out
            .join("packages")
            .join("contracts")
            .join("package.json")
            .exists());
        assert!(out
            .join("apps")
            .join("frontend")
            .join("vite.config.js")
            .exists());
        assert!(out
            .join("apps")
            .join("backend")
            .join("src")
            .join("server.js")
            .exists());
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
        assert!(out
            .join("apps")
            .join("frontend")
            .join("package.json")
            .exists());
        let back_cargo =
            std::fs::read_to_string(out.join("apps").join("backend").join("Cargo.toml")).unwrap();
        assert!(
            back_cargo.contains("axum"),
            "backend Cargo.toml should pin axum, got: {back_cargo}"
        );
        assert!(out
            .join("apps")
            .join("backend")
            .join("src")
            .join("main.rs")
            .exists());
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
        let go_mod =
            std::fs::read_to_string(out.join("apps").join("backend").join("go.mod")).unwrap();
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
        assert!(react_out
            .join("crates")
            .join("engine")
            .join("Cargo.toml")
            .exists());
        assert!(react_out.join("src").join("main.jsx").exists());
        assert!(react_out.join("src").join("App.jsx").exists());
        assert!(react_out
            .join("src")
            .join("bridges")
            .join("engine.js")
            .exists());
        assert!(react_out
            .join("src")
            .join("styles")
            .join("theme.css")
            .exists());
        assert!(react_out
            .join("src")
            .join("assets")
            .join("magicore-grid.svg")
            .exists());
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
        assert!(next_out
            .join("crates")
            .join("engine")
            .join("Cargo.toml")
            .exists());
        assert!(next_out.join("src").join("app").join("page.jsx").exists());
        assert!(next_out
            .join("src")
            .join("bridges")
            .join("engine.js")
            .exists());
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
        assert!(vue_out
            .join("src")
            .join("components")
            .join("AppShell.vue")
            .exists());
        assert!(vue_out
            .join("src")
            .join("router")
            .join("AppRouter.vue")
            .exists());
        assert!(vue_out
            .join("src")
            .join("hooks")
            .join("useProjectLinks.ts")
            .exists());
        assert!(!vue_out
            .join("src")
            .join("components")
            .join("AppShell.tsx")
            .exists());

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
        assert!(vanilla_out
            .join("src")
            .join("components")
            .join("AppShell.ts")
            .exists());
        assert!(vanilla_out
            .join("src")
            .join("router")
            .join("AppRouter.ts")
            .exists());
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
        assert!(react_express_out
            .join("src")
            .join("styles")
            .join("theme.css")
            .exists());
        assert!(react_express_out
            .join("server")
            .join("src")
            .join("server.ts")
            .exists());

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
        assert!(solid_out
            .join("src")
            .join("components")
            .join("AppShell.tsx")
            .exists());
        assert!(solid_out
            .join("src")
            .join("router")
            .join("AppRouter.tsx")
            .exists());

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
        assert!(fastify_out
            .join("src")
            .join("config")
            .join("app.js")
            .exists());
        assert!(fastify_out
            .join("src")
            .join("routes")
            .join("health.js")
            .exists());
        assert!(fastify_out
            .join("src")
            .join("services")
            .join("status.js")
            .exists());
        assert!(!fastify_out.join("tsconfig.json").exists());
    }

    #[test]
    fn test_web_typescript_feature_switches_extensions() {
        if !template_layer_ready("web/frontend/react-vite") {
            eprintln!("skipped: web/frontend/react-vite template layer not available offline (registry-first)");
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
        assert!(out
            .join("crates")
            .join("engine")
            .join("Cargo.toml")
            .exists());
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
        assert!(broken_mono_err
            .to_string()
            .contains("Scaffold for monorepo frontend framework 'ember' is not implemented yet"));
    }

    #[test]
    fn test_web_feature_gated_templates_materialize_only_when_active() {
        if !template_layer_ready("web/frontend/nextjs") {
            eprintln!("skipped: web/frontend/nextjs template layer not available offline (registry-first)");
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
            eprintln!("skipped: web/frontend/nextjs template layer not available offline (registry-first)");
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
            eprintln!("skipped: web/frontend/nextjs template layer not available offline (registry-first)");
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
        let android_build =
            std::fs::read_to_string(out.join("android/app/build.gradle.kts")).unwrap();
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
}
