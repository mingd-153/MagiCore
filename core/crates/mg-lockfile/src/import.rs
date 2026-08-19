//! Legacy lockfile migration parser (Phase 1A).
//!
//! Read legacy package-manager lockfiles as data only (serde_json / serde_yaml,
//! no subprocess, no dependency on any PM) and map them into `Lockfile`.
//! This module is migration-only: normal install must use `mg.lock` or resolve
//! natively with MegaGate.

use crate::{LockPackage, Lockfile, ResolutionMeta};
use mg_types::Manifest;
use std::path::Path;

pub const NPM_LOCKFILE: &str = "package-lock.json";
pub const PNPM_LOCKFILE: &str = "pnpm-lock.yaml";
pub const YARN_LOCKFILE: &str = "yarn.lock";
pub const BUN_LOCKFILE: &str = "bun.lock";

pub const ALL: [&str; 4] = [NPM_LOCKFILE, PNPM_LOCKFILE, YARN_LOCKFILE, BUN_LOCKFILE];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyLockfile {
    pub file_name: &'static str,
    pub path: std::path::PathBuf,
}

/// Detect supported legacy lockfiles without importing them.
/// Migration must be explicit; install callers should use this only for hints.
pub fn detect_legacy_lockfiles(project_root: &Path) -> Vec<LegacyLockfile> {
    ALL.iter()
        .filter_map(|name| {
            let path = project_root.join(name);
            path.exists().then_some(LegacyLockfile {
                file_name: name,
                path,
            })
        })
        .collect()
}

/// Phát hiện nguy cơ Trust-Downgrade: Dự án đã có `mg.lock` (bảo mật cao, có checksum & chữ ký BLAKE3)
/// nhưng lại xuất hiện file lockfile cũ chưa được đồng bộ hoặc bị dev dùng tool cũ ghi đè.
pub fn check_trust_downgrade_risk(project_root: &Path) -> Option<Vec<&'static str>> {
    let mg_lock = project_root.join(crate::LOCKFILE_NAME);
    if !mg_lock.exists() {
        return None;
    }
    let legacy = detect_legacy_lockfiles(project_root);
    if legacy.is_empty() {
        None
    } else {
        Some(legacy.into_iter().map(|l| l.file_name).collect())
    }
}


/// Explicitly import a legacy package-manager lockfile found in `project_root`.
/// Returns `None` when no supported lockfile is present.
/// Priority when several exist: npm > pnpm > yarn > bun.
pub fn import_legacy_lockfile_explicit(
    project_root: &Path,
    core: &str,
    mode: &str,
    manifest: &Manifest,
) -> anyhow::Result<Option<Lockfile>> {
    for name in ALL {
        let path = project_root.join(name);
        if !path.exists() {
            continue;
        }
        let contents = std::fs::read_to_string(&path)?;
        return Ok(Some(match name {
            NPM_LOCKFILE => import_npm(core, &contents, mode, manifest)?,
            PNPM_LOCKFILE => import_pnpm(core, &contents, mode, manifest)?,
            YARN_LOCKFILE => import_yarn(core, &contents, mode, manifest)?,
            BUN_LOCKFILE => import_bun(core, &contents, mode, manifest)?,
            _ => unreachable!(),
        }));
    }
    Ok(None)
}

fn lockfile_with(
    core: &str,
    mode: &str,
    manifest: &Manifest,
    packages: Vec<LockPackage>,
) -> Lockfile {
    let mut lock = Lockfile::new(core, mode);
    lock.resolution = ResolutionMeta {
        state: "locked".into(),
        store: "megagate".into(),
        package_count: packages.len(),
    };
    let direct: Vec<&mg_types::DependencySpec> = manifest
        .dependencies
        .iter()
        .chain(manifest.optional_dependencies.iter())
        .collect();
    let dev: Vec<&mg_types::DependencySpec> = manifest
        .dev_dependencies
        .iter()
        .chain(manifest.optional_dependencies.iter())
        .collect();
    lock.packages = packages
        .into_iter()
        .map(|mut p| {
            // Mark direct only when the locked version satisfies the manifest
            // range — nested duplicates of the same name stay transitive.
            let range_ok = |spec: &mg_types::DependencySpec| -> bool {
                spec.name.as_str() == p.name
                    && mg_types::Version::parse(&p.version)
                        .map(|v| spec.range.matches(&v))
                        .unwrap_or(false)
            };
            p.direct = direct.iter().any(|s| range_ok(s));
            p.dev = dev.iter().any(|s| range_ok(s));
            p
        })
        .collect();
    lock.packages
        .sort_by(|a, b| a.name.cmp(&b.name).then(a.version.cmp(&b.version)));
    lock.packages
        .dedup_by(|a, b| a.name == b.name && a.version == b.version);
    lock
}

