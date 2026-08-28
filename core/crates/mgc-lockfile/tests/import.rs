//! Tests cho lockfile import (npm/pnpm/yarn/bun → mgc.lock v2).
//! Toàn bộ hermetic: fixture inline, không mạng, không gọi PM nào.

use mgc_lockfile::{import_file, import_into_lockfile};
use tempfile::TempDir;

fn write_lock(dir: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path
}

// ---------------------------------------------------------------- npm

const NPM_V3: &str = r#"{
  "name": "app", "lockfileVersion": 3,
  "packages": {
    "": { "name": "app", "dependencies": { "foo": "^1.0.0" } },
    "node_modules/foo": {
      "version": "1.2.3",
      "resolved": "https://registry.example/foo/-/foo-1.2.3.tgz",
      "integrity": "sha512-AAAAfoo",
      "dependencies": { "bar": "^2.0.0" }
    },
    "node_modules/bar": {
      "version": "2.0.1",
      "resolved": "https://registry.example/bar/-/bar-2.0.1.tgz",
      "integrity": "sha512-BAAAbar"
    },
    "node_modules/foo/node_modules/bar": {
      "version": "2.9.9",
      "resolved": "https://registry.example/bar/-/bar-2.9.9.tgz",
      "integrity": "sha512-nested"
    },
    "node_modules/@scope/pkg": {
      "version": "0.1.0",
      "resolved": "https://registry.example/@scope/pkg.tgz",
      "integrity": "sha512-scoped"
    },
    "node_modules/link-dep": { "resolved": "packages/local", "link": true }
  }
}"#;

#[test]
fn npm_v3_imports_packages_and_root_level_edges() {
    let tmp = TempDir::new().unwrap();
    let path = write_lock(tmp.path(), "package-lock.json", NPM_V3);

    let (lock, report) = import_file(&path).unwrap();
    assert_eq!(report.source_file, "package-lock.json");

    // link entry không có version bị bỏ; root "" bị bỏ
    assert_eq!(
        report.packages, 4,
        "foo, bar(root), bar(nested), @scope/pkg"
    );
    assert_eq!(lock.packages.len(), 4);

    // sorted theo name
    let names: Vec<_> = lock.packages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["@scope/pkg", "bar", "bar", "foo"]);

    let foo = lock.packages.iter().find(|p| p.name == "foo").unwrap();
    assert_eq!(foo.version, "1.2.3");
    assert_eq!(foo.integrity, "sha512-AAAAfoo");
    // edge chỉ khi lookup thấy đúng node_modules/<dep> ở cấp gốc
    assert_eq!(foo.dependencies, vec!["bar@2.0.1".to_string()]);
}

#[test]
fn npm_unknown_newer_version_imports_by_shape_with_warning() {
    // Chính sách: KHÔNG pin version PM (họ bump liên tục) — shape đúng là import được,
    // version chưa test chỉ sinh cảnh báo.
    let tmp = TempDir::new().unwrap();
    let path = write_lock(
        tmp.path(),
        "package-lock.json",
        r#"{ "lockfileVersion": 99, "packages": {
          "node_modules/x": { "version": "1.0.0", "resolved": "https://r.example/x.tgz", "integrity": "sha512-x" } } }"#,
    );
    let (_, report) = import_file(&path).unwrap();
    assert_eq!(report.packages, 1);
    assert!(
        report.warnings.iter().any(|w| w.contains("99")),
        "phải cảnh báo version chưa test: {:?}",
        report.warnings
    );
}

