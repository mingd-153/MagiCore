//! `scanners/dotnet.rs` — .NET dependency CVE scanner via OSV.dev
//! (`NuGet` ecosystem — verified live 2026-09-10: `Newtonsoft.Json@
//! 12.0.2` → GHSA-5crp-9r3c-p9vr; `13.0.1` clean). P2 2026-09-10
//! matrix row "Lib .NET dotnet list package --vulnerable" — this lane
//! does not need the dotnet CLI: the NuGet lockfile is machine-written
//! JSON with a stable shape.
//!
//! Source read: `packages.lock.json` (written by
//! `dotnet restore --locked-mode` / RestoreLockedMode) — the `dependencies`
//! maps per-target-framework; direct + transitive both pinned. Pins are
//! deduplicated across TFMs (one package audited once).
//!
//! Scanner CVE dependency .NET qua OSV.dev (ecosystem `NuGet`). Đọc
//! packages.lock.json (JSON máy ghi, shape ổn định) — map dependencies
//! theo target-framework, ghim direct + transitive, khử trùng giữa TFM.

use crate::scanners::osv::{OsvPin, audit_osv_pins};
use mgc_types::adapter::{AuditReport, ScannerStatus};
use mgc_types::{MgError, MgResult};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

/// `packages.lock.json`: top-level `dependencies` object keyed by TFM,
/// each TFM holding a package map of name → {type, requested, resolved,
/// contentHash} — exactly what `dotnet restore` writes. The pin version
/// is `resolved` (the real field); a legacy `version` field is tolerated
/// as fallback.
/// packages.lock.json: object `dependencies` theo TFM, mỗi TFM giữ map
/// package name → entry — đúng shape `dotnet restore` ghi. Version ghim
/// lấy từ `resolved` (field thật); `version` cũ được dung thứ dự phòng.
#[derive(Debug, Deserialize)]
struct NuGetLock {
    #[serde(default)]
    dependencies:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, NuGetEntry>>,
}

#[derive(Debug, Deserialize)]
struct NuGetEntry {
    #[serde(default)]
    #[allow(dead_code)]
    r#type: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    resolved: Option<String>,
}

/// Read packages.lock.json into OSV pins (`NuGet` ecosystem). Entries
/// with a missing/empty version are skipped honestly. Pins are
/// deduplicated across target frameworks.
pub fn read_packages_lock(raw: &str) -> MgResult<(Vec<OsvPin>, Vec<String>)> {
    let lock: NuGetLock = serde_json::from_str(raw)
        .map_err(|e| MgError::Other(format!("invalid packages.lock.json: {e}")))?;
    let mut pins: Vec<OsvPin> = Vec::new();
    let mut skipped = Vec::new();
    let mut seen = BTreeSet::new();
    for tfm_deps in lock.dependencies.values() {
        for (name, entry) in tfm_deps {
            let version = entry
                .resolved
                .as_deref()
                .filter(|v| !v.is_empty())
                .or_else(|| entry.version.as_deref().filter(|v| !v.is_empty()));
            let Some(version) = version else {
                skipped.push(format!("{name}: no version pin in packages.lock.json"));
                continue;
            };
            if !seen.insert(format!("{name}@{version}")) {
                continue; // Same pin under another TFM — audited once.
                // Ghim trùng ở TFM khác — audit một lần.
            }
            pins.push(OsvPin {
                name: name.clone(),
                version: version.to_string(),
                ecosystem: "NuGet",
            });
        }
    }
    Ok((pins, skipped))
}

/// .NET audit: solution/project lockfiles → OSV `NuGet` queries.
/// Sources, in order: every `*.sln` project's `packages.lock.json`
/// (nested layouts included), then the root `packages.lock.json`.
/// Projects without a lock skip honestly with per-project remediation.
/// Without any solution, lockfile, or project the honest unsupported
/// state stays.
/// Audit .NET: lock từng project trong solution + root lock → OSV
/// `NuGet`. Project thiếu lock thì skip nêu tên kèm hướng dẫn.
pub async fn audit_dotnet(project_root: &Path) -> MgResult<AuditReport> {
    let (pins, skipped, recognized) = collect_dotnet_pins(project_root)?;

    if !recognized {
        let has_csproj = std::fs::read_dir(project_root)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .any(|e| e.file_name().to_string_lossy().ends_with(".csproj"))
            })
            .unwrap_or(false);
        return Ok(AuditReport::unsupported_ecosystem(if has_csproj {
            "lib/dotnet (csproj without packages.lock.json — run `dotnet restore --locked-mode` first)"
        } else {
            "lib/dotnet (no .sln/.csproj detected)"
        }));
    }

    if pins.is_empty() && skipped.is_empty() {
        return Ok(AuditReport::clean(0));
    }
    if pins.is_empty() {
        return Ok(AuditReport {
            packages_audited: 0,
            vulnerability_count: 0,
            vulnerabilities: vec![],
            scanner_status: ScannerStatus::Partial {
                scanned: 0,
                skipped: skipped.len(),
                reasons: skipped,
            },
        });
    }
    let mut report = audit_osv_pins(&pins).await?;
    if !skipped.is_empty() {
        report.scanner_status = ScannerStatus::Partial {
            scanned: pins.len(),
            skipped: skipped.len(),
            reasons: skipped,
        };
    }
    Ok(report)
}

