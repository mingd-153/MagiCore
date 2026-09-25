//! List installed packages that carry lifecycle scripts but have no
//! trust policy yet (npm parity: `approve-scripts --allow-scripts-pending`).
//! (Liệt kê package có script nhưng chưa có policy — để user duyệt.)

use anyhow::{Context, Result};
use mgc_store::{Database, Layout};
use std::collections::{HashMap, HashSet};
use std::env;

/// Execute trust pending — Thực thi trust pending
pub fn execute() -> Result<()> {
    let project_root = env::current_dir().context("failed to get current directory")?;
    let node_modules = project_root.join("node_modules");
    if !node_modules.exists() {
        anyhow::bail!(
            "no node_modules in {} — run install first",
            project_root.display()
        );
    }

    // Machine-local DB policies (name[@version] -> approved|denied).
    // (Policy DB máy local.)
    let cache_root = project_root.join(".magicore").join("cache").join("web");
    let layout = Layout::new(cache_root);
    let db_policies: HashMap<String, String> = Database::open(&layout.db_path())
        .and_then(|db| db.list_trust_policies())
        .map(|rows| {
            rows.into_iter()
                .map(|(id, policy, _)| (id, policy))
                .collect()
        })
        .unwrap_or_default();

    // Committed file policy (mgc.toml [scripts]) — same merge rule as the
    // install gate: deny wins from either source, then allow. A broken
    // table warns loudly instead of silently dropping a deny.
    // (Policy file commit được — cùng quy tắc hợp nhất với gate install.)
    let file_policy = match mgc_config::project::load_scripts_table(&project_root) {
        Ok(policy) => policy,
        Err(e) => {
            println!("[magicore] warning: {e} — [scripts] policy ignored");
            None
        }
    };

    // Scan installed packages for lifecycle scripts RECURSIVELY — the
    // install runner executes nested node_modules too, so a shallow
    // top-level-only scan would hide scripted transitive deps from
    // review. Symlink cycles (hoisted links) are cut via a canonicalized
    // visited set.
    // (Quét đệ quy khớp runner install — symlink cycle bị cắt.)
    let mut scripted: HashSet<(String, String)> = HashSet::new();
    let mut visited: HashSet<std::path::PathBuf> = HashSet::new();
    let mut stack = vec![node_modules.clone()];
    while let Some(dir) = stack.pop() {
        let canonical = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        if !visited.insert(canonical) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            let is_dir = path.is_dir();
            if name.starts_with('@') && is_dir {
                let Ok(scoped) = std::fs::read_dir(&path) else {
                    continue;
                };
                for sub in scoped.flatten() {
                    let sub_path = sub.path();
                    check_package_json(
                        &sub_path,
                        &format!("{name}/{}", sub.file_name().to_string_lossy()),
                        &mut scripted,
                    );
                    let nested = sub_path.join("node_modules");
                    if nested.is_dir() {
                        stack.push(nested);
                    }
                }
                continue;
            }
            if is_dir {
                check_package_json(&path, &name, &mut scripted);
                let nested = path.join("node_modules");
                if nested.is_dir() {
                    stack.push(nested);
                }
            }
        }
    }

    let mut approved = Vec::new();
    let mut denied = Vec::new();
    let mut pending = Vec::new();
    let mut scripted: Vec<(String, String)> = scripted.into_iter().collect();
    scripted.sort();
    for (name, version) in scripted {
        let id = format!("{name}@{version}");
        let db = db_policies
            .get(&id)
            .or_else(|| db_policies.get(&name))
            .map(String::as_str);
        // ONE merged decision (mgc-config::decide_scripts — shared with
        // the install gate): deny wins from either source, then allow.
        // Blanket is false here: anything undecided is pending review,
        // never silently allowed.
        // (Một quyết định gộp duy nhất — chưa quyết thì pending.)
        match mgc_config::project::decide_scripts(&name, &version, file_policy.as_ref(), db, false)
        {
            mgc_config::project::ScriptVerdict::Deny(_) => denied.push(id),
            mgc_config::project::ScriptVerdict::Allow(_) => approved.push(id),
            mgc_config::project::ScriptVerdict::Undecided => pending.push(id),
        }
    }

    if !denied.is_empty() {
        println!("Denied (scripts blocked):");
        for id in &denied {
            println!("  ✗ {id}");
        }
    }
    if !approved.is_empty() {
        println!("Approved (scripts will run):");
        for id in &approved {
            println!("  ✓ {id}");
        }
    }
    if pending.is_empty() {
        println!("No pending packages — every installed lifecycle script has a policy.");
    } else {
        println!("Pending review (scripts SKIPPED on install until approved):");
        for id in &pending {
            println!("  ? {id} — approve with: mgc trust approve {id}");
        }
    }
    Ok(())
}

/// Record (name, version) when this package dir carries lifecycle scripts.
/// (Ghi nhận package có lifecycle scripts.)
fn check_package_json(pkg_dir: &std::path::Path, name: &str, out: &mut HashSet<(String, String)>) {
    let manifest_path = pkg_dir.join("package.json");
    let Ok(contents) = std::fs::read_to_string(&manifest_path) else {
        return;
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return;
    };
    let has_scripts = manifest
        .get("scripts")
        .and_then(|s| s.as_object())
        .map(|scripts| {
            scripts.contains_key("preinstall")
                || scripts.contains_key("install")
                || scripts.contains_key("postinstall")
        })
        .unwrap_or(false);
    if !has_scripts {
        return;
    }
    // Prefer the manifest's own `name` (directory layouts like
    // `.store/name@version` would otherwise mislabel the package and
    // break policy matching).
    // (Ưu tiên trường name trong manifest hơn tên thư mục.)
    let name = manifest
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|n| !n.is_empty())
        .unwrap_or(name)
        .to_string();
    let version = manifest
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    out.insert((name, version));
}
