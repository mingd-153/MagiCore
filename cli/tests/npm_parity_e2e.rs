//! npm parity E2E: the SAME package.json resolved by mgc (native) and by
//! the reference npm MUST pick identical DIRECT versions (both are
//! max-satisfying selectors). Any divergence is a resolver bug — this test
//! exists so the 0.x-caret family can never regress silently.
//! Skipped when npm is unavailable (env-gated, like visual_qa).
//! (Parity npm: cùng package.json, version trực tiếp phải giống nhau.)

#![allow(clippy::unwrap_used)]

use std::process::Command;
use tempfile::TempDir;

fn mgc_binary() -> String {
    std::env::var_os("CARGO_BIN_EXE_mgc")
        .map(std::path::PathBuf::from)
        .map(|p| p.to_string_lossy().to_string())
        .expect("CARGO_BIN_EXE_mgc unavailable — run via cargo test")
}

fn npm_available() -> bool {
    Command::new("npm")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn write_fixture(dir: &std::path::Path) {
    // Tiny, stable, dependency-bearing packages: is-odd -> is-number.
    std::fs::write(
        dir.join("package.json"),
        r#"{
  "name": "parity-fixture",
  "version": "0.1.0",
  "dependencies": {
    "is-odd": "^3.0.1",
    "is-number": "^7.0.0"
  }
}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"parity-fixture\"\nversion = \"0.1.0\"\necosystem = \"web\"\n",
    )
    .unwrap();
}

fn mgc_versions(dir: &std::path::Path) -> std::collections::HashMap<String, Vec<String>> {
    let out = Command::new(mgc_binary())
        .args(["install-web"])
        .current_dir(dir)
        .output()
        .expect("spawn mgc");
    assert!(
        out.status.success(),
        "mgc install-web failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lock = std::fs::read_to_string(dir.join("mgc.lock")).expect("mgc.lock written");
    // Multi-version locks are legitimate (a transitive edge may pin another
    // version) — collect EVERY version per name, never first-match.
    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut name: Option<String> = None;
    for line in lock.lines() {
        let t = line.trim();
        if let Some(n) = t.strip_prefix("name = ") {
            name = Some(n.trim_matches('"').to_string());
        } else if let Some(v) = t.strip_prefix("version = ")
            && let Some(n) = name.take()
        {
            map.entry(n)
                .or_default()
                .push(v.trim_matches('"').to_string());
        }
    }
    map
}

fn npm_versions(dir: &std::path::Path) -> std::collections::HashMap<String, String> {
    let out = Command::new("npm")
        .args(["install", "--package-lock-only", "--ignore-scripts"])
        .current_dir(dir)
        .output()
        .expect("spawn npm");
    assert!(
        out.status.success(),
        "npm install failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lock = std::fs::read_to_string(dir.join("package-lock.json")).expect("package-lock");
    let json: serde_json::Value = serde_json::from_str(&lock).expect("package-lock parses");
    let mut map = std::collections::HashMap::new();
    if let Some(pkgs) = json.get("packages").and_then(|p| p.as_object()) {
        for (path, meta) in pkgs {
            // Top-level (non-nested) entries only — nested duplicates
            // are npm's layout, not resolver decisions.
            if let Some(name) = path
                .strip_prefix("node_modules/")
                .filter(|n| !n.contains("node_modules/"))
                && let Some(v) = meta.get("version").and_then(|v| v.as_str())
            {
                map.insert(name.to_string(), v.to_string());
            }
        }
    }
    map
}

#[test]
fn mgc_direct_versions_match_npm() {
    if !npm_available() {
        eprintln!("SKIP: npm unavailable");
        return;
    }
    let mgc_dir = TempDir::new().unwrap();
    write_fixture(mgc_dir.path());
    let mgc = mgc_versions(mgc_dir.path());

    let npm_dir = TempDir::new().unwrap();
    write_fixture(npm_dir.path());
    let npm = npm_versions(npm_dir.path());

    for (dep, range) in [("is-odd", "^3.0.1"), ("is-number", "^7.0.0")] {
        let mvs = mgc
            .get(dep)
            .unwrap_or_else(|| panic!("mgc lock missing {dep}: {mgc:?}"));
        let nv = npm
            .get(dep)
            .unwrap_or_else(|| panic!("npm lock missing {dep}: {npm:?}"));
        assert!(
            mvs.contains(nv),
            "resolver divergence on direct dep {dep} ({range}): npm={nv} not in mgc={mvs:?}"
        );
    }
}