#[test]
fn npm_known_version_has_no_warnings() {
    let tmp = TempDir::new().unwrap();
    let path = write_lock(tmp.path(), "package-lock.json", NPM_V3);
    let (_, report) = import_file(&path).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn npm_missing_packages_map_rejected_by_structure() {
    let tmp = TempDir::new().unwrap();
    let path = write_lock(
        tmp.path(),
        "package-lock.json",
        r#"{ "lockfileVersion": 1, "dependencies": { "foo": { "version": "1.0.0" } } }"#,
    );
    let err = import_file(&path).unwrap_err();
    assert!(err.to_string().contains("'packages' map"), "{err}");
}

// ---------------------------------------------------------------- pnpm

const PNPM_V9: &str = r#"lockfileVersion: '9.0'

settings:
  autoInstallPeers: true

packages:
  /ansi-styles@4.3.0(such@range):
    resolution: {integrity: sha512-ZB123pnpm}
  /chalk@5.3.0:
    resolution: {integrity: sha512-dITSchalk}
    dependencies:
      ansi-styles: 4.3.0
      missing-pkg: 9.9.9
"#;

#[test]
fn pnpm_v9_imports_with_integrity_and_exact_edges() {
    let tmp = TempDir::new().unwrap();
    let path = write_lock(tmp.path(), "pnpm-lock.yaml", PNPM_V9);

    let (lock, report) = import_file(&path).unwrap();
    assert_eq!(report.packages, 2);
    assert!(
        report.warnings.is_empty(),
        "'9.0' đã tested → không cảnh báo"
    );
    let chalk = lock.packages.iter().find(|p| p.name == "chalk").unwrap();
    assert_eq!(chalk.version, "5.3.0");
    assert_eq!(chalk.integrity, "sha512-dITSchalk");
    // edge exact "4.3.0" giữ lại; "9.9.9" không có trong tập → bỏ
    assert_eq!(chalk.dependencies, vec!["ansi-styles@4.3.0".to_string()]);

    let styles = lock
        .packages
        .iter()
        .find(|p| p.name == "ansi-styles")
        .unwrap();
    assert_eq!(styles.version, "4.3.0", "hậu tố (such@range) phải được cắt");
    assert_eq!(styles.integrity, "sha512-ZB123pnpm");
}

#[test]
fn pnpm_unknown_version_imports_by_shape_with_warning() {
    let tmp = TempDir::new().unwrap();
    let path = write_lock(
        tmp.path(),
        "pnpm-lock.yaml",
        r#"lockfileVersion: '10.0'
packages:
  /x@1.0.0:
    resolution: {integrity: sha512-x}
"#,
    );
    let (_, report) = import_file(&path).unwrap();
    assert_eq!(report.packages, 1);
    assert!(
        report.warnings.iter().any(|w| w.contains("10.0")),
        "phải cảnh báo: {:?}",
        report.warnings
    );
}

#[test]
fn pnpm_missing_packages_map_rejected() {
    let tmp = TempDir::new().unwrap();
    let path = write_lock(
        tmp.path(),
        "pnpm-lock.yaml",
        "lockfileVersion: '9.0'\nimporters:\n  .:\n    dependencies: {}\n",
    );
    let err = import_file(&path).unwrap_err();
    assert!(err.to_string().contains("'packages' map"), "{err}");
}

// ---------------------------------------------------------------- yarn

const YARN_CLASSIC: &str = r##"# THIS IS AN AUTOGENERATED FILE. DO NOT EDIT THIS FILE DIRECTLY.
# yarn lockfile v1

"@scope/lib@^1.0.0":
  version "1.2.3"
  resolved "https://registry.example/@scope/lib/-/lib-1.2.3.tgz"
  integrity sha512-Yscoped

left-pad@1.3.0:
  version "1.3.0"
  resolved "https://registry.example/left-pad/-/left-pad-1.3.0.tgz"
  integrity sha1-yarnleftpad
"##;

#[test]
fn yarn_classic_imports_scoped_and_plain() {
    let tmp = TempDir::new().unwrap();
    let path = write_lock(tmp.path(), "yarn.lock", YARN_CLASSIC);

    let (lock, report) = import_file(&path).unwrap();
    assert_eq!(report.packages, 2);

    let scoped = lock
        .packages
        .iter()
        .find(|p| p.name == "@scope/lib")
        .unwrap();
    assert_eq!(scoped.version, "1.2.3");
    assert_eq!(scoped.integrity, "sha512-Yscoped");

    let plain = lock.packages.iter().find(|p| p.name == "left-pad").unwrap();
    assert_eq!(plain.version, "1.3.0");
    assert_eq!(
        plain.resolved,
        "https://registry.example/left-pad/-/left-pad-1.3.0.tgz"
    );
}

#[test]
fn yarn_berry_metadata_rejected() {
    let tmp = TempDir::new().unwrap();
    let path = write_lock(
        tmp.path(),
        "yarn.lock",
        "__metadata:\n  version: 10\n\"a@npm:1\":\n  version: 1.0.0\n",
    );
    let err = import_file(&path).unwrap_err();
    assert!(err.to_string().contains("berry"), "{err}");
}

// ---------------------------------------------------------------- bun

const BUN_JSON: &str = r#"{
  "workspaces": {},
  "packages": {
    "zod": ["zod@3.22.4", "https://registry.example/zod-3.22.4.tgz", "sha512-bunZod"],
    "@x/kit": ["@x/kit@0.2.0"],
    "no-integrity": ["no-integrity@1.0.0", "https://registry.example/ni.tgz"]
  }
}"#;

