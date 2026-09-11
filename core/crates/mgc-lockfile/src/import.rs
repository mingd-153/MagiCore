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

use anyhow::{Result, bail};
use mgc_types::strip_jsonc;
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
pub const DENO_LOCKFILE: &str = "deno.lock";

pub const ALL: [&str; 5] = [
    NPM_LOCKFILE,
    PNPM_LOCKFILE,
    YARN_LOCKFILE,
    BUN_LOCKFILE,
    DENO_LOCKFILE,
];

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
/// Skipped records (P0-5 2026-09-11): mọi entry bị bỏ qua phải được ghi
/// đích danh kèm lý do — KHÔNG có skip âm thầm. Đổi tên 2026-09-12 (P0
/// finding #8): migration là BEST-EFFORT kèm loss report tường minh,
/// KHÔNG phải lossless — bun/deno lưu RANGES/prefixes mà mgc.lock chỉ
/// giữ pin; mọi thông tin không giữ được phải xuất hiện ở đây.
// (Import outcome — source file, converted package count, version advisories.
// Skipped records: every skipped entry must be named with a reason —
// silent skips are gone. Rename 2026-09-12 (P0 finding #8): migration is
// BEST-EFFORT with an explicit loss report, NOT lossless — bun/deno store
// RANGES/prefixes while mgc.lock keeps pins; anything not preserved must
// appear in this report.)
#[derive(Debug, Clone)]
pub struct ImportReport {
    pub source_file: String,
    pub packages: usize,
    /// Cảnh báo phiên bản format mới hơn mức đã kiểm chứng (parse theo shape).
    // (Warnings for format versions newer than tested — imported by structure.)
    pub warnings: Vec<String>,
    /// Records the importer refused or could not map — recorded
    /// EXPLICITLY so `mgc import` can surface them (P0-5: best-effort
    /// migration — every input record is either imported or listed;
    /// the report IS the loss ledger, so "lossless" is never claimed).
    /// Record importer từ chối hoặc không ánh xạ được — liệt kê
    /// TƯỜNG MINH để `mgc import` hiển thị (P0-5: migration best-effort
    /// — mọi record input hoặc được import hoặc được liệt kê; báo cáo
    /// chính là sổ ghi mất mát, không bao giờ claim "lossless").
    pub skipped: Vec<SkippedRecord>,
}

