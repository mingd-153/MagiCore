//! List installed packages that carry lifecycle scripts but have no
//! trust policy yet (npm parity: `approve-scripts --allow-scripts-pending`).
//! (Liệt kê package có script nhưng chưa có policy — để user duyệt.)

use anyhow::{Context, Result};
use mgc_lockfile::project_lock::ProjectWriteLock;
use mgc_store::{Database, Layout};
use std::collections::{HashMap, HashSet};
use std::env;
use std::io::Read;
use std::time::Duration;

// Match the web manifest reader's established 10 MiB bound.
// (Khớp giới hạn 10 MiB đã dùng ở bộ đọc manifest Web.)
const MAX_TRUST_PACKAGE_JSON_BYTES: u64 = 10 * 1024 * 1024;

/// Execute trust pending — Thực thi trust pending
pub fn execute() -> Result<()> {
    let project_root = env::current_dir().context("failed to get current directory")?;
    // Keep the package tree and project trust policy stable for the whole
    // report; never publish a mixed view while install/remove is in flight.
    // (Giữ ổn định cây package và policy trong suốt lần quét.)
    let _scan_lock = acquire_scan_lock(
        &project_root,
        crate::commands::core::shared::writer_lock_timeout(&project_root),
    )?;
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
    let db_policies = load_db_policies(&layout.db_path())?;

    // Committed file policy (mgc.toml [scripts]) — same merge rule as the
    // install gate: deny wins from either source, then allow. A broken
    // table aborts this report instead of silently dropping a deny.
    // (Policy file commit được — cùng quy tắc hợp nhất với gate install.)
    let file_policy = load_file_policy(&project_root)?;

    // Scan installed packages for lifecycle scripts RECURSIVELY — the
    // install runner executes nested node_modules too, so a shallow
    // top-level-only scan would hide scripted transitive deps from
    // review. Symlink cycles (hoisted links) are cut via a canonicalized
    // visited set.
    // (Quét đệ quy khớp runner install — symlink cycle bị cắt.)
    let scripted = scan_scripted_packages(&node_modules)?;

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

fn acquire_scan_lock(root: &std::path::Path, timeout: Duration) -> Result<ProjectWriteLock> {
    let lock = ProjectWriteLock::acquire(root, timeout)
        .map_err(|error| crate::error::trust_pending_lock_failed(root, &error))?;
    crate::commands::core::shared::ensure_no_pending_remove_journal(root, &lock)?;
    Ok(lock)
}

fn load_db_policies(db_path: &std::path::Path) -> Result<HashMap<String, String>> {
    let db = Database::open(db_path)
        .context(crate::error::trust_policy_database_open_failed(db_path))?;
    let rows = db
        .list_trust_policies()
        .context(crate::error::trust_policy_database_read_failed(db_path))?;
    Ok(rows
        .into_iter()
        .map(|(id, policy, _)| (id, policy))
        .collect())
}

fn load_file_policy(
    project_root: &std::path::Path,
) -> Result<Option<mgc_config::project::ScriptsPolicy>> {
    mgc_config::project::load_scripts_table(project_root)
        .map_err(|error| crate::error::trust_scripts_policy_invalid(&error))
}

fn scan_scripted_packages(node_modules: &std::path::Path) -> Result<HashSet<(String, String)>> {
    let mut scripted = HashSet::new();
    let mut visited = HashSet::new();
    let mut stack = vec![node_modules.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let canonical = std::fs::canonicalize(&dir)
            .map_err(|error| crate::error::trust_pending_scan_failed(&dir, &error))?;
        if !visited.insert(canonical) {
            continue;
        }
        for entry in std::fs::read_dir(&dir)
            .map_err(|error| crate::error::trust_pending_scan_failed(&dir, &error))?
        {
            let entry =
                entry.map_err(|error| crate::error::trust_pending_scan_failed(&dir, &error))?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path
                .metadata()
                .map_err(|error| crate::error::trust_pending_scan_failed(&path, &error))?
                .is_dir()
            {
                if name.starts_with('@') {
                    for sub in std::fs::read_dir(&path)
                        .map_err(|error| crate::error::trust_pending_scan_failed(&path, &error))?
                    {
                        let sub = sub.map_err(|error| {
                            crate::error::trust_pending_scan_failed(&path, &error)
                        })?;
                        let sub_path = sub.path();
                        let package_name = format!("{name}/{}", sub.file_name().to_string_lossy());
                        check_package_json(&sub_path, &package_name, &mut scripted)?;
                        push_nested_node_modules(&sub_path, &mut stack)?;
                    }
                    continue;
                }
                check_package_json(&path, &name, &mut scripted)?;
                push_nested_node_modules(&path, &mut stack)?;
            }
        }
    }
    Ok(scripted)
}

fn push_nested_node_modules(
    package_dir: &std::path::Path,
    stack: &mut Vec<std::path::PathBuf>,
) -> Result<()> {
    let nested = package_dir.join("node_modules");
    match nested.metadata() {
        Ok(metadata) if metadata.is_dir() => stack.push(nested),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(crate::error::trust_pending_scan_failed(&nested, &error)),
    }
    Ok(())
}

/// Record (name, version) when this package dir carries lifecycle scripts.
/// (Ghi nhận package có lifecycle scripts.)
fn check_package_json(
    pkg_dir: &std::path::Path,
    name: &str,
    out: &mut HashSet<(String, String)>,
) -> Result<()> {
    let manifest_path = pkg_dir.join("package.json");
    let Some(contents) = read_bounded_package_json(&manifest_path)? else {
        return Ok(());
    };
    let manifest = serde_json::from_slice::<serde_json::Value>(&contents)
        .map_err(|error| crate::error::trust_pending_scan_failed(&manifest_path, &error))?;
    let manifest = manifest.as_object().ok_or_else(|| {
        crate::error::trust_pending_manifest_invalid(&manifest_path, "root must be a JSON object")
    })?;
    let Some(scripts_value) = manifest.get("scripts") else {
        return Ok(());
    };
    let scripts = scripts_value.as_object().ok_or_else(|| {
        crate::error::trust_pending_manifest_invalid(
            &manifest_path,
            "'scripts' must be a JSON object",
        )
    })?;
    for lifecycle_hook in ["preinstall", "install", "postinstall"] {
        if scripts
            .get(lifecycle_hook)
            .is_some_and(|value| !value.is_string())
        {
            return Err(crate::error::trust_pending_manifest_invalid(
                &manifest_path,
                "lifecycle script values must be strings",
            ));
        }
    }
    let has_scripts = ["preinstall", "install", "postinstall"]
        .iter()
        .any(|hook| scripts.contains_key(*hook));
    if !has_scripts {
        return Ok(());
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
    Ok(())
}

fn read_bounded_package_json(path: &std::path::Path) -> Result<Option<Vec<u8>>> {
    let path_kind = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(crate::error::trust_pending_scan_failed(path, &error)),
    };
    if !path_kind.file_type().is_file() {
        return Err(crate::error::trust_pending_manifest_invalid(
            path,
            "must be a regular non-symlink file",
        ));
    }
    if path_kind.len() > MAX_TRUST_PACKAGE_JSON_BYTES {
        return Err(crate::error::trust_pending_manifest_too_large(
            path,
            MAX_TRUST_PACKAGE_JSON_BYTES,
        ));
    }

    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // OPEN_REPARSE_POINT prevents following a swapped-in symlink/reparse point.
        options.custom_flags(0x0020_0000);
    }
    let file = options
        .open(path)
        .map_err(|error| crate::error::trust_pending_scan_failed(path, &error))?;
    let metadata = file
        .metadata()
        .map_err(|error| crate::error::trust_pending_scan_failed(path, &error))?;
    if !metadata.is_file() {
        return Err(crate::error::trust_pending_manifest_invalid(
            path,
            "must be a regular non-symlink file",
        ));
    }
    if metadata.len() > MAX_TRUST_PACKAGE_JSON_BYTES {
        return Err(crate::error::trust_pending_manifest_too_large(
            path,
            MAX_TRUST_PACKAGE_JSON_BYTES,
        ));
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_TRUST_PACKAGE_JSON_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| crate::error::trust_pending_scan_failed(path, &error))?;
    if bytes.len() as u64 > MAX_TRUST_PACKAGE_JSON_BYTES {
        return Err(crate::error::trust_pending_manifest_too_large(
            path,
            MAX_TRUST_PACKAGE_JSON_BYTES,
        ));
    }
    Ok(Some(bytes))
}

#[cfg(test)]
#[path = "test/pending.rs"]
mod tests;
