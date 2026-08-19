    use super::*;
    use base64::Engine;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use mg_lockfile::{serialization, LockPackage, Lockfile, ResolutionMeta};
    use mg_resolver::{DependencyError, DependencyProvider, RegistryCache, ResolvedDep};
    use mg_store::{ContentStore, Database, Layout, PackageCache};
    use sha2::{Digest, Sha256, Sha512};
    use std::io::ErrorKind;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::sync::{Mutex, OnceLock};
    use tar::{Builder, Header};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use crate::cache::*;
    use crate::install::bin::*;
    use crate::install::download::*;
    use crate::install::extract::*;
    use crate::install::materialize::*;
    use crate::install::run_install;
    use crate::install::{should_run_lifecycle_scripts, trust_allows_script};
    use crate::lockfile::*;
    use crate::manifest::*;
    use crate::profile::*;
    use crate::provider::*;
    use crate::update::*;

    async fn bind_test_listener() -> Option<TcpListener> {
        match TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => Some(listener),
            Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                eprintln!("skipping socket-backed test in sandbox: {err}");
                None
            }
            Err(err) => panic!("failed to bind socket-backed test listener: {err}"),
        }
    }

    #[test]
    fn test_web_adapter() {
        assert_eq!(WebAdapter::new().registry_url, "https://registry.npmjs.org");
    }
    #[test]
    fn test_package_json() {
        let p = PackageJson::new("t".into(), "1.0.0".into());
        assert_eq!(p.name, "t");
    }
    #[test]
    fn test_can_handle() {
        let dir = tempfile::tempdir().unwrap();
        PackageJson::new("t".into(), "1.0.0".into())
            .save(&dir.path().join("package.json"))
            .unwrap();
        assert!(WebAdapter::new().can_handle(dir.path()));
    }

    #[tokio::test]
    async fn test_add_writes_manifest_and_install_creates_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "private": true,
                "type": "module",
                "scripts": {
                    "dev": "mg web dev"
                }
            })
            .to_string(),
        )
        .unwrap();

        let adapter = WebAdapter::new();
        let name = PackageName::new("tailwindcss").unwrap();
        let range = VersionRange::parse("^3.4.0").unwrap();
        adapter
            .add(dir.path(), &name, Some(&range), AddOptions::default())
            .await
            .unwrap();

        let manifest = adapter.parse_manifest(dir.path()).await.unwrap();
        assert!(manifest.find_dep("tailwindcss").is_some());
        let package_json = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
        assert!(package_json.contains("\"private\": true"));
        assert!(package_json.contains("\"type\": \"module\""));
        assert!(package_json.contains("\"dev\": \"mg web dev\""));

        let package_id = PackageId::new(name, Version::parse("3.4.0").unwrap());
        let integrity = seed_cached_tarball(dir.path(), &package_id);
        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: package_id.clone(),
                integrity,
                tarball_url: String::new(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: false,
            }],
        };
        let summary = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();
        assert_eq!(summary.added, vec![package_id]);
        assert!(dir
            .path()
            .join("node_modules")
            .join("tailwindcss")
            .join("package.json")
            .exists());
        assert!(dir
            .path()
            .join("node_modules")
            .join("tailwindcss")
            .join("index.css")
            .exists());

        let lock = std::fs::read_to_string(dir.path().join("mg.lock")).unwrap();
        let parsed: Lockfile = serialization::from_toml(&lock).unwrap();
        assert_eq!(parsed.resolution.state, "locked");
        assert_eq!(parsed.packages.len(), 1);
        assert_eq!(parsed.packages[0].name, "tailwindcss");
        assert_eq!(parsed.packages[0].version, "3.4.0");
    }

    #[tokio::test]
    async fn test_audit_fix_bumps_vulnerable_packages_and_rewrites_lockfile() {
        let shared = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();

        seed_shared_metadata(
            shared.path(),
            "react",
            serde_json::json!({
                "name": "react",
                "description": null,
                "versions": {
                    "18.2.0": {
                        "version": "18.2.0",
                        "dependencies": null,
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": "https://registry.example.test/react-18.2.0.tgz",
                            "integrity": "sha512-c2hhcmVk"
                        }
                    },
                    "19.0.0": {
                        "version": "19.0.0",
                        "dependencies": null,
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": "https://registry.example.test/react-19.0.0.tgz",
                            "integrity": "sha512-c2hhcmVk2"
                        }
                    }
                },
                "dist-tags": { "latest": "19.0.0" }
            }),
        );

        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "private": true,
                "dependencies": { "react": "^18.2.0" }
            })
            .to_string(),
        )
        .unwrap();

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "http://127.0.0.1:9".into(),
            shared.path().to_path_buf(),
        );
        let vulnerable = vec![PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        )];

        let bumped = adapter.audit_fix(dir.path(), &vulnerable).await.unwrap();
        assert_eq!(bumped, 1);

        let manifest = adapter.parse_manifest(dir.path()).await.unwrap();
        let dep = manifest.find_dep("react").unwrap();
        assert_eq!(dep.range.as_str(), "*");

        let lock = std::fs::read_to_string(dir.path().join("mg.lock")).unwrap();
        let parsed: Lockfile = serialization::from_toml(&lock).unwrap();
        assert!(parsed
            .packages
            .iter()
            .any(|p| p.name == "react" && p.version == "19.0.0"));
    }

    #[tokio::test]
    async fn test_audit_fix_fail_closed_keeps_manifest_and_lockfile_when_resolve_fails() {
        let shared = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "private": true,
                "dependencies": { "react": "^18.2.0" }
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("mg.lock"),
            "[resolution]\nstate = \"locked\"\n",
        )
        .unwrap();
        let original_manifest = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
        let original_lock = std::fs::read_to_string(dir.path().join("mg.lock")).unwrap();

        // Registry + metadata cache both empty -> resolve must fail -> nothing written.
        let adapter = WebAdapter::with_registry_and_shared_cache(
            "http://127.0.0.1:9".into(),
            shared.path().to_path_buf(),
        );
        let vulnerable = vec![PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        )];

        assert!(adapter.audit_fix(dir.path(), &vulnerable).await.is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("package.json")).unwrap(),
            original_manifest
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("mg.lock")).unwrap(),
            original_lock
        );
    }

    #[test]
    fn test_write_web_lockfile_with_state_skips_rewrite_when_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let package_id = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: package_id,
                integrity: "sha512-demo".into(),
                tarball_url: String::new(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        write_web_lockfile_with_state(dir.path(), &graph, "locked").unwrap();
        let lock_path = dir.path().join("mg.lock");
        let checksum_path = dir.path().join("mg.lock.sha256");
        let first_lock_modified = std::fs::metadata(&lock_path).unwrap().modified().unwrap();
        let first_checksum_modified = std::fs::metadata(&checksum_path)
            .unwrap()
            .modified()
            .unwrap();

        std::thread::sleep(std::time::Duration::from_secs(1));

        write_web_lockfile_with_state(dir.path(), &graph, "locked").unwrap();
        let second_lock_modified = std::fs::metadata(&lock_path).unwrap().modified().unwrap();
        let second_checksum_modified = std::fs::metadata(&checksum_path)
            .unwrap()
            .modified()
            .unwrap();

        assert_eq!(first_lock_modified, second_lock_modified);
        assert_eq!(first_checksum_modified, second_checksum_modified);
    }

    #[tokio::test]
    async fn test_install_materializes_node_modules_bin_links() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "vite": "^8.1.4"
                }
            })
            .to_string(),
        )
        .unwrap();

        let package_id = PackageId::new(
            PackageName::new("vite").unwrap(),
            Version::parse("8.1.4").unwrap(),
        );
        let integrity = seed_cached_tarball_with_files(
            dir.path(),
            &package_id,
            &[
                (
                    "package/package.json",
                    br#"{"name":"vite","version":"8.1.4","bin":"bin/vite.js"}"#.as_slice(),
                ),
                (
                    "package/bin/vite.js",
                    b"#!/usr/bin/env node\nconsole.log('vite')\n",
                ),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: package_id,
                integrity,
                tarball_url: String::new(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: true,
            }],
        };

        let adapter = WebAdapter::new();
        adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();

        assert!(dir.path().join("node_modules/.bin").exists());
        assert!(dir.path().join("node_modules/.bin/vite").exists());
    }

    #[tokio::test]
    async fn test_resolve_populates_tarball_url_and_integrity_from_shared_metadata() {
        let shared = tempfile::tempdir().unwrap();

        seed_shared_metadata(
            shared.path(),
            "react",
            serde_json::json!({
                "name": "react",
                "description": null,
                "versions": {
                    "18.2.0": {
                        "version": "18.2.0",
                        "dependencies": null,
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": "https://registry.example.test/react-18.2.0.tgz",
                            "integrity": "sha512-c2hhcmVk"
                        }
                    }
                },
                "dist-tags": {
                    "latest": "18.2.0"
                }
            }),
        );

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "http://127.0.0.1:9".into(),
            shared.path().to_path_buf(),
        );
        let mut manifest = Manifest::new("demo", mg_types::ecosystem::Ecosystem::Web);
        manifest.add_dep(
            DependencySpec::new(
                PackageName::new("react").unwrap(),
                VersionRange::parse("^18.2.0").unwrap(),
            ),
            false,
            false,
            false,
        );

        let graph = adapter.resolve(&manifest).await.unwrap();
        assert_eq!(graph.packages.len(), 1);
        assert_eq!(
            graph.packages[0].tarball_url,
            "https://registry.example.test/react-18.2.0.tgz"
        );
        assert_eq!(graph.packages[0].integrity, "sha512-c2hhcmVk");
    }

    #[tokio::test]
    async fn test_resolve_uses_shared_resolution_cache_when_registry_is_unavailable() {
        let shared = tempfile::tempdir().unwrap();
        let registry_url = "http://127.0.0.1:9";
        let cache = SharedWebCache {
            root: shared.path().to_path_buf(),
        };
        let mut manifest = Manifest::new("demo-a", mg_types::ecosystem::Ecosystem::Web);
        manifest.add_dep(
            DependencySpec::new(
                PackageName::new("react").unwrap(),
                VersionRange::parse("^18.2.0").unwrap(),
            ),
            false,
            false,
            false,
        );
        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: PackageId::new(
                    PackageName::new("react").unwrap(),
                    Version::parse("18.2.0").unwrap(),
                ),
                integrity: "sha512-react".to_string(),
                tarball_url: "https://registry.example.test/react-18.2.0.tgz".to_string(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: false,
            }],
        };
        let key = manifest_resolution_cache_key(&manifest, registry_url);
        cache.write_resolution(&key, registry_url, &graph).unwrap();

        let adapter = WebAdapter::with_registry_and_shared_cache(
            registry_url.to_string(),
            shared.path().to_path_buf(),
        );
        let resolved = adapter.resolve(&manifest).await.unwrap();

        assert_eq!(resolved.packages.len(), 1);
        assert_eq!(resolved.packages[0].id.to_string(), "react@18.2.0");
        assert_eq!(resolved.packages[0].integrity, "sha512-react");
    }

    #[test]
    fn test_read_web_lockfile_checked_rejects_checksum_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let mut lock = Lockfile::new("web", "frontend");
        lock.resolution.state = "locked".into();
        let toml = serialization::to_toml(&lock).unwrap();
        std::fs::write(dir.path().join("mg.lock"), toml).unwrap();
        std::fs::write(dir.path().join("mg.lock.sha256"), "not-the-checksum").unwrap();

        let err = read_web_lockfile_checked(dir.path()).unwrap_err();

        assert!(
            err.to_string().contains("lockfile checksum mismatch"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_read_web_lockfile_checked_rejects_malformed_lockfile() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mg.lock"), "not = [valid").unwrap();

        let err = read_web_lockfile_checked(dir.path()).unwrap_err();

        assert!(
            err.to_string().contains("failed to parse lockfile"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_pending_scaffold_lockfile_without_checksum_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("mg.lock"),
            r#"version = 1
core = "web"
mode = "frontend"
frameworks = ["react"]

[resolution]
state = "pending"
store = "megagate"
package_count = 0
"#,
        )
        .unwrap();

        let lock = read_web_lockfile_checked(dir.path()).unwrap().unwrap();
        assert_eq!(lock.resolution.state, "pending");
        assert_eq!(lock.resolution.package_count, 0);
    }

    #[test]
    fn test_lifecycle_scripts_are_opt_in() {
        let old = std::env::var_os("MEGAGATE_WEB_ALLOW_SCRIPTS");
        std::env::remove_var("MEGAGATE_WEB_ALLOW_SCRIPTS");
        assert!(!should_run_lifecycle_scripts(false, false));
        assert!(should_run_lifecycle_scripts(false, true));

        std::env::set_var("MEGAGATE_WEB_ALLOW_SCRIPTS", "1");
        assert!(should_run_lifecycle_scripts(false, false));
        assert!(!should_run_lifecycle_scripts(true, true));
        restore_env_var("MEGAGATE_WEB_ALLOW_SCRIPTS", old);
    }

    #[test]
    fn test_trust_gate_fail_closed_with_escape_hatch() {
        // Fail-closed: unlisted → run only under blanket opt-in.
        assert!(!trust_allows_script(None, false));
        assert!(trust_allows_script(None, true));
        // Explicit policy wins both ways.
        assert!(trust_allows_script(Some("approved"), false));
        assert!(!trust_allows_script(Some("denied"), true));
        // Unknown policy value is treated as unlisted (fail-closed).
        assert!(!trust_allows_script(Some("suspicious"), false));
    }

    #[test]
    fn test_manifest_resolution_cache_key_ignores_dep_order_and_app_name() {
        let registry_url = "https://registry.npmjs.org";
        let mut left = Manifest::new("demo-a", mg_types::ecosystem::Ecosystem::Web);
        left.add_dep(
            DependencySpec::new(
                PackageName::new("react").unwrap(),
                VersionRange::parse("^18.2.0").unwrap(),
            ),
            false,
            false,
            false,
        );
        left.add_dep(
            DependencySpec::new(
                PackageName::new("zod").unwrap(),
                VersionRange::parse("^3.22.4").unwrap(),
            ),
            true,
            false,
            false,
        );

        let mut right = Manifest::new("demo-b", mg_types::ecosystem::Ecosystem::Web);
        right.add_dep(
            DependencySpec::new(
                PackageName::new("zod").unwrap(),
                VersionRange::parse("^3.22.4").unwrap(),
            ),
            true,
            false,
            false,
        );
        right.add_dep(
            DependencySpec::new(
                PackageName::new("react").unwrap(),
                VersionRange::parse("^18.2.0").unwrap(),
            ),
            false,
            false,
            false,
        );

        assert_eq!(
            manifest_resolution_cache_key(&left, registry_url),
            manifest_resolution_cache_key(&right, registry_url)
        );
    }

    #[test]
    fn test_prune_shared_cache_to_quota_removes_prunable_entries() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache").join("react");
        let resolution_dir = dir.path().join("resolutions");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::create_dir_all(&resolution_dir).unwrap();
        std::fs::write(cache_dir.join("18.2.0.tgz"), vec![b'a'; 1024]).unwrap();
        std::fs::write(resolution_dir.join("graph.json"), vec![b'b'; 1024]).unwrap();

        prune_shared_cache_to_quota(dir.path(), 512, &std::collections::HashSet::new()).unwrap();

        let remaining = directory_size(dir.path());
        assert!(
            remaining <= 512,
            "expected quota prune to reduce cache to <= 512 bytes, got {remaining}"
        );
    }

    #[test]
    fn test_prune_shared_cache_to_quota_does_not_delete_unmarked_package_json_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("packages").join("manual").join("nested");
        let cache_dir = dir.path().join("cache").join("react");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(nested.join("package.json"), "{}").unwrap();
        std::fs::write(cache_dir.join("old.tgz"), vec![b'a'; 1024]).unwrap();

        prune_shared_cache_to_quota(dir.path(), 1, &std::collections::HashSet::new()).unwrap();

        assert!(
            nested.join("package.json").exists(),
            "quota pruning should only delete MegaGate-marked package cache roots"
        );
    }

    #[test]
    fn test_prune_shared_cache_to_quota_keeps_pinned_package_roots() {
        let dir = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let package_root = dir
            .path()
            .join("packages")
            .join("react")
            .join("18.2.0-sha512-demo")
            .join("package");
        let cache_dir = dir.path().join("cache").join("old");
        std::fs::create_dir_all(&package_root).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(package_root.join("package.json"), br#"{"name":"react"}"#).unwrap();
        std::fs::write(package_root.join(".megagate-package-root.json"), b"{}").unwrap();
        std::fs::write(package_root.join("index.js"), vec![b'a'; 1024]).unwrap();
        std::fs::write(cache_dir.join("old.tgz"), vec![b'b'; 1024]).unwrap();

        let shared = SharedWebCache {
            root: dir.path().to_path_buf(),
        };
        shared
            .write_project_ref(project.path(), [package_root.clone()])
            .unwrap();
        let pinned = read_shared_cache_pinned_package_roots(dir.path());

        prune_shared_cache_to_quota(dir.path(), 1, &pinned).unwrap();

        assert!(
            package_root.join("index.js").exists(),
            "quota pruning must not remove package roots pinned by project refs"
        );
    }

    #[test]
    fn test_project_cas_prune_keeps_hardlinked_live_blobs() {
        let temp = tempfile::tempdir().unwrap();
        let cas = temp.path().join("cas");
        let blob = cas.join("ab").join("live");
        let orphan = cas.join("cd").join("orphan");
        let live_link = temp.path().join("node_modules").join("live");
        std::fs::create_dir_all(blob.parent().unwrap()).unwrap();
        std::fs::create_dir_all(orphan.parent().unwrap()).unwrap();
        std::fs::create_dir_all(live_link.parent().unwrap()).unwrap();
        std::fs::write(&blob, b"live").unwrap();
        std::fs::write(&orphan, b"orphan").unwrap();
        std::fs::hard_link(&blob, &live_link).unwrap();

        prune_unlinked_old_cas_files_under(&cas, std::time::Duration::from_secs(0)).unwrap();

        assert!(blob.exists());
        assert!(live_link.exists());
        assert!(!orphan.exists());
    }

    #[test]
    fn test_backing_link_falls_back_to_hardlink_when_reflink_disabled() {
        use std::os::unix::fs::MetadataExt;
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.txt");
        let target = temp.path().join("target.txt");
        std::fs::write(&source, b"payload-123").unwrap();

        let profile = MaterializationProfile::default();
        backing_link_file(&source, &target, Some(&profile), false).unwrap();

        assert!(target.exists());
        assert_eq!(std::fs::read(&target).unwrap(), b"payload-123");
        assert_eq!(
            std::fs::metadata(&source).unwrap().nlink(),
            2,
            "disabled reflink must produce a real hardlink (shared inode)"
        );
    }

    #[test]
    fn test_backing_link_rematerializes_stale_target() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.txt");
        let target = temp.path().join("target.txt");
        std::fs::write(&source, b"fresh-content").unwrap();
        std::fs::write(&target, b"stale-content").unwrap();

        let profile = MaterializationProfile::default();
        backing_link_file(&source, &target, Some(&profile), true).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"fresh-content");
    }

    #[test]
    fn test_maybe_prune_skips_quota_scan_when_gc_not_due() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache").join("react");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let tarball_path = cache_dir.join("18.2.0.tgz");
        std::fs::write(&tarball_path, vec![b'a'; 1024]).unwrap();

        write_shared_cache_prune_stamp(dir.path()).unwrap();

        let shared = SharedWebCache {
            root: dir.path().to_path_buf(),
        };
        shared.maybe_prune();

        assert!(
            tarball_path.exists(),
            "fresh gc stamp should skip quota pruning on adapter startup"
        );
    }

    #[tokio::test]
    async fn test_alias_dependency_uses_target_metadata_and_range() {
        let shared = tempfile::tempdir().unwrap();

        seed_shared_metadata(
            shared.path(),
            "demo-parent",
            serde_json::json!({
                "name": "demo-parent",
                "description": null,
                "versions": {
                    "1.0.0": {
                        "version": "1.0.0",
                        "dependencies": {
                            "strip-ansi-cjs": "npm:strip-ansi@^6.0.1"
                        },
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": "https://registry.example.test/demo-parent-1.0.0.tgz",
                            "integrity": "sha512-parent"
                        }
                    }
                },
                "dist-tags": { "latest": "1.0.0" }
            }),
        );
        seed_shared_metadata(
            shared.path(),
            "strip-ansi",
            serde_json::json!({
                "name": "strip-ansi",
                "description": null,
                "versions": {
                    "6.0.1": {
                        "version": "6.0.1",
                        "dependencies": null,
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": "https://registry.example.test/strip-ansi-6.0.1.tgz",
                            "integrity": "sha512-strip-ansi"
                        }
                    }
                },
                "dist-tags": { "latest": "6.0.1" }
            }),
        );

        let provider = NpmDependencyProvider::new(
            "http://127.0.0.1:9",
            None,
            Some(SharedWebCache {
                root: shared.path().to_path_buf(),
            }),
        );
        let parent_id = PackageId::new(
            PackageName::new("demo-parent").unwrap(),
            Version::parse("1.0.0").unwrap(),
        );

        let deps = provider.get_dependencies(&parent_id).await.unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].package.as_str(), "strip-ansi-cjs");
        assert_eq!(deps[0].spec, "^6.0.1");

        let versions = provider.get_versions(&deps[0].package).await.unwrap();
        assert!(versions
            .iter()
            .any(|version| version.to_string() == "6.0.1"));
    }

    #[tokio::test]
    async fn test_load_metadata_persists_etag_after_initial_fetch() {
        let shared = tempfile::tempdir().unwrap();
        let Some(listener) = bind_test_listener().await else {
            return;
        };
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_server = hits.clone();

        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                hits_for_server.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf).await;
                let body = r#"{"name":"react","description":null,"versions":{"18.2.0":{"version":"18.2.0","dependencies":null,"optionalDependencies":null,"os":null,"cpu":null,"dist":{"tarball":"http://example.test/react.tgz","integrity":"sha512-react"}}},"dist-tags":{"latest":"18.2.0"}}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"react-v1\"\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });

        let registry = native::npm_registry::NpmRegistry::new(&format!("http://{addr}"));
        let cache = SharedWebCache {
            root: shared.path().to_path_buf(),
        };

        let metadata = load_metadata_by_name_with_fallback("react", &registry, Some(&cache))
            .await
            .unwrap();
        assert_eq!(metadata.name, "react");
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let cached = std::fs::read_to_string(
            shared
                .path()
                .join("metadata")
                .join(reg_key(&format!("http://{addr}")))
                .join("react")
                .join("metadata.json"),
        )
        .unwrap();
        assert!(cached.contains("\"etag\":\"\\\"react-v1\\\"\""));
    }

    #[tokio::test]
    async fn test_prefetch_resolution_metadata_dedupes_aliases_by_source_package() {
        let Some(listener) = bind_test_listener().await else {
            return;
        };
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_server = hits.clone();

        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                hits_for_server.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf).await;
                let body = r#"{"name":"strip-ansi","description":null,"versions":{"6.0.1":{"version":"6.0.1","dependencies":null,"optionalDependencies":null,"os":null,"cpu":null,"dist":{"tarball":"http://example.test/strip-ansi.tgz","integrity":"sha512-strip"}}},"dist-tags":{"latest":"6.0.1"}}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });

        let provider = NpmDependencyProvider::new(&format!("http://{addr}"), None, None);
        let alias_a = PackageName::new("strip-ansi-a").unwrap();
        let alias_b = PackageName::new("strip-ansi-b").unwrap();
        let source = PackageName::new("strip-ansi").unwrap();
        provider.record_alias_target(&alias_a, &source);
        provider.record_alias_target(&alias_b, &source);

        let metadata = provider
            .prefetch_resolution_metadata(&[alias_a.clone(), alias_b.clone()])
            .await
            .unwrap();

        assert_eq!(metadata.len(), 2);
        assert_eq!(metadata[alias_a.as_str()].name, "strip-ansi");
        assert_eq!(metadata[alias_b.as_str()].name, "strip-ansi");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_stale_metadata_failure_sets_retry_cooldown() {
        let _env_guard = env_test_lock().lock().unwrap();
        let shared = tempfile::tempdir().unwrap();
        let Some(listener) = bind_test_listener().await else {
            return;
        };
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_server = hits.clone();

        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                hits_for_server.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf).await;
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 3\r\n\r\nbad",
                    )
                    .await;
            }
        });

        let registry = native::npm_registry::NpmRegistry::new(&format!("http://{addr}"));
        let cache = SharedWebCache {
            root: shared.path().to_path_buf(),
        };
        let metadata: native::npm_registry::PackageMetadata =
            serde_json::from_value(serde_json::json!({
                "name": "react",
                "description": null,
                "versions": {
                    "18.2.0": {
                        "version": "18.2.0",
                        "dependencies": null,
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": "http://example.test/react.tgz",
                            "integrity": "sha512-react"
                        }
                    }
                },
                "dist-tags": {
                    "latest": "18.2.0"
                }
            }))
            .unwrap();

        cache
            .write_metadata_record(
                "react",
                &metadata,
                Some("\"react-v1\"".to_string()),
                current_unix_secs().saturating_sub(metadata_ttl_secs() + 1),
                None,
                &format!("http://{addr}"),
            )
            .unwrap();

        let previous_max_stale = std::env::var_os("MEGAGATE_WEB_METADATA_MAX_STALE_SECS");
        std::env::set_var("MEGAGATE_WEB_METADATA_MAX_STALE_SECS", "604800");

        let first = load_metadata_by_name_with_fallback("react", &registry, Some(&cache))
            .await
            .unwrap();
        let hits_after_first = hits.load(Ordering::SeqCst);
        let second = load_metadata_by_name_with_fallback("react", &registry, Some(&cache))
            .await
            .unwrap();
        restore_env_var("MEGAGATE_WEB_METADATA_MAX_STALE_SECS", previous_max_stale);

        assert_eq!(first.name, "react");
        assert_eq!(second.name, "react");
        assert!(hits_after_first >= 1);
        assert_eq!(hits.load(Ordering::SeqCst), hits_after_first);

        let cached = cache
            .read_metadata("react", &format!("http://{addr}"))
            .unwrap()
            .unwrap();
        assert!(cached.stale_retry_after.is_some());
        assert!(metadata_record_retry_deferred(&cached));
    }

    #[tokio::test]
    async fn test_stale_metadata_too_old_is_not_reused_when_network_fails() {
        let _env_guard = env_test_lock().lock().unwrap();
        let shared = tempfile::tempdir().unwrap();
        let Some(listener) = bind_test_listener().await else {
            return;
        };
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf).await;
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 3\r\n\r\nbad",
                    )
                    .await;
            }
        });

        let registry = native::npm_registry::NpmRegistry::new(&format!("http://{addr}"));
        let cache = SharedWebCache {
            root: shared.path().to_path_buf(),
        };
        let metadata: native::npm_registry::PackageMetadata =
            serde_json::from_value(serde_json::json!({
                "name": "react",
                "description": null,
                "versions": {
                    "18.2.0": {
                        "version": "18.2.0",
                        "dependencies": null,
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": "http://example.test/react.tgz",
                            "integrity": "sha512-react"
                        }
                    }
                },
                "dist-tags": {
                    "latest": "18.2.0"
                }
            }))
            .unwrap();

        let previous = std::env::var_os("MEGAGATE_WEB_METADATA_MAX_STALE_SECS");
        std::env::set_var("MEGAGATE_WEB_METADATA_MAX_STALE_SECS", "60");
        cache
            .write_metadata_record(
                "react",
                &metadata,
                Some("\"react-v1\"".to_string()),
                1,
                None,
                &format!("http://{addr}"),
            )
            .unwrap();

        let err = load_metadata_by_name_with_fallback("react", &registry, Some(&cache))
            .await
            .unwrap_err();
        restore_env_var("MEGAGATE_WEB_METADATA_MAX_STALE_SECS", previous);

        assert!(err
            .to_string()
            .contains("cached metadata is too old to reuse"));
    }

    #[tokio::test]
    async fn test_retry_deferred_does_not_bypass_max_stale_limit() {
        let _env_guard = env_test_lock().lock().unwrap();
        let shared = tempfile::tempdir().unwrap();
        let registry = native::npm_registry::NpmRegistry::new("http://127.0.0.1:9");
        let cache = SharedWebCache {
            root: shared.path().to_path_buf(),
        };
        let metadata: native::npm_registry::PackageMetadata =
            serde_json::from_value(serde_json::json!({
                "name": "react",
                "description": null,
                "versions": {
                    "18.2.0": {
                        "version": "18.2.0",
                        "dependencies": null,
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": "http://example.test/react.tgz",
                            "integrity": "sha512-react"
                        }
                    }
                },
                "dist-tags": {
                    "latest": "18.2.0"
                }
            }))
            .unwrap();

        let previous = std::env::var_os("MEGAGATE_WEB_METADATA_MAX_STALE_SECS");
        std::env::set_var("MEGAGATE_WEB_METADATA_MAX_STALE_SECS", "60");
        cache
            .write_metadata_record(
                "react",
                &metadata,
                Some("\"react-v1\"".to_string()),
                1,
                Some(current_unix_secs().saturating_add(60)),
                "http://127.0.0.1:9",
            )
            .unwrap();

        let err = load_metadata_by_name_with_fallback("react", &registry, Some(&cache))
            .await
            .unwrap_err();
        restore_env_var("MEGAGATE_WEB_METADATA_MAX_STALE_SECS", previous);

        assert!(err
            .to_string()
            .contains("cached metadata is too old to reuse"));
    }

    #[tokio::test]
    async fn test_add_uses_shared_metadata_cache_when_registry_is_unavailable() {
        let shared = tempfile::tempdir().unwrap();

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0"
            })
            .to_string(),
        )
        .unwrap();

        seed_shared_metadata(
            shared.path(),
            "react",
            serde_json::json!({
                "name": "react",
                "description": null,
                "versions": {
                    "18.2.0": {
                        "version": "18.2.0",
                        "dependencies": null,
                        "optionalDependencies": null,
                        "os": null,
                        "cpu": null,
                        "dist": {
                            "tarball": "http://127.0.0.1:9/react.tgz",
                            "integrity": null
                        }
                    }
                },
                "dist-tags": {
                    "latest": "18.2.0"
                }
            }),
        );

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "http://127.0.0.1:9".into(),
            shared.path().to_path_buf(),
        );
        let package_id = adapter
            .add(
                dir.path(),
                &PackageName::new("react").unwrap(),
                None,
                AddOptions::default(),
            )
            .await
            .unwrap();

        assert_eq!(package_id.version().to_string(), "18.2.0");
        let package_json = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
        assert!(package_json.contains("\"react\": \"^18.2.0\""));
    }

    #[tokio::test]
    async fn test_parse_manifest_ignores_workspace_protocol_dependencies() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "frontend",
                "version": "0.1.0",
                "dependencies": {
                    "@core/shared": "workspace:*",
                    "react": "^18.2.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let adapter = WebAdapter::new();
        let manifest = adapter.parse_manifest(dir.path()).await.unwrap();
        assert!(manifest.find_dep("react").is_some());
        assert!(manifest.find_dep("@core/shared").is_none());
    }

    #[tokio::test]
    async fn test_list_prefers_lockfile_state() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "tailwindcss": "^4.3.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let package_dir = dir.path().join("node_modules").join("tailwindcss");
        std::fs::create_dir_all(&package_dir).unwrap();
        std::fs::write(
            package_dir.join("package.json"),
            "{\"name\":\"tailwindcss\",\"version\":\"4.3.2\"}",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("mg.lock"),
            serialization::to_toml(&Lockfile {
                version: 1,
                core: "web".into(),
                mode: "frontend".into(),
                frameworks: vec![],
                resolution: ResolutionMeta {
                    state: "locked".into(),
                    store: "megagate".into(),
                    package_count: 1,
                },
                workspaces: vec![],
                packages: vec![LockPackage {
                    name: "tailwindcss".into(),
                    version: "4.3.2".into(),
                    integrity: Some("sha256-test".into()),
                    direct: true,
                    dev: false,
                    dependencies: vec![],
                    peer_deps: vec![],
                }],
                patches: vec![],
                sig: None,
            })
            .unwrap(),
        )
        .unwrap();

        let adapter = WebAdapter::new();
        let installed = adapter.list(dir.path()).await.unwrap();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].id.name_str(), "tailwindcss");
        assert_eq!(installed[0].id.version().to_string(), "4.3.2");
        assert_eq!(installed[0].integrity.as_deref(), Some("sha256-test"));
    }

    #[tokio::test]
    async fn test_install_multiple_packages_from_cache() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0",
                    "tailwindcss": "^4.3.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let tailwind = PackageId::new(
            PackageName::new("tailwindcss").unwrap(),
            Version::parse("4.3.2").unwrap(),
        );

        let react_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &react,
            &[
                (
                    "package/package.json",
                    br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
                ),
                ("package/index.js", b"export const version = '18.2.0';"),
            ],
        );
        let tailwind_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &tailwind,
            &[
                (
                    "package/package.json",
                    br#"{"name":"tailwindcss","version":"4.3.2"}"#.as_slice(),
                ),
                ("package/index.css", b"@import 'tailwindcss';"),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![
                ResolvedPackage {
                    id: react.clone(),
                    integrity: react_integrity,
                    tarball_url: String::new(),
                    deps: vec![],
                    peer_deps: vec![],
                    direct: true,
                    dev: false,
                },
                ResolvedPackage {
                    id: tailwind.clone(),
                    integrity: tailwind_integrity,
                    tarball_url: String::new(),
                    deps: vec![],
                    peer_deps: vec![],
                    direct: true,
                    dev: false,
                },
            ],
        };

        let adapter = WebAdapter::new();
        let summary = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();
        assert_eq!(summary.added.len(), 2);
        assert!(summary.bytes_from_cache > 0);
        assert!(dir.path().join("node_modules/react/index.js").exists());
        assert!(dir
            .path()
            .join("node_modules/tailwindcss/index.css")
            .exists());

        let installed = adapter.list(dir.path()).await.unwrap();
        assert_eq!(installed.len(), 2);
    }

    #[tokio::test]
    async fn test_install_finalizes_lock_and_cleans_staging_tmp() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0",
                    "@types/react": "^19.2.17"
                }
            })
            .to_string(),
        )
        .unwrap();

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let react_types = PackageId::new(
            PackageName::new("@types/react").unwrap(),
            Version::parse("19.2.17").unwrap(),
        );

        let react_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &react,
            &[
                (
                    "package/package.json",
                    br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
                ),
                ("package/index.js", b"export default 'react';"),
            ],
        );
        let react_types_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &react_types,
            &[
                (
                    "package/package.json",
                    br#"{"name":"@types/react","version":"19.2.17"}"#.as_slice(),
                ),
                ("package/index.d.ts", b"export = React;"),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![
                ResolvedPackage {
                    id: react.clone(),
                    integrity: react_integrity,
                    tarball_url: String::new(),
                    deps: vec![],
                    peer_deps: vec![],
                    direct: true,
                    dev: false,
                },
                ResolvedPackage {
                    id: react_types.clone(),
                    integrity: react_types_integrity,
                    tarball_url: String::new(),
                    deps: vec![],
                    peer_deps: vec![],
                    direct: true,
                    dev: true,
                },
            ],
        };

        let adapter = WebAdapter::new();
        let summary = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();
        assert_eq!(summary.added.len(), 2);
        assert!(dir.path().join("node_modules/react/index.js").exists());
        assert!(dir
            .path()
            .join("node_modules/@types/react/index.d.ts")
            .exists());

        let lock = std::fs::read_to_string(dir.path().join("mg.lock")).unwrap();
        let parsed: Lockfile = serialization::from_toml(&lock).unwrap();
        assert_eq!(parsed.resolution.state, "locked");
        assert_eq!(parsed.resolution.package_count, 2);

        let tmp_dir = dir
            .path()
            .join(".megagate")
            .join("cache")
            .join("web")
            .join("tmp");
        let lingering_entries = std::fs::read_dir(&tmp_dir)
            .unwrap()
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        assert!(
            lingering_entries.is_empty(),
            "expected staging tmp to be cleaned, found {:?}",
            lingering_entries
                .iter()
                .map(|entry| entry.path())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_install_uses_cache_when_registry_is_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "tailwindcss": "^4.3.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let package_id = PackageId::new(
            PackageName::new("tailwindcss").unwrap(),
            Version::parse("4.3.2").unwrap(),
        );
        let integrity = seed_cached_tarball(dir.path(), &package_id);

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: package_id.clone(),
                integrity,
                tarball_url: "http://127.0.0.1:9/unreachable.tgz".into(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::with_registry("http://127.0.0.1:9".into());
        let summary = adapter
            .install(
                &graph,
                dir.path(),
                InstallOptions {
                    legacy_flat: true,
                    ..InstallOptions::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(summary.added, vec![package_id]);
        assert!(summary.bytes_from_cache > 0);
    }

    #[tokio::test]
    async fn test_install_uses_shared_tarball_cache_for_new_project() {
        let shared = tempfile::tempdir().unwrap();

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let integrity = seed_shared_tarball_with_files(
            shared.path(),
            &react,
            &[
                (
                    "package/package.json",
                    br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
                ),
                ("package/index.js", b"export default 'react';"),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: react.clone(),
                integrity,
                tarball_url: "http://127.0.0.1:9/unreachable.tgz".into(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "http://127.0.0.1:9".into(),
            shared.path().to_path_buf(),
        );
        let summary = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();
        assert_eq!(summary.added, vec![react.clone()]);
        assert!(summary.bytes_from_cache > 0);
        assert!(dir.path().join("node_modules/react/index.js").exists());
        assert!(shared
            .path()
            .join("cache")
            .join("react")
            .join("18.2.0.tgz")
            .exists());
    }

    #[tokio::test]
    async fn test_install_recovers_from_corrupted_local_cache_using_shared_cache() {
        let shared = tempfile::tempdir().unwrap();

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let files = vec![
            (
                "package/package.json",
                br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
            ),
            ("package/index.js", b"export default 'react';".as_slice()),
        ];
        let good_tarball = build_tarball_bytes(&files);
        seed_shared_tarball_with_files(shared.path(), &react, &files);
        let local_layout = Layout::new(dir.path().join(".megagate").join("cache").join("web"));
        std::fs::create_dir_all(local_layout.root()).unwrap();
        let local_cache = PackageCache::new(local_layout.cache_dir()).unwrap();
        local_cache.cache_tarball(&react, b"corrupted").unwrap();

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: react.clone(),
                integrity: sri_sha512(&good_tarball),
                tarball_url: "http://127.0.0.1:9/unreachable.tgz".into(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "http://127.0.0.1:9".into(),
            shared.path().to_path_buf(),
        );
        let summary = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();
        assert_eq!(summary.added, vec![react.clone()]);
        assert!(dir.path().join("node_modules/react/index.js").exists());

        assert!(local_cache.get_tarball(&react).unwrap().is_none());
        let shared_cache = PackageCache::new(shared.path().join("cache")).unwrap();
        let repaired = shared_cache.get_tarball(&react).unwrap().unwrap();
        assert_eq!(repaired, good_tarball);
    }

    #[tokio::test]
    async fn test_install_fails_when_registry_is_unavailable_and_cache_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "tailwindcss": "^4.3.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let package_id = PackageId::new(
            PackageName::new("tailwindcss").unwrap(),
            Version::parse("4.3.2").unwrap(),
        );
        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: package_id,
                integrity: String::new(),
                tarball_url: "http://127.0.0.1:9/unreachable.tgz".into(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::build("http://127.0.0.1:9".into(), None, vec![], None);
        let err = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("download failed"));
    }

    #[tokio::test]
    async fn test_install_failure_does_not_materialize_partial_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0",
                    "tailwindcss": "^4.3.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let missing = PackageId::new(
            PackageName::new("tailwindcss").unwrap(),
            Version::parse("4.3.2").unwrap(),
        );

        let react_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &react,
            &[
                (
                    "package/package.json",
                    br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
                ),
                ("package/index.js", b"export default 'react';"),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![
                ResolvedPackage {
                    id: react,
                    integrity: react_integrity,
                    tarball_url: String::new(),
                    deps: vec![],
                    peer_deps: vec![],
                    direct: true,
                    dev: false,
                },
                ResolvedPackage {
                    id: missing,
                    integrity: String::new(),
                    tarball_url: "http://127.0.0.1:9/unreachable.tgz".into(),
                    deps: vec![],
                    peer_deps: vec![],
                    direct: true,
                    dev: false,
                },
            ],
        };

        let adapter = WebAdapter::build("http://127.0.0.1:9".into(), None, vec![], None);
        let _err = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap_err();
        assert!(!dir.path().join("node_modules/react").exists());

        let lock = std::fs::read_to_string(dir.path().join("mg.lock")).unwrap();
        let parsed: Lockfile = serialization::from_toml(&lock).unwrap();
        assert_eq!(parsed.resolution.state, "installing");
        assert_eq!(parsed.packages.len(), 2);
    }

    #[tokio::test]
    async fn test_install_skips_when_matching_package_is_already_materialized() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "zod": "^4.4.3"
                }
            })
            .to_string(),
        )
        .unwrap();

        let package_id = PackageId::new(
            PackageName::new("zod").unwrap(),
            Version::parse("4.4.3").unwrap(),
        );
        let pkg_dir = dir.path().join("node_modules").join("zod");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join("package.json"),
            br#"{"name":"zod","version":"4.4.3"}"#,
        )
        .unwrap();
        std::fs::write(pkg_dir.join("marker.txt"), "keep-me").unwrap();

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: package_id.clone(),
                integrity: String::new(),
                tarball_url: "http://127.0.0.1:9/unreachable.tgz".into(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::with_registry("http://127.0.0.1:9".into());
        let summary = adapter
            .install(
                &graph,
                dir.path(),
                InstallOptions {
                    legacy_flat: true,
                    ..InstallOptions::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(summary.added, vec![package_id]);
        assert_eq!(
            std::fs::read_to_string(pkg_dir.join("marker.txt")).unwrap(),
            "keep-me"
        );

        let lock = std::fs::read_to_string(dir.path().join("mg.lock")).unwrap();
        let parsed: Lockfile = serialization::from_toml(&lock).unwrap();
        assert_eq!(parsed.resolution.state, "locked");
        assert_eq!(parsed.packages[0].version, "4.4.3");
    }

    #[tokio::test]
    async fn test_install_materializes_scoped_package() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "devDependencies": {
                    "@types/node": "26.1.1"
                }
            })
            .to_string(),
        )
        .unwrap();

        let package_id = PackageId::new(
            PackageName::new("@types/node").unwrap(),
            Version::parse("26.1.1").unwrap(),
        );
        let integrity = seed_cached_tarball_with_files(
            dir.path(),
            &package_id,
            &[
                (
                    "package/package.json",
                    br#"{"name":"@types/node","version":"26.1.1"}"#.as_slice(),
                ),
                ("package/index.d.ts", b"export {};"),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: package_id.clone(),
                integrity,
                tarball_url: String::new(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: true,
            }],
        };

        let adapter = WebAdapter::new();
        let summary = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();
        assert_eq!(summary.added, vec![package_id]);
        assert!(dir
            .path()
            .join("node_modules")
            .join("@types")
            .join("node")
            .join("package.json")
            .exists());
        assert!(dir
            .path()
            .join("node_modules")
            .join("@types")
            .join("node")
            .join("index.d.ts")
            .exists());
    }

    #[tokio::test]
    async fn test_install_materializes_nested_conflicting_dependency_versions() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "@nuxt/kit": "1.0.0",
                    "legacy-tool": "1.0.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let nuxt_kit = PackageId::new(
            PackageName::new("@nuxt/kit").unwrap(),
            Version::parse("1.0.0").unwrap(),
        );
        let legacy_tool = PackageId::new(
            PackageName::new("legacy-tool").unwrap(),
            Version::parse("1.0.0").unwrap(),
        );
        let semver7 = PackageId::new(
            PackageName::new("semver").unwrap(),
            Version::parse("7.8.5").unwrap(),
        );
        let semver6 = PackageId::new(
            PackageName::new("semver").unwrap(),
            Version::parse("6.3.1").unwrap(),
        );

        let nuxt_kit_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &nuxt_kit,
            &[(
                "package/package.json",
                br#"{"name":"@nuxt/kit","version":"1.0.0"}"#.as_slice(),
            )],
        );
        let legacy_tool_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &legacy_tool,
            &[(
                "package/package.json",
                br#"{"name":"legacy-tool","version":"1.0.0"}"#.as_slice(),
            )],
        );
        let semver7_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &semver7,
            &[
                (
                    "package/package.json",
                    br#"{"name":"semver","version":"7.8.5","exports":{"./functions/satisfies.js":"./functions/satisfies.js"}}"#
                        .as_slice(),
                ),
                ("package/functions/satisfies.js", b"export default true;\n"),
            ],
        );
        let semver6_integrity = seed_cached_tarball_with_files(
            dir.path(),
            &semver6,
            &[(
                "package/package.json",
                br#"{"name":"semver","version":"6.3.1"}"#.as_slice(),
            )],
        );

        let graph = ResolvedGraph {
            packages: vec![
                ResolvedPackage {
                    id: nuxt_kit.clone(),
                    integrity: nuxt_kit_integrity,
                    tarball_url: String::new(),
                    deps: vec![semver7.clone()],
                    peer_deps: vec![],
                    direct: true,
                    dev: false,
                },
                ResolvedPackage {
                    id: legacy_tool.clone(),
                    integrity: legacy_tool_integrity,
                    tarball_url: String::new(),
                    deps: vec![semver6.clone()],
                    peer_deps: vec![],
                    direct: true,
                    dev: false,
                },
                ResolvedPackage {
                    id: semver7.clone(),
                    integrity: semver7_integrity,
                    tarball_url: String::new(),
                    deps: vec![],
                    peer_deps: vec![],
                    direct: false,
                    dev: false,
                },
                ResolvedPackage {
                    id: semver6.clone(),
                    integrity: semver6_integrity,
                    tarball_url: String::new(),
                    deps: vec![],
                    peer_deps: vec![],
                    direct: false,
                    dev: false,
                },
            ],
        };

        let adapter = WebAdapter::new();
        adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();

        let nested_nuxt_semver = dir
            .path()
            .join("node_modules")
            .join("@nuxt")
            .join("kit")
            .join("node_modules")
            .join("semver");
        assert!(!dir.path().join("node_modules").join("semver").exists());
        assert!(nested_nuxt_semver.exists());
        assert!(nested_nuxt_semver
            .join("functions")
            .join("satisfies.js")
            .exists());
        assert_eq!(
            installed_package_version(&nested_nuxt_semver)
                .unwrap()
                .to_string(),
            "7.8.5"
        );
        assert_eq!(
            installed_package_version(
                &dir.path()
                    .join("node_modules")
                    .join("legacy-tool")
                    .join("node_modules")
                    .join("semver"),
            )
            .unwrap()
            .to_string(),
            "6.3.1"
        );
    }

    #[tokio::test]
    async fn test_install_retries_flaky_tarball_download() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let tarball = build_tarball_bytes(&[
            (
                "package/package.json",
                br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
            ),
            ("package/index.js", b"export default 'react';"),
        ]);
        let integrity = sri_sha512(&tarball);
        let tarball_for_server = tarball.clone();
        let Some(listener) = bind_test_listener().await else {
            return;
        };
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut attempts = 0usize;
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                attempts += 1;
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                if attempts == 1 {
                    let _ = stream
                        .write_all(
                            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 4\r\n\r\nnope",
                        )
                        .await;
                } else {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                        tarball_for_server.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.write_all(&tarball_for_server).await;
                }
            }
        });

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: react.clone(),
                integrity,
                tarball_url: format!("http://{addr}/react-18.2.0.tgz"),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::new();
        let summary = adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();
        assert_eq!(summary.added, vec![react]);
        assert!(dir.path().join("node_modules/react/index.js").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_install_materialization_uses_store_links_from_cached_extract_root() {
        let dir = tempfile::tempdir().unwrap();
        let shared = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let integrity = seed_cached_tarball_with_files(
            dir.path(),
            &react,
            &[
                (
                    "package/package.json",
                    br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
                ),
                ("package/index.js", b"export default 'react';"),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: react.clone(),
                integrity: integrity.clone(),
                tarball_url: String::new(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "https://registry.npmjs.org".into(),
            shared.path().to_path_buf(),
        );
        adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();

        let cache_key = extracted_package_cache_key(&graph.packages[0]);
        let cached_root = shared
            .path()
            .join("packages")
            .join("react")
            .join(cache_key)
            .join("package");
        let installed_file = dir
            .path()
            .join("node_modules")
            .join("react")
            .join("index.js");
        let vstore_link = dir
            .path()
            .join("node_modules")
            .join(".megagate")
            .join(format!("react@{}", react.version()))
            .join("node_modules")
            .join("react");

        let link_meta = std::fs::symlink_metadata(&vstore_link)
            .unwrap_or_else(|_| panic!("vstore link not found at: {}", vstore_link.display()));
        assert!(link_meta.file_type().is_dir());
        let refs = read_shared_cache_pinned_package_roots(shared.path());
        assert!(
            refs.contains(&canonical_or_original(&cached_root)),
            "install should pin store-linked package root in shared cache refs"
        );
        assert_eq!(
            std::fs::read_to_string(&installed_file).unwrap(),
            "export default 'react';"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_install_repairs_broken_store_links_when_shared_packages_are_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let shared = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            serde_json::json!({
                "name": "demo",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let integrity = seed_cached_tarball_with_files(
            dir.path(),
            &react,
            &[
                (
                    "package/package.json",
                    br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
                ),
                ("package/index.js", b"export default 'react';"),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: react.clone(),
                integrity,
                tarball_url: String::new(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "https://registry.npmjs.org".into(),
            shared.path().to_path_buf(),
        );
        adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();

        let installed_file = dir.path().join("node_modules/react/index.js");
        assert!(installed_file.exists());
        std::fs::remove_dir_all(shared.path().join("packages")).unwrap();
        assert!(
            installed_file.exists(),
            "hard-linked install should survive shared cache deletion"
        );

        adapter
            .install(&graph, dir.path(), InstallOptions::default())
            .await
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(&installed_file).unwrap(),
            "export default 'react';"
        );
    }

    #[tokio::test]
    async fn test_install_rebuilds_shared_extracted_root_when_marker_mismatches() {
        let shared = tempfile::tempdir().unwrap();

        let first = tempfile::tempdir().unwrap();
        std::fs::write(
            first.path().join("package.json"),
            serde_json::json!({
                "name": "demo-a",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0"
                }
            })
            .to_string(),
        )
        .unwrap();

        let react = PackageId::new(
            PackageName::new("react").unwrap(),
            Version::parse("18.2.0").unwrap(),
        );
        let integrity = seed_cached_tarball_with_files(
            first.path(),
            &react,
            &[
                (
                    "package/package.json",
                    br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
                ),
                ("package/index.js", b"export default 'react';\n"),
            ],
        );

        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: react.clone(),
                integrity: integrity.clone(),
                tarball_url: String::new(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: false,
            }],
        };

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "https://registry.npmjs.org".into(),
            shared.path().to_path_buf(),
        );
        adapter
            .install(&graph, first.path(), InstallOptions::default())
            .await
            .unwrap();

        let shared_root = shared_extracted_package_root(shared.path(), &graph.packages[0]);
        std::fs::write(shared_root.join("index.js"), "tampered\n").unwrap();
        write_extracted_package_marker(
            &shared_root,
            &ExtractedPackageMarker {
                schema_version: 0,
                name: "react".into(),
                version: "18.2.0".into(),
                integrity: Some(integrity),
                tarball_sha256: "bad-digest".into(),
                file_count: 0,
                unpacked_size: 0,
                file_tree_sha256: "bad-tree".into(),
            },
        )
        .unwrap();

        let second = tempfile::tempdir().unwrap();
        std::fs::write(
            second.path().join("package.json"),
            serde_json::json!({
                "name": "demo-b",
                "version": "0.1.0",
                "dependencies": {
                    "react": "^18.2.0"
                }
            })
            .to_string(),
        )
        .unwrap();
        seed_cached_tarball_with_files(
            second.path(),
            &react,
            &[
                (
                    "package/package.json",
                    br#"{"name":"react","version":"18.2.0"}"#.as_slice(),
                ),
                ("package/index.js", b"export default 'react';\n"),
            ],
        );

        adapter
            .install(&graph, second.path(), InstallOptions::default())
            .await
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(shared_root.join("index.js")).unwrap(),
            "export default 'react';\n"
        );
        assert_eq!(
            std::fs::read_to_string(second.path().join("node_modules/react/index.js")).unwrap(),
            "export default 'react';\n"
        );
        let marker = read_extracted_package_marker(&shared_root)
            .unwrap()
            .unwrap();
        assert_ne!(marker.tarball_sha256, "bad-digest");
    }

    #[tokio::test]
    async fn test_install_rebuilds_cached_root_when_file_tree_is_incomplete() {
        let shared = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let rollup = PackageId::new(
            PackageName::new("rollup").unwrap(),
            Version::parse("4.62.2").unwrap(),
        );
        let integrity = seed_shared_tarball_with_files(
            shared.path(),
            &rollup,
            &[
                (
                    "package/package.json",
                    br#"{"name":"rollup","version":"4.62.2","exports":{"./parseAst":{"import":"./dist/es/parseAst.js","require":"./dist/parseAst.js"}}}"#.as_slice(),
                ),
                ("package/dist/parseAst.js", b"module.exports = {};\n"),
                (
                    "package/dist/es/parseAst.js",
                    b"export const parseAst = () => null;\n",
                ),
            ],
        );
        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: rollup.clone(),
                integrity: integrity.clone(),
                tarball_url: String::new(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: false,
            }],
        };
        let shared_root = shared_extracted_package_root(shared.path(), &graph.packages[0]);
        std::fs::create_dir_all(shared_root.join("dist")).unwrap();
        std::fs::write(
            shared_root.join("package.json"),
            br#"{"name":"rollup","version":"4.62.2"}"#,
        )
        .unwrap();
        std::fs::write(
            shared_root.join("dist/parseAst.js"),
            b"module.exports = {};\n",
        )
        .unwrap();
        let tarball = PackageCache::new(shared.path().join("cache"))
            .unwrap()
            .get_tarball(&rollup)
            .unwrap()
            .unwrap();
        let mut marker =
            expected_extracted_package_marker_from_bytes(&graph.packages[0], &tarball).unwrap();
        marker.schema_version = 0;
        marker.file_count = 0;
        marker.unpacked_size = 0;
        marker.file_tree_sha256.clear();
        write_extracted_package_marker(&shared_root, &marker).unwrap();

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "https://registry.npmjs.org".into(),
            shared.path().to_path_buf(),
        );
        adapter
            .install(&graph, project.path(), InstallOptions::default())
            .await
            .unwrap();

        assert!(project
            .path()
            .join("node_modules/rollup/dist/es/parseAst.js")
            .exists());
    }

    #[tokio::test]
    async fn test_install_rebuilds_schema_v2_root_when_marker_signature_is_missing() {
        let shared = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let entities = PackageId::new(
            PackageName::new("entities").unwrap(),
            Version::parse("7.0.1").unwrap(),
        );
        let integrity = seed_shared_tarball_with_files(
            shared.path(),
            &entities,
            &[
                (
                    "package/package.json",
                    br#"{"name":"entities","version":"7.0.1","exports":{"./decode":{"require":{"default":"./dist/commonjs/decode.js"}}}}"#.as_slice(),
                ),
                ("package/decode.js", b"module.exports = {};\n"),
                ("package/dist/commonjs/decode.js", b"module.exports = {};\n"),
            ],
        );
        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: entities.clone(),
                integrity: integrity.clone(),
                tarball_url: String::new(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: false,
            }],
        };
        let shared_root = shared_extracted_package_root(shared.path(), &graph.packages[0]);
        std::fs::create_dir_all(&shared_root).unwrap();
        std::fs::write(
            shared_root.join("package.json"),
            br#"{"name":"entities","version":"7.0.1","exports":{"./decode":{"require":{"default":"./dist/commonjs/decode.js"}}}}"#,
        )
        .unwrap();
        std::fs::write(shared_root.join("decode.js"), b"module.exports = {};\n").unwrap();

        let tarball = PackageCache::new(shared.path().join("cache"))
            .unwrap()
            .get_tarball(&entities)
            .unwrap()
            .unwrap();
        let mut marker =
            expected_extracted_package_marker_from_bytes(&graph.packages[0], &tarball).unwrap();
        marker.file_count = 0;
        marker.unpacked_size = 0;
        marker.file_tree_sha256.clear();
        write_extracted_package_marker(&shared_root, &marker).unwrap();

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "https://registry.npmjs.org".into(),
            shared.path().to_path_buf(),
        );
        adapter
            .install(&graph, project.path(), InstallOptions::default())
            .await
            .unwrap();

        assert!(project
            .path()
            .join("node_modules/entities/dist/commonjs/decode.js")
            .exists());
    }

    #[tokio::test]
    async fn test_full_cache_validation_rebuilds_v2_root_when_file_tree_is_incomplete() {
        let old = std::env::var_os("MEGAGATE_WEB_VALIDATE_EXTRACTED_CACHE");
        std::env::set_var("MEGAGATE_WEB_VALIDATE_EXTRACTED_CACHE", "1");

        let shared = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let rollup = PackageId::new(
            PackageName::new("rollup").unwrap(),
            Version::parse("4.62.2").unwrap(),
        );
        let integrity = seed_shared_tarball_with_files(
            shared.path(),
            &rollup,
            &[
                (
                    "package/package.json",
                    br#"{"name":"rollup","version":"4.62.2"}"#.as_slice(),
                ),
                ("package/dist/parseAst.js", b"module.exports = {};\n"),
                (
                    "package/dist/es/parseAst.js",
                    b"export const parseAst = () => null;\n",
                ),
            ],
        );
        let graph = ResolvedGraph {
            packages: vec![ResolvedPackage {
                id: rollup.clone(),
                integrity: integrity.clone(),
                tarball_url: String::new(),
                deps: vec![],
                peer_deps: vec![],
                direct: true,
                dev: false,
            }],
        };
        let shared_root = shared_extracted_package_root(shared.path(), &graph.packages[0]);
        std::fs::create_dir_all(shared_root.join("dist")).unwrap();
        std::fs::write(
            shared_root.join("package.json"),
            br#"{"name":"rollup","version":"4.62.2"}"#,
        )
        .unwrap();
        std::fs::write(
            shared_root.join("dist/parseAst.js"),
            b"module.exports = {};\n",
        )
        .unwrap();
        let tarball = PackageCache::new(shared.path().join("cache"))
            .unwrap()
            .get_tarball(&rollup)
            .unwrap()
            .unwrap();
        let marker =
            expected_extracted_package_marker_from_bytes(&graph.packages[0], &tarball).unwrap();
        write_extracted_package_marker(&shared_root, &marker).unwrap();

        let adapter = WebAdapter::with_registry_and_shared_cache(
            "https://registry.npmjs.org".into(),
            shared.path().to_path_buf(),
        );
        adapter
            .install(&graph, project.path(), InstallOptions::default())
            .await
            .unwrap();

        assert!(project
            .path()
            .join("node_modules/rollup/dist/es/parseAst.js")
            .exists());
        restore_env_var("MEGAGATE_WEB_VALIDATE_EXTRACTED_CACHE", old);
    }

    fn seed_cached_tarball(root: &Path, pkg: &PackageId) -> String {
        let package_json = format!(
            "{{\"name\":\"{}\",\"version\":\"{}\"}}",
            pkg.name_str(),
            pkg.version()
        );
        seed_cached_tarball_with_files(
            root,
            pkg,
            &[
                ("package/package.json", package_json.as_bytes()),
                ("package/index.css", b"@import 'tailwindcss';"),
            ],
        )
    }

    fn seed_cached_tarball_with_files(
        root: &Path,
        pkg: &PackageId,
        files: &[(&str, &[u8])],
    ) -> String {
        let layout = Layout::new(root.join(".megagate").join("cache").join("web"));
        std::fs::create_dir_all(layout.root()).unwrap();
        let cache = PackageCache::new(layout.cache_dir()).unwrap();
        let tarball_path = cache.tarball_path(pkg);
        if let Some(parent) = tarball_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let tarball = build_tarball_bytes(files);
        std::fs::write(&tarball_path, &tarball).unwrap();
        sri_sha512(&tarball)
    }

    fn seed_shared_tarball_with_files(
        root: &Path,
        pkg: &PackageId,
        files: &[(&str, &[u8])],
    ) -> String {
        let layout = Layout::new(root.to_path_buf());
        std::fs::create_dir_all(layout.root()).unwrap();
        let cache = PackageCache::new(layout.cache_dir()).unwrap();
        let tarball_path = cache.tarball_path(pkg);
        if let Some(parent) = tarball_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let tarball = build_tarball_bytes(files);
        std::fs::write(&tarball_path, &tarball).unwrap();
        sri_sha512(&tarball)
    }

    fn seed_shared_metadata(root: &Path, package: &str, payload: serde_json::Value) {
        let path = root
            .join("metadata")
            .join("http___127_0_0_1_9") // khớp reg_key của mock registry url (:9)
            .join(package)
            .join("metadata.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, serde_json::to_vec(&payload).unwrap()).unwrap();
    }

    fn build_tarball_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let file = temp.reopen().unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);
        for (path, data) in files {
            write_tar_entry(&mut builder, path, data);
        }
        builder.finish().unwrap();
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();
        std::fs::read(temp.path()).unwrap()
    }

    fn sri_sha512(data: &[u8]) -> String {
        let mut hasher = Sha512::new();
        hasher.update(data);
        format!(
            "sha512-{}",
            base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
        )
    }

    fn restore_env_var(key: &str, previous: Option<std::ffi::OsString>) {
        if let Some(value) = previous {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }

    fn reg_key(url: &str) -> String {
        url.chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect()
    }

    fn env_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_prefetch_defaults_are_conservative() {
        let _guard = env_test_lock().lock().unwrap();
        let old_resolve = std::env::var_os("MEGAGATE_WEB_RESOLVE_PREFETCH");
        std::env::remove_var("MEGAGATE_WEB_RESOLVE_PREFETCH");

        assert!(!resolve_prefetch_enabled());

        restore_env_var("MEGAGATE_WEB_RESOLVE_PREFETCH", old_resolve);
    }

    #[test]
    fn test_prefetch_flag_can_be_enabled_explicitly() {
        let _guard = env_test_lock().lock().unwrap();
        let old_resolve = std::env::var_os("MEGAGATE_WEB_RESOLVE_PREFETCH");
        std::env::set_var("MEGAGATE_WEB_RESOLVE_PREFETCH", "1");

        assert!(resolve_prefetch_enabled());

        restore_env_var("MEGAGATE_WEB_RESOLVE_PREFETCH", old_resolve);
    }

    fn write_tar_entry(builder: &mut Builder<GzEncoder<std::fs::File>>, path: &str, data: &[u8]) {
        let mut header = Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, path, data).unwrap();
    }

    #[test]
    fn test_preferred_saved_range_preserves_strategy() {
        assert_eq!(
            WebAdapter::preferred_saved_range(&VersionRange::parse("^4.0.0").unwrap(), "5.1.2")
                .unwrap()
                .to_string(),
            "^5.1.2"
        );
        assert_eq!(
            WebAdapter::preferred_saved_range(&VersionRange::parse("~4.0.0").unwrap(), "5.1.2")
                .unwrap()
                .to_string(),
            "~5.1.2"
        );
        assert_eq!(
            WebAdapter::preferred_saved_range(&VersionRange::parse("*").unwrap(), "5.1.2")
                .unwrap()
                .to_string(),
            "^5.1.2"
        );
        assert_eq!(
            WebAdapter::preferred_saved_range(&VersionRange::parse("4.0.0").unwrap(), "5.1.2")
                .unwrap()
                .to_string(),
            "5.1.2"
        );
    }

    #[test]
    fn test_preferred_registry_version_prefers_stable_over_prerelease() {
        let metadata = native::npm_registry::PackageMetadata {
            name: "demo".into(),
            description: None,
            versions: std::collections::HashMap::from([
                (
                    "4.4.3".into(),
                    native::npm_registry::VersionInfo {
                        version: "4.4.3".into(),
                        dependencies: None,
                        dev_dependencies: None,
                        optional_dependencies: None,
                        peer_dependencies: None,
                        os: None,
                        cpu: None,
                        dist: None,
                    },
                ),
                (
                    "4.5.0-canary.20260504T180558".into(),
                    native::npm_registry::VersionInfo {
                        version: "4.5.0-canary.20260504T180558".into(),
                        dependencies: None,
                        dev_dependencies: None,
                        optional_dependencies: None,
                        peer_dependencies: None,
                        os: None,
                        cpu: None,
                        dist: None,
                    },
                ),
            ]),
            dist_tags: std::collections::HashMap::from([(
                "latest".into(),
                "4.5.0-canary.20260504T180558".into(),
            )]),
            time: std::collections::HashMap::new(),
        };

        assert_eq!(
            preferred_registry_version(&metadata).as_deref(),
            Some("4.4.3")
        );
    }

    #[test]
    fn test_installed_package_version_reads_real_version() {
        let dir = tempfile::tempdir().unwrap();
        let pkg_dir = dir.path().join("node_modules").join("zod");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join("package.json"),
            br#"{"name":"zod","version":"4.4.3"}"#,
        )
        .unwrap();

        assert_eq!(
            installed_package_version(&pkg_dir).unwrap().to_string(),
            "4.4.3"
        );
    }

    #[test]
    fn test_known_optional_native_binary_supported_only_matches_current_target() {
        let supported = PackageName::new(format!(
            "@esbuild/{}-{}",
            NpmDependencyProvider::current_npm_os(),
            NpmDependencyProvider::current_npm_cpu()
        ))
        .unwrap();
        let unsupported = PackageName::new("@esbuild/linux-s390x").unwrap();
        let unknown = PackageName::new("optional-but-not-native").unwrap();

        assert_eq!(
            NpmDependencyProvider::known_optional_native_binary_supported(&supported),
            Some(true)
        );
        assert_eq!(
            NpmDependencyProvider::known_optional_native_binary_supported(&unsupported),
            Some(false)
        );
        assert_eq!(
            NpmDependencyProvider::known_optional_native_binary_supported(&unknown),
            None
        );
    }

    #[test]
    fn test_installed_package_matches_version() {
        let dir = tempfile::tempdir().unwrap();
        let pkg_dir = dir.path().join("node_modules").join("zod");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join("package.json"),
            br#"{"name":"zod","version":"4.4.3"}"#,
        )
        .unwrap();
        let package_id = PackageId::new(
            PackageName::new("zod").unwrap(),
            Version::parse("4.4.3").unwrap(),
        );
        assert!(installed_package_matches(&pkg_dir, &package_id));
    }
