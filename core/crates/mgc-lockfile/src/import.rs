//! Import legacy PM lockfiles (npm / pnpm / yarn / bun) → `mgc.lock` schema v2.
//! Chuyển đổi lockfile của PM khác sang mgc.lock — parser dữ liệu thuần,
//! KHÔNG gọi/wrap/bao PM nào, KHÔNG copy code PM (00-index §2: không delegate PM wrapper).
//!
//! Chính sách phiên bản: SHAPE-FIRST — chấp nhận theo cấu trúc `packages` map,
//! số version chỉ để cảnh báo khi chưa test (PM bump version liên tục;
//! RULE §12: không pin giá trị hay thay đổi vào code).
// (Import other package managers' lockfiles into mgc.lock v2 — pure data parsers,
// never executes/wraps any PM. Version policy: SHAPE-FIRST — acceptance by data
// structure; version numbers only drive advisories for untested formats.)

use anyhow::{bail, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::schema::Package;

/// Legacy lockfile descriptor — mô tả 1 lockfile PM tìm thấy trong project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyLockfile {
    pub file_name: &'static str,
    pub path: PathBuf,
}

/// Supported legacy lockfile names — tên file được nhận dạng.
pub const NPM_LOCKFILE: &str = "package-lock.json";
pub const PNPM_LOCKFILE: &str = "pnpm-lock.yaml";
pub const YARN_LOCKFILE: &str = "yarn.lock";
pub const BUN_LOCKFILE: &str = "bun.lock";

pub const ALL: [&str; 4] = [NPM_LOCKFILE, PNPM_LOCKFILE, YARN_LOCKFILE, BUN_LOCKFILE];

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

/// Phát hiện nguy cơ Trust-Downgrade: đã có `mgc.lock` nhưng lại xuất hiện
/// lockfile PM cũ có nguy cơ ghi đè/nhầm lẫn.
// (Trust-downgrade risk: a signed mgc.lock exists alongside stale legacy lockfiles.)
pub fn check_trust_downgrade_risk(project_root: &Path) -> Option<Vec<&'static str>> {
    let mgc_lock = project_root.join("mgc.lock");
    if !mgc_lock.exists() {
        return None;
    }
    let legacy = detect_legacy_lockfiles(project_root);
    if legacy.is_empty() {
        None
    } else {
        Some(legacy.iter().map(|l| l.file_name).collect())
    }
}

/// Kết quả import — báo cáo nguồn + số package đã chuyển đổi + cảnh báo phiên bản.
// (Import outcome — source file, converted package count, version advisories.)
#[derive(Debug, Clone)]
pub struct ImportReport {
    pub source_file: String,
    pub packages: usize,
    /// Cảnh báo phiên bản format mới hơn mức đã kiểm chứng (parse theo shape).
    // (Warnings for format versions newer than tested — imported by structure.)
    pub warnings: Vec<String>,
}

/// Phiên bản format ĐÃ kiểm chứng — chỉ dùng để quyết định cảnh báo, KHÔNG dùng
/// để chặn: PM thay đổi version thường xuyên, parser chấp nhận theo cấu trúc dữ liệu
/// và cảnh báo khi gặp version chưa test (RULE §12 — không pin giá trị hay đổi).
// (Tested format versions — advisory only, never a gate. PMs bump versions often;
// parsers accept by data shape and warn on untested versions per RULE §12.)
const NPM_TESTED_VERSIONS: &[i64] = &[2, 3];
const PNPM_TESTED_VERSIONS: &[&str] = &["9.0"];

/// Detect theo độ ưu tiên npm > pnpm > yarn > bun rồi parse sang Lockfile v2.
/// Nhiều file cùng tồn tại → dùng file ưu tiên cao nhất (spec lockfile-import-plan §2).
// (Detect by priority npm > pnpm > yarn > bun and parse the first match.)
pub fn import_into_lockfile(project_root: &Path) -> Result<(crate::Lockfile, ImportReport)> {
    let candidates = detect_legacy_lockfiles(project_root);
    let first = candidates
        .first()
        .ok_or_else(|| anyhow::anyhow!("no supported legacy lockfile found (npm/pnpm/yarn/bun)"))?;
    import_file(&first.path)
}