#[test]
fn bun_json_imports_array_entries() {
    let tmp = TempDir::new().unwrap();
    let path = write_lock(tmp.path(), "bun.lock", BUN_JSON);

    let (lock, report) = import_file(&path).unwrap();
    assert_eq!(report.packages, 3);

    let zod = lock.packages.iter().find(|p| p.name == "zod").unwrap();
    assert_eq!(zod.version, "3.22.4");
    assert_eq!(zod.resolved, "https://registry.example/zod-3.22.4.tgz");
    assert_eq!(zod.integrity, "sha512-bunZod");

    // entry thiếu tarball/integrity vẫn nhập với chuỗi rỗng
    let kit = lock.packages.iter().find(|p| p.name == "@x/kit").unwrap();
    assert_eq!(kit.version, "0.2.0");
    assert_eq!(kit.resolved, "");
}

#[test]
fn bun_comments_rejected_with_clear_error() {
    let tmp = TempDir::new().unwrap();
    let path = write_lock(
        tmp.path(),
        "bun.lock",
        "{\n  // comment\n  \"packages\": {}\n}",
    );
    let err = import_file(&path).unwrap_err();
    assert!(err.to_string().contains("not valid JSON"), "{err}");
}

// ---------------------------------------------------------------- detect + priority + e2e

#[test]
fn detect_lists_existing_files_in_priority_order() {
    let tmp = TempDir::new().unwrap();
    write_lock(tmp.path(), "yarn.lock", YARN_CLASSIC);
    write_lock(tmp.path(), "package-lock.json", NPM_V3);

    let detected = mgc_lockfile::detect_legacy_lockfiles(tmp.path());
    let names: Vec<_> = detected.iter().map(|l| l.file_name).collect();
    assert_eq!(names.first().copied(), Some("package-lock.json"));
}

#[test]
fn import_into_lockfile_prefers_npm_over_yarn() {
    let tmp = TempDir::new().unwrap();
    write_lock(tmp.path(), "yarn.lock", YARN_CLASSIC);
    write_lock(tmp.path(), "package-lock.json", NPM_V3);

    let (_, report) = import_into_lockfile(tmp.path()).unwrap();
    assert_eq!(report.source_file, "package-lock.json");
}

#[test]
fn imported_output_is_deterministic_sorted() {
    let tmp = TempDir::new().unwrap();
    let path = write_lock(tmp.path(), "package-lock.json", NPM_V3);

    let (a, _) = import_file(&path).unwrap();
    let (b, _) = import_file(&path).unwrap();
    // generated_at luôn khác nhau nên so phần nội dung quyết định
    assert_eq!(
        a.packages, b.packages,
        "import 2 lần phải ra cùng danh sách package"
    );
    assert_eq!(a.version, "2");
}

// --------------------------------------------- audit round: adversarial cases

#[test]
fn pnpm_registry_path_dep_spec_resolves_edge() {
    // deps value dạng registry path "/name@ver" cũng phải dựng được edge
    let tmp = TempDir::new().unwrap();
    let path = write_lock(
        tmp.path(),
        "pnpm-lock.yaml",
        r#"lockfileVersion: '9.0'
packages:
  /a@1.0.0:
    resolution: {integrity: sha512-a}
  /b@2.0.0:
    resolution: {integrity: sha512-b}
    dependencies:
      a: /a@1.0.0
"#,
    );
    let (lock, _) = import_file(&path).unwrap();
    let b = lock.packages.iter().find(|p| p.name == "b").unwrap();
    assert_eq!(b.dependencies, vec!["a@1.0.0".to_string()]);
}

#[test]
fn npm_same_name_version_dedupes() {
    // root-level và nested trùng (name,version) → chỉ giữ 1
    let tmp = TempDir::new().unwrap();
    let path = write_lock(
        tmp.path(),
        "package-lock.json",
        r#"{
  "lockfileVersion": 3,
  "packages": {
    "node_modules/dup": { "version": "1.0.0", "resolved": "https://r.example/dup.tgz", "integrity": "sha512-x" },
    "node_modules/other/node_modules/dup": { "version": "1.0.0", "resolved": "https://r.example/dup.tgz", "integrity": "sha512-x" }
  }
}"#,
    );
    let (_, report) = import_file(&path).unwrap();
    assert_eq!(report.packages, 1, "trùng (name,version) phải dedupe");
}

#[test]
fn yarn_multi_spec_header_uses_first() {
    let tmp = TempDir::new().unwrap();
    let path = write_lock(
        tmp.path(),
        "yarn.lock",
        "\"multi@^1, multi@^2\":\n  version \"1.5.0\"\n  resolved \"https://r.example/multi.tgz\"\n",
    );
    let (lock, report) = import_file(&path).unwrap();
    assert_eq!(report.packages, 1);
    assert_eq!(lock.packages[0].name, "multi");
    assert_eq!(lock.packages[0].version, "1.5.0");
}