fn package(name: &str, version: &str, integrity: Option<String>, deps: Vec<String>) -> LockPackage {
    LockPackage {
        name: name.to_string(),
        version: version.to_string(),
        integrity,
        direct: false,
        dev: false,
        dependencies: deps,
        peer_deps: vec![],
    }
}

// ---------------------------------------------------------------------------
// npm — package-lock.json v2/v3 (JSON, "packages" map)
// ---------------------------------------------------------------------------

struct NpmEntry {
    path: String,
    name: String,
    version: String,
    integrity: Option<String>,
    deps: Vec<String>,
}

fn import_npm(
    core: &str,
    contents: &str,
    mode: &str,
    manifest: &Manifest,
) -> anyhow::Result<Lockfile> {
    let doc: serde_json::Value = serde_json::from_str(contents)
        .map_err(|e| anyhow::anyhow!("invalid package-lock.json: {e}"))?;
    let lv = doc
        .get("lockfileVersion")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if !(2..=3).contains(&lv) {
        anyhow::bail!(
            "package-lock.json lockfileVersion {lv} is not supported (supported: 2, 3). \
             Regenerate it with a supported tool instead."
        );
    }
    let packages = doc
        .get("packages")
        .and_then(|p| p.as_object())
        .ok_or_else(|| anyhow::anyhow!("package-lock.json missing 'packages' map"))?;

    let mut entries: Vec<NpmEntry> = vec![];
    for (key, val) in packages {
        if key.is_empty() || !key.starts_with("node_modules/") {
            continue; // root "" and non-installed entries
        }
        let Some(obj) = val.as_object() else { continue };
        // npm v3 entries carry no "name" field: derive it from the path
        // (last "node_modules/" segment keeps scoped names like @scope/util).
        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .or_else(|| key.rsplit_once("node_modules/").map(|(_, n)| n))
            .unwrap_or("");
        let Some(version) = obj.get("version").and_then(|v| v.as_str()) else {
            continue;
        };
        if name.is_empty() || version.is_empty() {
            continue;
        }
        let integrity = obj
            .get("integrity")
            .and_then(|v| v.as_str())
            .map(String::from);
        entries.push(NpmEntry {
            path: key.clone(),
            name: name.to_string(),
            version: version.to_string(),
            integrity,
            deps: vec![],
        });
    }

    // Version lookup: exact nested path wins, root-level node_modules/<name>
    // may be shadowed by a deeper entry (e.g. duplicate versions).
    let resolve = |entries: &[NpmEntry], parent_path: &str, dep_name: &str| -> Option<String> {
        let nested = format!("{parent_path}/node_modules/{dep_name}");
        entries
            .iter()
            .find(|e| e.path == nested)
            .map(|e| e.version.clone())
            .or_else(|| {
                entries
                    .iter()
                    .find(|e| e.name == dep_name && e.path == format!("node_modules/{dep_name}"))
                    .map(|e| e.version.clone())
            })
    };

    let mut deps_all: Vec<Vec<String>> = Vec::with_capacity(entries.len());
    for i in 0..entries.len() {
        let mut deps = vec![];
        if let Some(obj) = packages.get(&entries[i].path).and_then(|v| v.as_object()) {
            if let Some(dmap) = obj.get("dependencies").and_then(|v| v.as_object()) {
                for dname in dmap.keys() {
                    if let Some(v) = resolve(&entries, &entries[i].path, dname) {
                        deps.push(format!("{dname}@{v}"));
                    }
                }
            }
        }
        deps_all.push(deps);
    }
    for (i, entry) in entries.iter_mut().enumerate() {
        entry.deps = std::mem::take(&mut deps_all[i]);
    }

    let packages = entries
        .into_iter()
        .map(|e| package(&e.name, &e.version, e.integrity, e.deps))
        .collect();
    Ok(lockfile_with(core, mode, manifest, packages))
}

