    use super::*;
    use mg_lockfile::{serialization, LockPackage, ResolutionMeta};
    use mg_types::{DependencySpec, Ecosystem, VersionRange};

    #[test]
    fn game_hook_optimizer_dep_adds_path_dep_to_bevy_manifest() {
        let dir = std::env::temp_dir().join(format!(
            "mg-game-hook-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nbevy = \"0.14\"\n",
        )
        .unwrap();

        game_hook_optimizer_dep(&dir).unwrap();
        let after = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        assert!(after.contains("mg-optimizer"), "dep must be added: {after}");
        assert!(after.contains("./optimizer"), "path dep: {after}");
        assert!(
            toml::from_str::<toml::Value>(&after).is_ok(),
            "manifest stays valid"
        );

        // idempotent: chạy lại không thêm trùng
        game_hook_optimizer_dep(&dir).unwrap();
        let twice = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        assert_eq!(
            twice.matches("mg-optimizer").count(),
            1,
            "no duplicate dep: {twice}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn game_hook_optimizer_dep_skips_non_cargo_projects() {
        let dir = std::env::temp_dir().join(format!(
            "mg-game-hook-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // godot project: không có root Cargo.toml
        std::fs::write(dir.join("project.godot"), "# demo\n").unwrap();
        game_hook_optimizer_dep(&dir).unwrap();
        assert!(!dir.join("Cargo.toml").exists(), "no Cargo.toml created");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_lock_matches_manifest_when_versions_satisfy_ranges() {
        let mut manifest = Manifest::new("demo", Ecosystem::Web);
        manifest.add_dep(
            DependencySpec::new(
                PackageName::new("tailwindcss").unwrap(),
                VersionRange::parse("^4.3.0").unwrap(),
            ),
            false,
            false,
            false,
        );
        let mut lock = Lockfile::new("web", "frontend");
        lock.resolution = ResolutionMeta {
            state: "locked".into(),
            store: "megagate".into(),
            package_count: 1,
        };
        lock.packages.push(LockPackage {
            name: "tailwindcss".into(),
            version: "4.3.2".into(),
            integrity: None,
            direct: true,
            dev: false,
            dependencies: vec![],
            peer_deps: vec![],
        });
        assert!(lock_matches_manifest(&lock, &manifest));
    }

    #[test]
    fn test_lock_matches_manifest_rejects_stale_version() {
        let mut manifest = Manifest::new("demo", Ecosystem::Web);
        manifest.add_dep(
            DependencySpec::new(
                PackageName::new("tailwindcss").unwrap(),
                VersionRange::parse("^5.0.0").unwrap(),
            ),
            false,
            false,
            false,
        );
        let mut lock = Lockfile::new("web", "frontend");
        lock.resolution = ResolutionMeta {
            state: "locked".into(),
            store: "megagate".into(),
            package_count: 1,
        };
        lock.packages.push(LockPackage {
            name: "tailwindcss".into(),
            version: "4.3.2".into(),
            integrity: None,
            direct: true,
            dev: false,
            dependencies: vec![],
            peer_deps: vec![],
        });
        assert!(!lock_matches_manifest(&lock, &manifest));
    }

    #[test]
    fn test_read_checked_lockfile_errors_on_checksum_mismatch() {
        let root = tempfile::tempdir().unwrap();
        let lock = Lockfile::new("web", "frontend");
        std::fs::write(
            root.path().join("mg.lock"),
            serialization::to_toml(&lock).unwrap(),
        )
        .unwrap();
        std::fs::write(root.path().join("mg.lock.sha256"), "bad").unwrap();

        let err = read_checked_lockfile(root.path()).unwrap_err();

        assert!(
            err.to_string().contains("lockfile checksum mismatch"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_graph_from_lockfile_rejects_invalid_dependency_id() {
        let mut lock = Lockfile::new("web", "frontend");
        lock.packages.push(LockPackage {
            name: "react".into(),
            version: "18.2.0".into(),
            integrity: None,
            direct: true,
            dev: false,
            dependencies: vec!["not-a-package-id".into()],
            peer_deps: vec![],
        });

        let err = graph_from_lockfile(&lock).unwrap_err();

        assert!(
            err.to_string().contains("invalid dependency id"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_load_locked_graph_ignores_legacy_pm_lockfile() {
        let root = tempfile::tempdir().unwrap();
        let mut manifest = Manifest::new("demo", Ecosystem::Web);
        manifest.add_dep(
            DependencySpec::new(
                PackageName::new("left-pad").unwrap(),
                VersionRange::parse("^1.3.0").unwrap(),
            ),
            false,
            false,
            false,
        );
        std::fs::write(
            root.path().join("package-lock.json"),
            r#"{
  "name": "demo",
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "demo", "dependencies": { "left-pad": "^1.3.0" } },
    "node_modules/left-pad": { "version": "1.3.0", "integrity": "sha512-abc" }
  }
}"#,
        )
        .unwrap();

        let graph = load_locked_graph(root.path(), "web", &manifest).unwrap();

        assert!(graph.is_none());
        assert!(
            !root.path().join("mg.lock").exists(),
            "legacy lockfiles must not become mg.lock without an explicit migration command"
        );
    }

    #[test]
    fn test_load_locked_graph_fails_closed_on_future_version() {
        let root = tempfile::tempdir().unwrap();
        let manifest = Manifest::new("demo", Ecosystem::Web);
        let mut lock = Lockfile::new("web", "frontend");
        lock.version = 99;
        lock.resolution = ResolutionMeta {
            state: "locked".into(),
            store: "megagate".into(),
            package_count: 0,
        };
        std::fs::write(
            root.path().join("mg.lock"),
            serialization::to_toml(&lock).unwrap(),
        )
        .unwrap();

        let err = load_locked_graph(root.path(), "web", &manifest).unwrap_err();
        assert!(
            err.to_string().contains("newer than this version"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_load_pruned_locked_graph_keeps_only_reachable_packages() {
        let root = tempfile::tempdir().unwrap();
        let mut manifest = Manifest::new("demo", Ecosystem::Web);
        manifest.add_dep(
            DependencySpec::new(
                PackageName::new("react").unwrap(),
                VersionRange::parse("^18.0.0").unwrap(),
            ),
            false,
            false,
            false,
        );

        let mut lock = Lockfile::new("web", "frontend");
        lock.resolution = ResolutionMeta {
            state: "locked".into(),
            store: "megagate".into(),
            package_count: 4,
        };
        lock.packages.push(LockPackage {
            name: "react".into(),
            version: "18.3.1".into(),
            integrity: None,
            direct: true,
            dev: false,
            dependencies: vec!["loose-envify@1.4.0".into()],
            peer_deps: vec![],
        });
        lock.packages.push(LockPackage {
            name: "loose-envify".into(),
            version: "1.4.0".into(),
            integrity: None,
            direct: false,
            dev: false,
            dependencies: vec![],
            peer_deps: vec![],
        });
        lock.packages.push(LockPackage {
            name: "zod".into(),
            version: "4.4.3".into(),
            integrity: None,
            direct: true,
            dev: false,
            dependencies: vec![],
            peer_deps: vec![],
        });
        lock.packages.push(LockPackage {
            name: "orphan".into(),
            version: "1.0.0".into(),
            integrity: None,
            direct: false,
            dev: false,
            dependencies: vec![],
            peer_deps: vec![],
        });
        std::fs::write(
            root.path().join("mg.lock"),
            serialization::to_toml(&lock).unwrap(),
        )
        .unwrap();

        let graph = load_pruned_locked_graph(root.path(), "web", &manifest)
            .unwrap()
            .unwrap();
        let names = graph
            .packages
            .iter()
            .map(|pkg| pkg.id.name_str().to_string())
            .collect::<std::collections::BTreeSet<_>>();

        assert!(names.contains("react"));
        assert!(names.contains("loose-envify"));
        assert!(!names.contains("zod"));
        assert!(!names.contains("orphan"));
    }

    #[test]
    fn test_build_delta_manifest_keeps_dependency_group() {
        let manifest = Manifest::new("demo", Ecosystem::Web);
        let added = vec![AddedPackage {
            id: PackageId::parse("vitest@3.2.1").unwrap(),
            dev: true,
            optional: false,
            peer: false,
        }];

        let delta = build_delta_manifest(&manifest, &added).unwrap();

        assert!(delta.dependencies.is_empty());
        assert_eq!(delta.dev_dependencies.len(), 1);
        assert_eq!(delta.dev_dependencies[0].name.as_str(), "vitest");
        assert_eq!(delta.dev_dependencies[0].range.as_str(), "=3.2.1");
    }

    #[test]
    fn test_merge_graphs_promotes_existing_transitive_to_direct() {
        let dep_id = PackageId::parse("zod@4.4.3").unwrap();
        let base = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: dep_id.clone(),
                integrity: String::new(),
                tarball_url: String::new(),
                deps: vec![],
                peer_deps: vec![],
                direct: false,
                dev: false,
            }],
        };
        let delta = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: dep_id,
                integrity: "sha512-test".into(),
                tarball_url: "https://registry.example/zod.tgz".into(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: true,
            }],
        };

        let merged = merge_graphs(base, delta);

        assert_eq!(merged.packages.len(), 1);
        assert!(merged.packages[0].direct);
        assert!(merged.packages[0].dev);
        assert_eq!(merged.packages[0].integrity, "sha512-test");
    }