/// One record the importer refused, with the exact reason.
/// Một record bị importer từ chối, kèm lý do chính xác.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedRecord {
    pub key: String,
    pub reason: String,
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
    let mut skipped: Vec<SkippedRecord> = Vec::new();
    // Root direct-dep pins (P0 finding #6) — attached to the Lockfile
    // as the root graph, never as package self-edges.
    // Pin direct-dep gốc (P0 finding #6) — gắn vào Lockfile như graph
    // root, không bao giờ là self-edge của package.
    let mut root_deps: Vec<String> = Vec::new();
    let mut packages = match file_name {
        NPM_LOCKFILE => parse_npm(&content, &mut warnings)?,
        PNPM_LOCKFILE => parse_pnpm(&content, &mut warnings)?,
        YARN_LOCKFILE => parse_yarn(&content)?,
        BUN_LOCKFILE => parse_bun(&content, &mut skipped)?,
        DENO_LOCKFILE => parse_deno(&content, &mut skipped, &mut root_deps)?,
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
    // Root graph (P0 finding #6): the source root pin set becomes the
    // lockfile root_dependencies — the honest representation of
    // "workspace root → dependency" edges.
    // Graph root (P0 finding #6): tập pin gốc của lockfile nguồn thành
    // root_dependencies — biểu diễn trung thực cạnh "workspace root →
    // dependency".
    lockfile.root_dependencies = root_deps;

    Ok((
        lockfile,
        ImportReport {
            source_file: file_name.to_string(),
            packages: count,
            warnings,
            skipped,
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
// deno — deno.lock v5 (JSON): npm map "name@version" → integrity; jsr map
// giữ nguyên tiền tố "jsr:" để audit báo skipped trung thực (P0-3 2026-09-10).
// P0 finding #6 (2026-09-12): root pins từ workspace.dependencies là
// GRAPH ROOT — ghi vào lockfile.root_dependencies, KHÔNG self-edge vào
// package. P0 finding #8: import là BEST-EFFORT kèm loss report — mọi
// record bị từ chối liệt kê tường minh trong `skipped`; JSR pin giữ
// tiền tố "jsr:" và luôn có lý do no-npm-advisory đi kèm.
// (deno.lock v5: npm map name@version → integrity; jsr keeps the jsr:
// prefix so audit reports honestly. Root pins are the ROOT GRAPH —
// lockfile.root_dependencies, never a package self-edge. Best-effort
// migration: every refused record is explicitly listed in `skipped`.)
// ---------------------------------------------------------------------------

fn parse_deno(
    content: &str,
    skipped: &mut Vec<SkippedRecord>,
    root_deps: &mut Vec<String>,
) -> Result<Vec<Package>> {
    let json: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| anyhow::anyhow!("deno.lock is not valid JSON: {e}"))?;

    // Shape-first: bắt buộc có map "npm" (deno.lock v5); "jsr" tùy chọn.
    let npm = json
        .get("npm")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("deno.lock has no 'npm' map (v5 schema expected)"))?;

    // Root dependency pins (v5 "workspace"."dependencies"): raw pin list
    // used to rebuild direct-dependency edges for the root set.
    // Ghim dependency gốc (v5 "workspace"."dependencies"): danh sách pin
    // thô dùng để dựng lại cạnh dependency-trực-tiếp cho tập root.
    let root_pins: Vec<String> = json
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let mut out = Vec::new();
    for (pin, entry) in npm {
        let Some((name, version)) = split_name_version(pin) else {
            // P0-5: malformed pins are RECORDED, never silently skipped.
            // P0-5: pin malformed được GHI LẠI, không bao giờ bỏ âm thầm.
            skipped.push(SkippedRecord {
                key: pin.clone(),
                reason: "npm pin is not name@version".to_string(),
            });
            continue;
        };
        // deno.lock v5 npm values come in TWO shapes: an object with
        // integrity ({"integrity": "sha512-..."}) or a bare version
        // string ("4.17.20") — both are valid real-world writer
        // output; accept both (string = no integrity recorded).
        // Giá trị npm của deno.lock v5 có HAI dạng: object chứa
        // integrity hoặc chuỗi version trơn — cả hai đều là output
        // writer thật; chấp nhận cả hai (string = không ghi integrity).
        let integrity = match entry {
            serde_json::Value::Object(obj) => obj
                .get("integrity")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            serde_json::Value::String(s) => {
                // Bare version string — cross-check against the pin's
                // version half; mismatch is a corrupt record.
                // Chuỗi version trơn — đối chiếu với nửa version của
                // pin; lệch nhau là record hỏng.
                if s != &version {
                    skipped.push(SkippedRecord {
                        key: pin.clone(),
                        reason: format!("npm version mismatch: pin says {version}, entry says {s}"),
                    });
                    continue;
                }
                String::new()
            }
            other => {
                skipped.push(SkippedRecord {
                    key: pin.clone(),
                    reason: format!("npm entry is neither object nor version string: {other}"),
                });
                continue;
            }
        };

        // Root direct dependencies: pins listed under
        // workspace.dependencies are the ROOT GRAPH (root → dep), NOT a
        // self-edge of the resolved package. They land in
        // lockfile.root_dependencies via the `root_deps` out-parameter
        // (P0 finding #6, 2026-09-12): the previous code pushed them
        // into the package's own `dependencies`, turning "workspace
        // root depends on lodash" into the lie "lodash depends on
        // npm:lodash@4.17.20". v5 pins carry runtime prefixes
        // ("npm:", "jsr:") — the pin is kept VERBATIM (prefix intact)
        // so no source information is lost.
        // Direct-dep gốc: pin trong workspace.dependencies là GRAPH
        // ROOT (root → dep), KHÔNG phải self-edge của package được
        // resolve. Chúng ghi vào lockfile.root_dependencies qua tham
        // số ra `root_deps` (P0 finding #6): code cũ đẩy vào
        // `dependencies` của chính package, biến "workspace root phụ
        // thuộc lodash" thành câu sai "lodash phụ thuộc
        // npm:lodash@4.17.20". Pin v5 mang tiền tố runtime — giữ
        // NGUYÊN pin, không mất thông tin nguồn.
        root_deps.extend(root_pins.iter().cloned());

        out.push(Package {
            name,
            version,
            resolved: String::new(),
            integrity,
            dependencies: vec![],
        });
    }

    // JSR deps: không có advisory DB npm — import kèm tiền tố "jsr:" để
    // downstream (audit) báo skipped trung thực; P0-5: mỗi pin JSR đều
    // được liệt kê trong skipped kèm lý do (dữ liệu gốc không có npm
    // integrity/registry → không thể lossless-map sang npm semantics).
    // (JSR deps carry no npm advisory DB — the `jsr:` prefix survives so
    // downstream audit reports honestly; P0-5: every JSR pin is listed
    // in `skipped` with its reason — the source carries no npm
    // integrity/registry data, so a lossless npm mapping is impossible.)
    if let Some(jsr) = json.get("jsr").and_then(|v| v.as_object()) {
        for pin in jsr.keys() {
            let Some((name, version)) = split_name_version(pin) else {
                skipped.push(SkippedRecord {
                    key: format!("jsr:{pin}"),
                    reason: "jsr pin is not name@version".to_string(),
                });
                continue;
            };
            // Record the JSR skip BEFORE the values move into Package.
            // Ghi skip JSR TRƯỚC khi giá trị chuyển vào Package.
            skipped.push(SkippedRecord {
                key: format!("jsr:{name}@{version}"),
                reason: "jsr package has no npm registry/integrity mapping — audit reports it as skipped".to_string(),
            });
            out.push(Package {
                name: format!("jsr:{name}"),
                version,
                resolved: String::new(),
                integrity: String::new(),
                dependencies: vec![],
            });
        }
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// bun — bun.lock JSON (text; .lockb binary ngoài phạm vi)
// P0 finding #8 (2026-09-12): BEST-EFFORT, not lossless — bun stores
// RANGES in the dep map while mgc.lock pins; the edge keeps the dep NAME
// and the range is recorded as a loss (the source file remains the
// range source of truth). Every refused entry is EXPLICITLY appended to
// `skipped` — silent `continue` is gone. Entry shapes:
// ["name@version", "url?", {deps}, "integrity?"].
// (bun.lock text JSON — .lockb binary out of scope. Best-effort: bun
// keeps RANGES, mgc.lock pins — dep NAME survives as the edge, the
// range is a recorded loss; every refused entry lands in `skipped`.)
// ---------------------------------------------------------------------------

fn parse_bun(content: &str, skipped: &mut Vec<SkippedRecord>) -> Result<Vec<Package>> {
    // P0-3 (2026-09-10): bun's writer emits JSONC (line comments +
    // trailing commas) — the same shape mgc-audit's lockfile reader
    // already handles. Import must accept what real `bun install` writes,
    // else `mgc import bun` fails on the very fixtures the migration
    // targets. Strip JSONC BEFORE the strict JSON parse — the shared
    // mgc-types::jsonc implementation (P1 dedup, 2026-09-11: one
    // implementation, two consumers, same contract).
    // Writer của bun ghi JSONC (comment dòng + dấu phẩy cuối) — import
    // phải chấp nhận đúng cái bun install ghi ra, nếu không `mgc import
    // bun` fail trên chính fixture migration nhắm tới. Cắt JSONC trước
    // khi parse JSON strict — dùng bản strip_jsonc chung trong
    // mgc-types::jsonc (dedup P1: một bản implement, hai nơi dùng).
    let cleaned = strip_jsonc(content);
    let json: serde_json::Value = serde_json::from_str(&cleaned)
        .map_err(|e| anyhow::anyhow!("bun.lock is not valid JSONC: {e}"))?;

    if json.get("__metadata").is_some() {
        bail!("bun lockfileVersion 2+ is not supported yet — refusing to guess");
    }

    let entries = json
        .get("packages")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("bun.lock has no 'packages' map"))?;

    let mut out = Vec::new();
    for (key, entry) in entries {
        // Entry chuẩn: ["name@version", "tarball-url"?, {deps}?, "sha512-..."?]
        let Some(items) = entry.as_array() else {
            skipped.push(SkippedRecord {
                key: key.clone(),
                reason: "entry is not an array".to_string(),
            });
            continue;
        };
        let Some(spec) = items.first().and_then(|v| v.as_str()) else {
            skipped.push(SkippedRecord {
                key: key.clone(),
                reason: "first entry element is not a name@version string".to_string(),
            });
            continue;
        };
        let Some((name, version)) = split_name_version(spec) else {
            // Workspace/registry ids that are not npm pins: RECORDED
            // (P0-5) — e.g. "workspace:root" survives in the report.
            // Id workspace/registry không phải pin npm: ĐƯỢC GHI
            // (P0-5) — vd "workspace:root" sống sót trong report.
            skipped.push(SkippedRecord {
                key: key.clone(),
                reason: format!("entry spec '{spec}' is not a name@version pin"),
            });
            continue;
        };
        let mut resolved = String::new();
        let mut integrity = String::new();
        let mut dependencies: Vec<String> = Vec::new();
        for item in items.iter().skip(1) {
            if let Some(text) = item.as_str() {
                if text.starts_with("http://") || text.starts_with("https://") {
                    resolved = text.to_string();
                } else if text.contains("sha512-") || text.contains("sha256-") {
                    integrity = text.to_string();
                }
            } else if let Some(deps) = item.as_object() {
                // Dependency map element: {"left-pad": "left-pad@^4.0.0"}.
                // bun stores RANGES; mgc.lock pins — keep the dep NAME
                // as the edge; the RANGE is a recorded loss (P0 finding
                // #8: best-effort, the source lockfile keeps ranges).
                // Phần tử map dependency: bun lưu RANGE; mgc.lock lưu
                // pin — giữ TÊN dep làm cạnh; RANGE là mất mát được ghi
                // nhận (best-effort, lockfile nguồn giữ range).
                for dep_name in deps.keys() {
                    dependencies.push(dep_name.clone());
                }
            }
        }
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

// The bun JSONC strip (quote-aware comments + trailing commas) was
// consolidated into mgc-types::jsonc::strip_jsonc (P1 dedup 2026-09-11)
// — this file now imports the single shared implementation above.
// Phần strip JSONC của bun (comment nhận-biết-quote + dấu phẩy cuối)
// đã gộp vào mgc-types::jsonc::strip_jsonc (dedup P1) — file này giờ
// dùng bản chung duy nhất ở trên.