/// Gather every auditable .NET pin for a project: all root solutions'
/// (sorted) plus the root lockfile. Pure I/O + parsing — no network —
/// so multi-solution aggregation is unit-testable hermetically.
/// (Gom mọi ghim .NET: mọi solution + root lock — thuần I/O, test được.)
pub fn collect_dotnet_pins(project_root: &Path) -> MgResult<(Vec<OsvPin>, Vec<String>, bool)> {
    let mut pins: Vec<OsvPin> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut recognized = false;

    // Solution lane (P0/F4 + P1): EVERY root solution is read
    // (sorted, deterministic) — extra solutions are lanes too, never
    // silently ignored. Project paths are canonicalized against the
    // project root: absolute paths and `..` escapes are REFUSED with an
    // honest skip reason (a .sln must never pull the audit outside its
    // project).
    // (Đọc MỌI solution ở root; path chuẩn hóa, từ chối escape.)
    let mut sln_paths = find_solution_files(project_root);
    sln_paths.sort();
    for sln in &sln_paths {
        recognized = true;
        let sln_raw = std::fs::read_to_string(sln)
            .map_err(|e| MgError::Other(format!("read {}: {e}", sln.display())))?;
        let projects = read_solution_projects(&sln_raw);
        if projects.is_empty() {
            skipped.push(format!(
                "{} lists no .csproj projects — nothing auditable in this solution",
                sln.display()
            ));
            continue;
        }
        for rel in &projects {
            // Join, then canonicalize: symlinks/.. must resolve INSIDE
            // the project root, else the entry is refused (recorded).
            // Windows solutions record paths in whatever case
            // ("a\\A.csproj" vs disk "a/a.csproj") while Linux checkouts
            // are case-sensitive — resolve case-insensitively so a
            // Windows-authored .sln audits the same everywhere.
            // (Chuẩn hóa path: thoát root thì từ chối có ghi nhận.
            // Resolve không phân biệt hoa thường cho .sln từ Windows.)
            let csproj = resolve_project_path(project_root, rel);
            let inside = csproj
                .as_deref()
                .and_then(|p| p.canonicalize().ok())
                .filter(|abs| abs.starts_with(canonical_root(project_root)));
            let Some(csproj_abs) =
                inside.filter(|abs| abs.extension().and_then(|e| e.to_str()) == Some("csproj"))
            else {
                skipped.push(format!(
                    "{rel}: escapes the project root or is not a .csproj — refused"
                ));
                continue;
            };
            let lock = csproj_abs
                .parent()
                .map(|d| d.join("packages.lock.json"))
                .filter(|p| p.is_file());
            match lock {
                Some(path) => {
                    let raw = std::fs::read_to_string(&path)
                        .map_err(|e| MgError::Other(format!("read {}: {e}", path.display())))?;
                    let (mut p, mut s) = read_packages_lock(&raw)?;
                    drain_new_pins(&mut p, &mut seen, &mut pins);
                    skipped.append(&mut s);
                }
                None => skipped.push(format!(
                    "{rel}: no packages.lock.json — run `dotnet restore --locked-mode` in {}",
                    csproj_abs
                        .parent()
                        .map(|d| d.display().to_string())
                        .unwrap_or_default()
                )),
            }
        }
    }

    // Root lockfile (single-project layout, unchanged legacy path).
    // Lockfile root (layout đơn project, đường cũ giữ nguyên).
    let root_lock = project_root.join("packages.lock.json");
    if root_lock.is_file() {
        recognized = true;
        let raw = std::fs::read_to_string(&root_lock)
            .map_err(|e| MgError::Other(format!("read packages.lock.json: {e}")))?;
        let (mut p, mut s) = read_packages_lock(&raw)?;
        drain_new_pins(&mut p, &mut seen, &mut pins);
        skipped.append(&mut s);
    }
    Ok((pins, skipped, recognized))
}