/// Parse 1 file lockfile cụ thể → Lockfile v2 (chưa ký).
// (Parse one specific lockfile file into an unsigned v2 Lockfile.)
pub fn import_file(path: &Path) -> Result<(crate::Lockfile, ImportReport)> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    let mut warnings: Vec<String> = Vec::new();
    let mut packages = match file_name {
        NPM_LOCKFILE => parse_npm(&content, &mut warnings)?,
        PNPM_LOCKFILE => parse_pnpm(&content, &mut warnings)?,
        YARN_LOCKFILE => parse_yarn(&content)?,
        BUN_LOCKFILE => parse_bun(&content)?,
        other => bail!("unsupported lockfile '{other}'"),
    };

    if packages.is_empty() {
        bail!("no importable packages found in {file_name}");
    }

    // Sắp xếp + khử trùng lặp (name,version) → output deterministic
    // (Sort + dedupe by (name, version) for deterministic output)
    packages.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    packages.dedup_by(|a, b| a.name == b.name && a.version == b.version);

    let count = packages.len();
    let mut lockfile = crate::Lockfile::new();
    lockfile.packages = packages;

    Ok((
        lockfile,
        ImportReport {
            source_file: file_name.to_string(),
            packages: count,
            warnings,
        },
    ))
}

/// Helper: tách "name@version" tại `@` cuối cùng (hỗ trợ scoped @scope/pkg@1.0.0).
// (Split "name@version" at the last '@', scoped-package safe.)
fn split_name_version(spec: &str) -> Option<(String, String)> {
    let trimmed = spec.trim();
    let at = trimmed.rfind('@')?;
    if at == 0 {
        return None; // "@scope" chưa có phần version
    }
    let name = trimmed[..at].trim_start_matches('/').to_string();
    let version = trimmed[at + 1..].to_string();
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some((name, version))
}

/// Cắt hậu tố pnpm sau version: "/name@1.2.3(peer_x@1.0.0)" → "1.2.3".
// (Trim pnpm's parenthesised peer suffix after the version.)
fn strip_paren_suffix(spec: &str) -> &str {
    match spec.find('(') {
        Some(i) => &spec[..i],
        None => spec,
    }
}

// ---------------------------------------------------------------------------
// npm — package-lock.json lockfileVersion 2|3 (packages map)
// ---------------------------------------------------------------------------