// ---------------------------------------------------------------------------
// pnpm — pnpm-lock.yaml (v6/v9, YAML, "packages" map keyed "name@version")
// ---------------------------------------------------------------------------

fn import_pnpm(
    core: &str,
    contents: &str,
    mode: &str,
    manifest: &Manifest,
) -> anyhow::Result<Lockfile> {
    let doc: serde_yaml::Value = serde_yaml::from_str(contents)
        .map_err(|e| anyhow::anyhow!("invalid pnpm-lock.yaml: {e}"))?;
    let packages = doc
        .get("packages")
        .and_then(|p| p.as_mapping())
        .ok_or_else(|| anyhow::anyhow!("pnpm-lock.yaml missing 'packages' map"))?;

    let mut out = vec![];
    for (key, val) in packages {
        let (Some(key), Some(obj)) = (key.as_str(), val.as_mapping()) else {
            continue;
        };
        // key is "name@version" — the '@' separating version is the last one.
        let Some(pos) = key.rfind('@') else { continue };
        let version = &key[pos + 1..];
        let name = &key[..pos];
        if name.is_empty()
            || version.is_empty()
            || !version.chars().next().is_some_and(|c| c.is_ascii_digit())
        {
            continue;
        }
        let integrity = obj
            .get("resolution")
            .and_then(|r| r.as_mapping())
            .and_then(|r| r.get("integrity"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let mut deps = vec![];
        for dep_name in ["dependencies", "optionalDependencies"] {
            if let Some(m) = obj.get(dep_name).and_then(|v| v.as_mapping()) {
                for (dname, dver) in m {
                    if let (Some(dname), Some(dver)) = (dname.as_str(), dver.as_str()) {
                        deps.push(format!("{dname}@{dver}"));
                    }
                }
            }
        }
        out.push(package(name, version, integrity, deps));
    }
    Ok(lockfile_with(core, mode, manifest, out))
}

// ---------------------------------------------------------------------------
// yarn — yarn.lock v1 (text blocks, `name@range:` header + indented fields)
// ---------------------------------------------------------------------------

fn import_yarn(
    core: &str,
    contents: &str,
    mode: &str,
    manifest: &Manifest,
) -> anyhow::Result<Lockfile> {
    if contents.contains("__metadata:") {
        anyhow::bail!(
            "yarn.lock is berry format (v2+); only yarn v1 lockfiles are supported. \
             Regenerate it with yarn v1 or mg instead."
        );
    }

    // yarn v1 fields are whitespace-separated: `version "1.3.0"`, `integrity sha512-...`.
    // `dependencies:` sections list nested `"name" "range"` lines — skip those.
    struct Block {
        keys: Vec<String>,
        version: Option<String>,
        integrity: Option<String>,
    }
    let mut blocks: Vec<Block> = vec![];
    let mut current: Option<Block> = None;
    for raw in contents.lines() {
        let line = raw.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !line.starts_with(' ') && !line.starts_with('\t') {
            // header: `"left-pad@^1.3.0", "other@npm:1.0.0":`
            if let Some(stripped) = line.strip_suffix(':') {
                if let Some(block) = current.take() {
                    blocks.push(block);
                }
                let keys: Vec<String> = stripped
                    .split(',')
                    .map(|k| k.trim().trim_matches('"').to_string())
                    .filter(|k| !k.is_empty())
                    .collect();
                if !keys.is_empty() {
                    current = Some(Block {
                        keys,
                        version: None,
                        integrity: None,
                    });
                }
            }
            continue;
        }
        if let Some(block) = current.as_mut() {
            let t = line.trim();
            if let Some(pos) = t.find(char::is_whitespace) {
                let (key, value) = (&t[..pos], t[pos..].trim().trim_matches('"'));
                if key.starts_with('"') {
                    continue; // nested dependency `"name" "range"` line
                }
                match key {
                    "version" => block.version = Some(value.to_string()),
                    "integrity" => block.integrity = Some(value.to_string()),
                    _ => {}
                }
            }
        }
    }
    if let Some(block) = current.take() {
        blocks.push(block);
    }

    let mut out = vec![];
    for block in blocks {
        let Some(version) = block.version else {
            continue;
        };
        for key in block.keys {
            // key is "name@range" (scoped: "@scope/name@range") — split at last '@'.
            let Some(pos) = key.rfind('@') else { continue };
            let mut name = key[..pos].to_string();
            if let Some(stripped) = name.strip_prefix("npm:") {
                name = stripped.to_string();
            }
            if name.is_empty() {
                continue;
            }
            out.push(package(&name, &version, block.integrity.clone(), vec![]));
        }
    }
    Ok(lockfile_with(core, mode, manifest, out))
}

// ---------------------------------------------------------------------------
// bun — bun.lock (JSON v1, array entries ["name@version", ver, meta, integrity])
// ---------------------------------------------------------------------------

fn import_bun(
    core: &str,
    contents: &str,
    mode: &str,
    manifest: &Manifest,
) -> anyhow::Result<Lockfile> {
    let doc: serde_json::Value = serde_json::from_str(contents)
        .map_err(|e| anyhow::anyhow!("invalid bun.lock (JSON expected): {e}"))?;
    let lv = doc
        .get("lockfileVersion")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if lv != 1 {
        anyhow::bail!(
            "bun.lock lockfileVersion {lv} is not supported (supported: 1 JSON). \
             Regenerate it with bun >= 1.3 or mg instead."
        );
    }
    let packages = doc
        .get("packages")
        .and_then(|p| p.as_object())
        .ok_or_else(|| anyhow::anyhow!("bun.lock missing 'packages' map"))?;

    let mut out = vec![];
    for (key, val) in packages {
        let id = match val {
            serde_json::Value::Array(arr) => arr.first(),
            serde_json::Value::Object(obj) => obj.get("id"),
            _ => None,
        }
        .and_then(|v| v.as_str())
        .unwrap_or(key);
        let Some(pos) = id.rfind('@') else { continue };
        let name = &id[..pos];
        let version = &id[pos + 1..];
        if name.is_empty() || version.is_empty() {
            continue;
        }
        let integrity = match val {
            serde_json::Value::Array(arr) => arr.last().and_then(|v| v.as_str()).map(String::from),
            _ => None,
        };
        out.push(package(name, version, integrity, vec![]));
    }
    Ok(lockfile_with(core, mode, manifest, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mg_types::DependencySpec;
    use mg_types::{Ecosystem, PackageName, VersionRange};

    fn dep(name: &str) -> DependencySpec {
        DependencySpec::new(PackageName::new(name).unwrap(), VersionRange::star())
    }

    fn manifest_with(deps: &[&str], dev: &[&str]) -> Manifest {
        let mut m = Manifest::new("demo", Ecosystem::Web);
        for d in deps {
            m.add_dep(dep(d), false, false, false);
        }
        for d in dev {
            m.add_dep(dep(d), true, false, false);
        }
        m
    }

    const NPM_V3: &str = r#"{
  "name": "demo",
  "version": "1.0.0",
  "lockfileVersion": 3,
  "requires": true,
  "packages": {
    "": { "name": "demo", "version": "1.0.0", "dependencies": { "left-pad": "^1.3.0" } },
    "node_modules/left-pad": {
      "version": "1.3.0",
      "integrity": "sha512-abc",
      "dependencies": { "@scope/util": "^2.0.0" }
    },
    "node_modules/@scope/util": {
      "version": "2.1.0",
      "resolved": "https://registry.npmjs.org/@scope/util/-/util-2.1.0.tgz"
    },
    "node_modules/@scope/util/node_modules/left-pad": {
      "version": "1.2.0",
      "dev": true
    }
  }
}"#;

    #[test]
    fn import_npm_v3_maps_packages_and_edges() {
        let mut m = manifest_with(&["left-pad"], &[]);
        m.add_dep(
            DependencySpec::new(
                PackageName::new("left-pad").unwrap(),
                VersionRange::parse("^1.3.0").unwrap(),
            ),
            false,
            false,
            false,
        );
        let lock = import_npm("web", NPM_V3, "frontend", &m).unwrap();
        assert_eq!(lock.resolution.state, "locked");
        assert_eq!(lock.packages.len(), 3);

        let lp = lock
            .packages
            .iter()
            .find(|p| p.name == "left-pad" && p.version == "1.3.0")
            .unwrap();
        assert!(lp.direct);
        assert_eq!(lp.integrity.as_deref(), Some("sha512-abc"));
        assert!(lp.dependencies.iter().any(|d| d == "@scope/util@2.1.0"));

        let nested = lock
            .packages
            .iter()
            .find(|p| p.name == "left-pad" && p.version == "1.2.0")
            .unwrap();
        assert!(!nested.direct);
        assert!(nested.dependencies.is_empty());
    }

    #[test]
    fn import_npm_rejects_unsupported_version() {
        let v4 = r#"{"lockfileVersion": 4, "packages": {}}"#;
        let err = import_npm("web", v4, "web", &manifest_with(&[], &[])).unwrap_err();
        assert!(err.to_string().contains("lockfileVersion 4"), "{err}");
    }

    #[test]
    fn import_npm_v2_supported() {
        let v2 = r#"{"lockfileVersion": 2, "packages": { "": {}, "node_modules/x": { "version": "1.0.0", "name": "x" } }}"#;
        let lock = import_npm("web", v2, "web", &manifest_with(&["x"], &[])).unwrap();
        assert_eq!(lock.packages.len(), 1);
        assert_eq!(lock.packages[0].name, "x");
    }

    const PNPM_V9: &str = r#"---
lockfileVersion: '9.0'

importers:
  .:
    dependencies:
      left-pad: 1.3.0

packages:
  left-pad@1.3.0:
    resolution: {integrity: sha512-xyz}
  '@scope/util@2.1.0':
    resolution: {integrity: sha512-util}
    dependencies:
      left-pad: 1.3.0
"#;

    #[test]
    fn import_pnpm_maps_packages_and_edges() {
        let m = manifest_with(&["left-pad"], &[]);
        let lock = import_pnpm("web", PNPM_V9, "frontend", &m).unwrap();
        assert_eq!(lock.packages.len(), 2);

        let lp = lock.packages.iter().find(|p| p.name == "left-pad").unwrap();
        assert_eq!(lp.version, "1.3.0");
        assert!(lp.direct);
        assert_eq!(lp.integrity.as_deref(), Some("sha512-xyz"));

        let util = lock
            .packages
            .iter()
            .find(|p| p.name == "@scope/util")
            .unwrap();
        assert!(!util.direct);
        assert!(util.dependencies.iter().any(|d| d == "left-pad@1.3.0"));
    }

    const YARN_V1: &str = r#"# THIS IS AN AUTOGENERATED FILE. DO NOT EDIT THIS FILE DIRECTLY.
# yarn lockfile v1

"left-pad@^1.3.0":
  version "1.3.0"
  resolved "https://registry.yarnpkg.com/left-pad/-/left-pad-1.3.0.tgz#a"
  integrity sha512-aaa
  dependencies:
    "@scope/util" "^2.0.0"

"@scope/util@^2.0.0":
  version "2.1.0"
  resolved "https://registry.yarnpkg.com/@scope/util/-/util-2.1.0.tgz#b"
  integrity sha512-bbb
"#;

    #[test]
    fn import_yarn_v1_maps_packages() {
        let m = manifest_with(&["left-pad"], &[]);
        let lock = import_yarn("web", YARN_V1, "frontend", &m).unwrap();
        assert_eq!(lock.packages.len(), 2);
        let lp = lock.packages.iter().find(|p| p.name == "left-pad").unwrap();
        assert_eq!(lp.version, "1.3.0");
        assert!(lp.direct);
        assert_eq!(lp.integrity.as_deref(), Some("sha512-aaa"));
        let util = lock
            .packages
            .iter()
            .find(|p| p.name == "@scope/util")
            .unwrap();
        assert!(!util.direct);
        assert_eq!(util.version, "2.1.0");
    }

    #[test]
    fn import_yarn_rejects_berry() {
        let berry =
            "# This file is generated by running \"yarn install\"\n__metadata:\n  version: 10\n";
        let err = import_yarn("web", berry, "web", &manifest_with(&[], &[])).unwrap_err();
        assert!(err.to_string().contains("berry"), "{err}");
    }

    const BUN_V1: &str = r#"{
  "lockfileVersion": 1,
  "configVersion": 1,
  "workspaces": {
    "": { "name": "demo", "dependencies": { "left-pad": "^1.3.0" } }
  },
  "overrides": {},
  "packages": {
    "left-pad@1.3.0": ["left-pad@1.3.0", "", {}, "sha512-ccc"],
    "@scope/util@2.1.0": ["@scope/util@2.1.0", "", {}, "sha512-ddd"]
  }
}"#;

    #[test]
    fn import_bun_v1_maps_packages() {
        let m = manifest_with(&["left-pad"], &[]);
        let lock = import_bun("web", BUN_V1, "frontend", &m).unwrap();
        assert_eq!(lock.packages.len(), 2);
        let lp = lock.packages.iter().find(|p| p.name == "left-pad").unwrap();
        assert_eq!(lp.version, "1.3.0");
        assert_eq!(lp.integrity.as_deref(), Some("sha512-ccc"));
        assert!(lp.direct);
        let util = lock
            .packages
            .iter()
            .find(|p| p.name == "@scope/util")
            .unwrap();
        assert_eq!(util.version, "2.1.0");
        assert!(!util.direct);
    }

    // End-to-end: explicit legacy migration picks npm first when several exist.
    #[test]
    fn import_legacy_lockfile_explicit_detects_and_prioritizes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(NPM_LOCKFILE), NPM_V3).unwrap();
        std::fs::write(dir.path().join(PNPM_LOCKFILE), PNPM_V9).unwrap();
        let m = manifest_with(&["left-pad"], &[]);
        let detected = detect_legacy_lockfiles(dir.path());
        assert_eq!(detected.len(), 2);
        assert_eq!(detected[0].file_name, NPM_LOCKFILE);

        let lock = import_legacy_lockfile_explicit(dir.path(), "web", "frontend", &m)
            .unwrap()
            .expect("npm lockfile should be picked");
        assert!(lock.packages.iter().any(|p| p.name == "@scope/util"));
        assert!(lock.packages.iter().any(|p| p.name == "left-pad"));

        let empty = tempfile::tempdir().unwrap();
        assert!(detect_legacy_lockfiles(empty.path()).is_empty());
        assert!(
            import_legacy_lockfile_explicit(empty.path(), "web", "frontend", &m)
                .unwrap()
                .is_none()
        );
    }


    #[test]
    fn check_trust_downgrade_risk_detects_coexisting_legacy_locks() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Chưa có mg.lock -> không có risk downgrade
        assert!(check_trust_downgrade_risk(root).is_none());

        // Tạo mg.lock
        std::fs::write(root.join(crate::LOCKFILE_NAME), "version = 1\ncore = \"web\"\nmode = \"frontend\"\n").unwrap();
        // Chỉ có mg.lock -> an toàn
        assert!(check_trust_downgrade_risk(root).is_none());

        // Xuất hiện pnpm-lock.yaml song song với mg.lock -> phát hiện rủi ro
        std::fs::write(root.join(PNPM_LOCKFILE), "lockfileVersion: '9.0'").unwrap();
        let risks = check_trust_downgrade_risk(root).unwrap();
        assert_eq!(risks, vec![PNPM_LOCKFILE]);
    }
}