/// Move pins not seen before (name@version across files) into the
/// shared set — one package audited once per solution.
/// Chuyển ghim chưa gặp (name@version mọi file) vào tập chung.
fn drain_new_pins(source: &mut Vec<OsvPin>, seen: &mut BTreeSet<String>, out: &mut Vec<OsvPin>) {
    for pin in source.drain(..) {
        if seen.insert(format!("{}@{}", pin.name, pin.version)) {
            out.push(pin);
        }
    }
}

/// Canonical project root for escape checks (falls back to the raw
/// root when it does not exist — join+canonicalize of members still
/// resolves under it).
/// (Root chuẩn cho kiểm tra escape.)
fn canonical_root(project_root: &Path) -> std::path::PathBuf {
    project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf())
}

/// Resolve an sln-relative project path, tolerating Windows case drift
/// (`A.csproj` on disk as `a.csproj`). Exact hit first (zero cost on
/// matching systems); otherwise walk components matching directory
/// entries case-insensitively. Returns None when unresolvable — callers
/// refuse with a recorded skip, never a silent drop.
/// (Resolve path project, chịu case drift của Windows.)
fn resolve_project_path(project_root: &Path, rel: &str) -> Option<std::path::PathBuf> {
    use std::path::Component;
    let direct = project_root.join(rel);
    if direct.is_file() {
        return Some(direct);
    }
    let mut current = project_root.to_path_buf();
    for comp in std::path::Path::new(rel).components() {
        let want = match comp {
            Component::Normal(part) => part.to_string_lossy().to_lowercase(),
            _ => return None,
        };
        let hit = std::fs::read_dir(&current)
            .ok()?
            .filter_map(|e| e.ok())
            .find(|e| e.file_name().to_string_lossy().to_lowercase() == want)?;
        current = hit.path();
    }
    current.is_file().then_some(current)
}

/// Every `*.sln` at the project root (unsorted — callers sort for
/// determinism).
/// Mọi `*.sln` ở root.
fn find_solution_files(project_root: &Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(project_root)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("sln"))
                .collect()
        })
        .unwrap_or_default()
}

/// Read a `.sln`'s `Project(...) = "name", "<path>.csproj", ...` lines
/// into forward-slash project paths (backslashes normalized — solutions
/// are Windows-authored more often than not). Non-csproj entries
/// (solution folders, website refs) are ignored: they carry no packages.
/// Đọc dòng Project trong .sln thành path (gạch chéo chuẩn hóa —
/// solution thường viết trên Windows). Entry không-csproj bị bỏ: không
/// mang package.
pub fn read_solution_projects(raw: &str) -> Vec<String> {
    let mut projects = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("Project(") {
            continue;
        }
        let mut parts = trimmed.splitn(2, '=');
        let _guid = parts.next();
        let Some(rest) = parts.next() else { continue };
        // "Name", "path", "guid" — the path is the second quoted string.
        // "Name", "path", "guid" — path là chuỗi quote thứ hai.
        let mut quotes = Vec::new();
        let mut current = String::new();
        let mut in_q = false;
        for ch in rest.chars() {
            if ch == '"' {
                if in_q {
                    quotes.push(std::mem::take(&mut current));
                }
                in_q = !in_q;
            } else if in_q {
                current.push(ch);
            }
        }
        if quotes.len() >= 2 {
            let path = quotes[1].replace('\\', "/");
            if path.ends_with(".csproj") {
                projects.push(path);
            }
        }
    }
    projects
}

#[cfg(test)]
mod case_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// Windows case drift: sln says `A.csproj`, disk has `a.csproj` —
    /// must resolve on case-sensitive filesystems too (Linux CI).
    #[test]
    fn resolve_project_path_tolerates_case_drift() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("a")).unwrap();
        std::fs::write(dir.path().join("a").join("a.csproj"), "<Project/>\n").unwrap();
        let hit = resolve_project_path(dir.path(), "a/A.csproj").expect("case drift resolves");
        assert!(hit.is_file());
        assert!(resolve_project_path(dir.path(), "a/missing.csproj").is_none());
        assert!(resolve_project_path(dir.path(), "../escape.csproj").is_none());
    }
}