fn parse_npm(content: &str, warnings: &mut Vec<String>) -> Result<Vec<Package>> {
    let json: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| anyhow::anyhow!("package-lock.json is not valid JSON: {e}"))?;

    // Cổng theo CẤU TRÚC (packages map) chứ không theo số version — npm bump
    // version thường xuyên; shape giữ nguyên thì dữ liệu vẫn import đúng được.
    // (Gate on STRUCTURE, not the version number — PMs bump versions often;
    // unchanged shape means the data still imports correctly.)
    let entries = json
        .get("packages")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("package-lock.json has no 'packages' map"))?;

    match json.get("lockfileVersion").and_then(|v| v.as_i64()) {
        Some(v) if NPM_TESTED_VERSIONS.contains(&v) => {}
        Some(other) => warnings.push(format!(
            "package-lock.json lockfileVersion {other} is newer than tested ({}) — importing by structure, review the result",
            NPM_TESTED_VERSIONS.iter().map(|v| v.to_string()).collect::<Vec<_>>().join("/")
        )),
        None => warnings.push(
            "package-lock.json has no lockfileVersion field — importing by structure".to_string(),
        ),
    }

    // Lookup key chuẩn để dựng dependency edges: "node_modules/<name>" → name@version
    let lookup: BTreeMap<String, (String, String)> = entries
        .iter()
        .filter_map(|(key, entry)| {
            let ver = entry.get("version")?.as_str()?;
            let segment = key.rsplit("node_modules/").next()?;
            let name = segment
                .strip_prefix("node_modules/")
                .filter(|n| !n.is_empty())
                .unwrap_or(segment);
            (!name.is_empty()).then_some((key.clone(), (name.to_string(), ver.to_string())))
        })
        .collect();

    let mut out = Vec::new();
    for (key, entry) in entries {
        // Bỏ root "" và entry không có version (link/workspace)
        // (Skip the root entry and link/workspace entries without a version)
        let Some(version) = entry.get("version").and_then(|v| v.as_str()) else {
            continue;
        };
        // Tên package = đoạn sau "node_modules/" cuối cùng của key
        let Some(name) = key.rsplit("node_modules/").next().and_then(|seg| {
            seg.strip_prefix("node_modules/")
                .or_else(|| (!seg.is_empty()).then_some(seg))
        }) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }

        let resolved = entry
            .get("resolved")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let integrity = entry
            .get("integrity")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        // Dependency edges: chỉ khi lookup thấy đúng "node_modules/<dep>" (exact version)
        let dependencies: Vec<String> = entry
            .get("dependencies")
            .and_then(|v| v.as_object())
            .map(|deps| {
                deps.keys()
                    .filter_map(|dep| {
                        lookup
                            .get(&format!("node_modules/{dep}"))
                            .map(|(dn, dv)| format!("{dn}@{dv}"))
                    })
                    .collect()
            })
            .unwrap_or_default();

        out.push(Package {
            name: name.to_string(),
            version: version.to_string(),
            resolved,
            integrity,
            dependencies,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// pnpm — pnpm-lock.yaml lockfileVersion '9.0'
// ---------------------------------------------------------------------------

fn parse_pnpm(content: &str, warnings: &mut Vec<String>) -> Result<Vec<Package>> {
    let yaml: serde_yaml::Value = serde_yaml::from_str(content)
        .map_err(|e| anyhow::anyhow!("pnpm-lock.yaml is not valid YAML: {e}"))?;

    // Shape-first như npm — pnpm đổi '9.0' → '10.0'... không cần sửa code.
    // (Shape-first like npm — pnpm bumping '9.0' → '10.0' needs no code change.)
    let entries = yaml
        .get("packages")
        .and_then(|v| v.as_mapping())
        .ok_or_else(|| anyhow::anyhow!("pnpm-lock.yaml has no 'packages' map"))?;

    match yaml.get("lockfileVersion").and_then(|v| v.as_str()) {
        Some(v) if PNPM_TESTED_VERSIONS.contains(&v) => {}
        Some(other) => warnings.push(format!(
            "pnpm-lock.yaml lockfileVersion {other:?} is newer than tested ({}) — importing by structure, review the result",
            PNPM_TESTED_VERSIONS.iter().map(|v| (*v).to_string()).collect::<Vec<_>>().join("/")
        )),
        None => warnings.push(
            "pnpm-lock.yaml has no lockfileVersion field — importing by structure".to_string(),
        ),
    }

    // Tập (name,version) đã khoá để dựng edges từ dependencies exact
    let known: std::collections::HashSet<(String, String)> = entries
        .keys()
        .filter_map(|k| k.as_str())
        // key có hậu tố peer "/name@1.2.3(peer@x)" — cắt trước khi split
        .map(|raw| strip_paren_suffix(raw.trim_start_matches('/')).to_string())
        .filter_map(|cleaned| split_name_version(&cleaned))
        .collect();

    let mut out = Vec::new();
    for (key, entry) in entries {
        let Some(raw) = key.as_str() else { continue };
        // Key dạng "/name@1.2.3(peer@x)" hoặc "name@1.2.3"
        let Some((name, version)) = split_name_version(strip_paren_suffix(raw)) else {
            continue;
        };
        let integrity = entry
            .get("resolution")
            .and_then(|r| r.get("integrity"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let resolved = entry
            .get("resolution")
            .and_then(|r| r.get("tarball"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        // Edge chỉ lấy khi dependency trỏ đúng version đã có trong tập.
        // deps map: {tên_dep: "1.2.3" | "/tên@1.2.3(...)"} — tên lấy từ key của map.
        let dependencies: Vec<String> = entry
            .get("dependencies")
            .and_then(|v| v.as_mapping())
            .map(|deps| {
                deps.iter()
                    .filter_map(|(dep_key, spec)| {
                        let dep_name = dep_key.as_str()?;
                        let spec = spec.as_str()?;
                        let cleaned = strip_paren_suffix(spec.trim_start_matches('/'));
                        // Spec có thể "name@ver" hoặc version thuần "1.2.3"
                        let candidate = match split_name_version(cleaned) {
                            Some(pair) => Some(pair),
                            None => {
                                let looks_exact = !cleaned.is_empty()
                                    && cleaned.chars().next().is_some_and(|c| c.is_ascii_digit());
                                looks_exact.then(|| (dep_name.to_string(), cleaned.to_string()))
                            }
                        }?;
                        known
                            .contains(&candidate)
                            .then(|| format!("{}@{}", candidate.0, candidate.1))
                    })
                    .collect()
            })
            .unwrap_or_default();

        out.push(Package {
            name,
            version,
            resolved,
            integrity,
            dependencies,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// yarn — yarn.lock text (classic v1 syntax; berry bị reject fail-closed)
// ---------------------------------------------------------------------------

fn parse_yarn(content: &str) -> Result<Vec<Package>> {
    if content.contains("__metadata:") {
        bail!(
            "yarn berry lockfiles are not supported (detected '__metadata:') — refusing to guess"
        );
    }

    let mut out: Vec<Package> = Vec::new();
    let mut current_name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut resolved = String::new();
    let mut integrity = String::new();

    let flush = |name: &mut Option<String>,
                 version: &mut Option<String>,
                 resolved: &mut String,
                 integrity: &mut String,
                 out: &mut Vec<Package>| {
        if let (Some(name), Some(version)) = (name.take(), version.take()) {
            out.push(Package {
                name,
                version,
                resolved: std::mem::take(resolved),
                integrity: std::mem::take(integrity),
                dependencies: vec![],
            });
        } else {
            *name = None;
            *version = None;
        }
    };

    for line in content.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') {
            // Header block mới: `name@range:` (có thể quoted, nhiều spec cách nhau dấu phẩy)
            flush(
                &mut current_name,
                &mut version,
                &mut resolved,
                &mut integrity,
                &mut out,
            );
            let header = line.trim_end_matches(':').trim().trim_matches('"');
            let first_spec = header.split(',').next().unwrap_or_default().trim();
            current_name = split_name_version(first_spec).map(|(n, _)| n);
        } else if let Some((field, value)) = line.trim().split_once(' ') {
            let value = value.trim().trim_matches('"');
            // Cú pháp yarn.lock: `version "x"` — field KHÔNG có dấu ':'
            // (yarn.lock syntax: bare field name, no trailing colon)
            match field.trim_end_matches(':') {
                "version" => version = Some(value.to_string()),
                "resolved" => resolved = value.to_string(),
                "integrity" => integrity = value.to_string(),
                _ => {}
            }
        }
    }
    flush(
        &mut current_name,
        &mut version,
        &mut resolved,
        &mut integrity,
        &mut out,
    );

    Ok(out)
}

// ---------------------------------------------------------------------------
// bun — bun.lock JSON (text; .lockb binary ngoài phạm vi)
// ---------------------------------------------------------------------------

fn parse_bun(content: &str) -> Result<Vec<Package>> {
    let json: serde_json::Value = serde_json::from_str(content).map_err(|e| {
        anyhow::anyhow!(
            "bun.lock is not valid JSON (bun comments/trailing commas unsupported): {e}"
        )
    })?;

    if json.get("__metadata").is_some() {
        bail!("bun lockfileVersion 2+ is not supported yet — refusing to guess");
    }

    let entries = json
        .get("packages")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("bun.lock has no 'packages' map"))?;

    let mut out = Vec::new();
    for (_key, entry) in entries {
        // Entry chuẩn: ["name@version", "tarball-url"?, "sha512-..."?]
        let Some(items) = entry.as_array() else {
            continue;
        };
        let Some(spec) = items.first().and_then(|v| v.as_str()) else {
            continue;
        };
        let Some((name, version)) = split_name_version(spec) else {
            continue;
        };
        let mut resolved = String::new();
        let mut integrity = String::new();
        for item in items.iter().skip(1) {
            let Some(text) = item.as_str() else { continue };
            if text.starts_with("http://") || text.starts_with("https://") {
                resolved = text.to_string();
            } else if text.contains("sha512-") || text.contains("sha256-") {
                integrity = text.to_string();
            }
        }
        out.push(Package {
            name,
            version,
            resolved,
            integrity,
            dependencies: vec![],
        });
    }
    Ok(out)
}
