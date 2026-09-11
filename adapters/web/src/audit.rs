//! `audit.rs` — Security audits, supply-chain checks, and advisory query execution for WebAdapter.

use mgc_types::adapter::{AuditReport, Vulnerability, VulnerabilitySeverity};
use mgc_types::{DependencySpec, MgError, MgResult, PackageId, PackageName, Version, VersionRange};
use std::path::Path;

use crate::lockfile::{read_web_lockfile_checked, write_web_lockfile_with_state};
use crate::manifest::{parse_manifest, write_manifest};

pub fn allow_insecure_loopback_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    parsed.scheme() == "http"
        && matches!(
            parsed.host_str(),
            Some("127.0.0.1") | Some("localhost") | Some("::1")
        )
}

pub fn is_tarball_url_trusted(tarball_url: &str, registry_url: &str) -> bool {
    let Ok(tarball_parsed) = url::Url::parse(tarball_url) else {
        return false;
    };

    let Ok(registry_parsed) = url::Url::parse(registry_url) else {
        return false;
    };

    let Some(tarball_host) = tarball_parsed.host_str() else {
        return false;
    };

    let Some(registry_host) = registry_parsed.host_str() else {
        return false;
    };

    if tarball_host == "127.0.0.1" || tarball_host == "localhost" || tarball_host == "::1" {
        return true;
    }

    if tarball_host == registry_host {
        return true;
    }

    if registry_host == "registry.npmjs.org" {
        return tarball_host == "registry.npmjs.org"
            || tarball_host.ends_with(".npmjs.org")
            || tarball_host == "registry.yarnpkg.com";
    }

    if let Ok(allowed) = std::env::var("MAGICORE_WEB_ALLOWED_TARBALL_HOSTS") {
        let allowed_hosts: Vec<&str> = allowed
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if allowed_hosts.contains(&tarball_host) {
            return true;
        }
    }

    false
}

pub fn registry_advisory_bulk_endpoint(registry_url: &str) -> MgResult<url::Url> {
    let base = url::Url::parse(registry_url).map_err(|err| {
        MgError::Other(format!(
            "invalid web registry URL '{}': {}",
            registry_url, err
        ))
    })?;

    base.join("-/npm/v1/security/advisories/bulk")
        .map_err(|err| {
            MgError::Other(format!(
                "invalid advisory endpoint for registry '{}': {}",
                registry_url, err
            ))
        })
}

pub async fn run_audit(project_root: &Path, registry_url: &str) -> MgResult<AuditReport> {
    // ARCHITECTURE CONTRACT (2026-09-10 P0-3 audit): mgc.lock là NGUỒN
    // CHÂN LÝ DUY NHẤT cho audit web. bun.lock / deno.lock là INPUT
    // MIGRATION, không phải nguồn audit vận hành — không fallback âm
    // thầm. Phát hiện rival lockfile mà thiếu mgc.lock → fail-closed kèm
    // remediation `mgc import`, không tự audit lockfile đối thủ.
    // (mgc.lock is the ONLY operational audit source. Rival lockfiles
    // are migration inputs: detected without mgc.lock → fail with the
    // explicit `mgc import` remediation, never a silent fallback.)
    let lockfile = read_web_lockfile_checked(project_root)?;
    if lockfile.is_none() {
        let rival = detect_rival_lockfiles(project_root);
        if !rival.is_empty() {
            return Err(MgError::Other(format!(
                "rival lockfile(s) detected [{}] but mgc.lock is missing — mgc audit consumes mgc.lock ONLY. Run `mgc import {}` to migrate this project first.",
                rival.join(", "),
                if rival.iter().any(|r| r.contains("deno")) {
                    "deno"
                } else {
                    "bun"
                }
            )));
        }
        return Ok(AuditReport::clean(0));
    }

    // Tách 2 tập pin: npm-auditable và JSR (tiền tố "jsr:" import từ
    // deno.lock migration). JSR không có advisory DB npm — gửi tên "jsr:"
    // vào npm bulk API sẽ 400/fake-failure; chúng chỉ được báo Partial
    // skipped trung thực, KHÔNG vào body request.
    // (Split pins: npm-auditable vs JSR-prefixed. JSR names never enter
    // the npm bulk body — they ride the honest Partial-skip lane.)
    let mut pins: Vec<(String, String)> = lockfile
        .as_ref()
        .map(|l| {
            l.packages
                .iter()
                .filter(|p| !p.name.as_str().starts_with("jsr:"))
                .map(|p| (p.name.to_string(), p.version.clone()))
                .collect()
        })
        .unwrap_or_default();
    let jsr_pins: Vec<String> = lockfile
        .as_ref()
        .map(|l| {
            l.packages
                .iter()
                .filter(|p| p.name.as_str().starts_with("jsr:"))
                .map(|p| p.name.as_str().to_string())
                .collect()
        })
        .unwrap_or_default();

    pins.sort();
    pins.dedup();

    if pins.is_empty() && jsr_pins.is_empty() {
        return Ok(AuditReport::clean(0));
    }
    if pins.is_empty() {
        // JSR-only graph (migrated from deno.lock): honest Partial —
        // nothing silently clean when the graph exists but the advisory
        // lane cannot see it.
        // Đồ thị chỉ JSR (migrate từ deno.lock): Partial trung thực.
        return Ok(AuditReport {
            packages_audited: 0,
            vulnerability_count: 0,
            vulnerabilities: vec![],
            scanner_status: mgc_types::adapter::ScannerStatus::Partial {
                scanned: 0,
                skipped: jsr_pins.len(),
                reasons: jsr_pins
                    .iter()
                    .map(|j| format!("jsr package '{j}' has no npm advisory mapping yet"))
                    .collect(),
            },
        });
    }

    let mut body = serde_json::Map::new();
    for (name, version) in &pins {
        let version_entry = serde_json::json!([version.clone()]);
        body.insert(name.clone(), version_entry);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(format!("magicore/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| MgError::Network(format!("audit client error: {e}")))?;

    let advisory_endpoint = registry_advisory_bulk_endpoint(registry_url)?;
    let response = match client.post(advisory_endpoint).json(&body).send().await {
        Ok(response) => response,
        Err(e) => {
            // SCANNER FAILURE, not a hard error (same contract fix as the
            // OSV lane 2026-09-10): a dead/unreachable registry is an
            // ENVIRONMENT state — the finisher turns Failed into the
            // honest UNVERIFIED lane (exit 2 strict), never exit 1
            // (findings) and never a fake clean.
            // SCANNER FAILED, không phải hard error (cùng hợp đồng fix
            // như lane OSV): registry chết là trạng thái MÔI TRƯỜNG —
            // finisher chuyển Failed thành lane UNVERIFIED trung thực
            // (strict exit 2), không bao giờ exit 1 (findings) hay sạch giả.
            return Ok(AuditReport::scanner_failed(
                "npm-bulk-advisory",
                format!("audit request failed: {e}"),
            ));
        }
    };

    if !response.status().is_success() {
        return Ok(AuditReport::scanner_failed(
            "npm-bulk-advisory",
            format!("audit API returned {}", response.status()),
        ));
    }

    let advisories: serde_json::Value = response
        .json()
        .await
        .map_err(|e| MgError::Other(format!("audit response parse error: {e}")))?;

    let vulnerabilities = parse_advisory_bulk_response(&advisories, &pins)?;
    let vuln_count = vulnerabilities.len();

    // JSR pins imported INTO mgc.lock from a deno.lock migration cannot
    // ride the npm advisory pipeline — surface them as Partial reasons
    // (loud, never silently dropped). Detection here reads mgc.lock only
    // (P0-3: no operational rival-lockfile reads).
    // Ghim JSR import VÀO mgc.lock từ migration deno.lock không đi được
    // đường advisory npm — đưa ra thành lý do Partial (rõ ràng, không bỏ
    // âm thầm). Detection chỉ đọc mgc.lock (P0-3: không đọc lockfile đối
    // thủ trên đường vận hành).
    // JSR pins (migrated from deno.lock) cannot ride the npm advisory
    // pipeline — surface them as Partial reasons (loud, never silently
    // dropped). They were already excluded from the request body above.
    // Ghim JSR (migrate từ deno.lock) không đi được đường advisory npm —
    // đưa ra thành lý do Partial (rõ ràng, không bỏ âm thầm); đã loại
    // khỏi body request phía trên.
    let scanner_status = if jsr_pins.is_empty() {
        mgc_types::adapter::ScannerStatus::Available
    } else {
        mgc_types::adapter::ScannerStatus::Partial {
            scanned: pins.len(),
            skipped: jsr_pins.len(),
            reasons: jsr_pins
                .iter()
                .map(|j| format!("jsr package '{j}' has no npm advisory mapping yet"))
                .collect(),
        }
    };

    Ok(AuditReport {
        packages_audited: pins.len(),
        vulnerability_count: vuln_count,
        vulnerabilities,
        // Real network scanner executed — typed parse is fail-closed, so
        // reaching this line means the payload matched the contract.
        // Scanner mạng thật đã chạy — parse typed fail-closed, tới đây
        // nghĩa là payload khớp hợp đồng.
        scanner_status,
    })
}

/// Typed advisory record — the REAL npm Bulk Advisory API schema
/// (verified against the npm bulk endpoint contract; the same shape
/// pnpm audit consumes). The registry does NOT return `findings` —
/// clients build findings by matching `vulnerable_versions` against the
/// local dependency tree (Tech Lead P0-1 2026-09-09).
/// Bản ghi advisory typed — schema THẬT của npm Bulk Advisory API (cùng
/// shape mà pnpm audit dùng). Registry KHÔNG trả `findings` — client tự
/// đối chiếu `vulnerable_versions` với dependency tree local.
///
/// Required: id, url, title, severity, vulnerable_versions. A response
/// missing any of these is malformed and must fail closed.
/// Bắt buộc: id, url, title, severity, vulnerable_versions. Response thiếu
/// field nào là malformed và phải fail-closed.
#[derive(serde::Deserialize)]
struct AdvisoryRecord {
    /// Numeric advisory id from the registry.
    /// Id advisory số từ registry.
    id: serde_json::Number,
    url: String,
    title: String,
    severity: String,
    /// Semver range of affected versions, e.g. "<4.17.21".
    /// Range semver của các version dính, vd "<4.17.21".
    vulnerable_versions: String,
}

/// Parse the bulk advisory response with a fail-closed contract: the
/// payload MUST be an object mapping package names to advisory arrays;
/// every advisory MUST carry the npm Bulk API required fields. Anything
/// else (array, string, null, wrong shapes) is an error — never an
/// implicit zero-finding "clean" report.
/// Parse response bulk advisory theo hợp đồng fail-closed: payload PHẢI
/// là object map tên package → mảng advisory; mọi advisory PHẢI đủ field
/// bắt buộc của npm Bulk API. Cái khác (array, string, null, shape sai)
/// là lỗi — không bao giờ tự biến thành report "sạch" 0 finding.
///
/// Findings are built CLIENT-SIDE: for each advisory, every lockfile
/// entry whose installed version matches `vulnerable_versions` becomes
/// one Vulnerability row (same model as pnpm audit — the registry has
/// no knowledge of our tree).
/// Finding được dựng Ở CLIENT: với mỗi advisory, mọi entry lockfile có
/// version nằm trong `vulnerable_versions` thành một dòng Vulnerability
/// (registry không biết tree của ta — pnpm cũng làm vậy).
pub(crate) fn parse_advisory_bulk_response(
    advisories: &serde_json::Value,
    pins: &[(String, String)],
) -> MgResult<Vec<Vulnerability>> {
    let Some(map) = advisories.as_object() else {
        return Err(MgError::Other(format!(
            "advisory response must be a JSON object mapping packages to advisory arrays, got: {}",
            type_of_json(advisories)
        )));
    };

    // Requested package names — the response may only speak about packages
    // we actually asked for; anything else is a contract violation.
    // Tên package đã yêu cầu — response chỉ được nói về package ta hỏi;
    // ngoài tập này là vi phạm hợp đồng.
    let requested: std::collections::HashSet<&str> = pins.iter().map(|(n, _)| n.as_str()).collect();

    let mut vulnerabilities = Vec::new();
    let mut rejected: Vec<String> = Vec::new();

    for (pkg_name, advisory_list) in map {
        if !requested.contains(pkg_name.as_str()) {
            return Err(MgError::Other(format!(
                "advisory response mentions package '{pkg_name}' which was not requested — possible malformed or hostile response"
            )));
        }
        let Some(advisories_arr) = advisory_list.as_array() else {
            return Err(MgError::Other(format!(
                "advisory response for '{pkg_name}' must be an array, got: {}",
                type_of_json(advisory_list)
            )));
        };
        for advisory_value in advisories_arr {
            match build_findings_for_advisory(pkg_name, advisory_value, pins) {
                Ok(mut found) => vulnerabilities.append(&mut found),
                Err(reason) => rejected.push(format!("{pkg_name}: {reason}")),
            }
        }
    }

    // No advisory entry may be dropped silently — a malformed entry is a
    // schema error, not a "skip gracefully" moment.
    // Không được bỏ âm thầm entry advisory nào — entry malformed là lỗi
    // schema, không phải lúc "skip gracefully".
    if !rejected.is_empty() {
        return Err(MgError::Other(format!(
            "advisory response contained {} malformed entr{}: {}",
            rejected.len(),
            if rejected.len() == 1 { "y" } else { "ies" },
            rejected.join("; ")
        )));
    }

    Ok(vulnerabilities)
}

/// Build findings for ONE advisory: parse the npm Bulk API record, then
/// match every installed lockfile version of that package against
/// `vulnerable_versions`. Zero matching versions → the advisory does not
/// apply to this tree → no finding (legitimate, unlike a parse error).
/// Dựng finding cho MỘT advisory: parse record npm Bulk API, rồi đối
/// chiếu mọi version lockfile của package đó với `vulnerable_versions`.
/// Không version nào khớp → advisory không áp dụng cho tree này → không
/// finding (hợp lệ, khác với lỗi parse).
fn build_findings_for_advisory(
    pkg_name: &str,
    advisory: &serde_json::Value,
    pins: &[(String, String)],
) -> Result<Vec<Vulnerability>, String> {
    let record: AdvisoryRecord =
        serde::Deserialize::deserialize(advisory).map_err(|e| format!("schema mismatch: {e}"))?;

    let range = VersionRange::parse(&record.vulnerable_versions).map_err(|e| {
        format!(
            "invalid vulnerable_versions '{}': {e}",
            record.vulnerable_versions
        )
    })?;
    // Strict semver validation for audit (unlike resolver ranges, an
    // advisory range we cannot interpret must NOT silently match
    // nothing — that would hide real findings behind garbage ranges).
    // Kiểm tra semver chặt cho audit (khác range resolver): range
    // advisory không diễn giải được KHÔNG được phép khớp rỗng một cách
    // âm thầm — sẽ giấu finding thật sau range rác.
    validate_semver_range(&record.vulnerable_versions)?;

    let mut findings = Vec::new();
    for (_pin_name, pin_version) in pins.iter().filter(|(n, _)| n == pkg_name) {
        let installed = Version::parse(pin_version)
            .map_err(|e| format!("invalid installed version '{pin_version}': {e}"))?;
        if !range.matches(&installed) {
            continue;
        }
        findings.push(
            Vulnerability {
                package: PackageId::new(
                    PackageName::new(pkg_name.to_string())
                        .map_err(|e| format!("invalid package name: {e}"))?,
                    installed,
                ),
                title: record.title.clone(),
                severity: record.severity.clone(),
                // The registry numeric `id` is the advisory identifier (same
                // thing npm audit prints); there is no CVE array on this API.
                // `id` số của registry chính là định danh advisory (npm audit
                // cũng in vậy); API này không có mảng CVE.
                cve: record.id.to_string(),
                severity_level: VulnerabilitySeverity::from_str(&record.severity),
                patched_versions: Some(record.vulnerable_versions.clone()),
                url: Some(record.url.clone()),
                scanner: None,
                ecosystem: None,
                evidence_at: None,
            }
            .with_evidence("npm-bulk-advisory", "web/javascript"),
        );
    }
    Ok(findings)
}

/// Human-readable JSON value kind for schema error messages.
/// Tên loại giá trị JSON cho thông báo lỗi schema.
fn type_of_json(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Audit-grade semver range validation: every `||` alternative must be
/// an operator + parseable version (`*` allowed). The general resolver
/// tolerates loose ranges; an AUDIT range that cannot be interpreted
/// must fail closed instead of matching nothing (which would hide real
/// findings behind malformed data from the registry).
/// Kiểm tra range semver cấp audit: mỗi alternative `||` phải là toán
/// tử + version parse được (cho phép `*`). Resolver chung dung thứ range
/// lỏng lẻo; range AUDIT không diễn giải được phải fail-closed thay vì
/// khớp rỗng (đề phòng giấu finding thật sau dữ liệu registry rác).
fn validate_semver_range(range: &str) -> Result<(), String> {
    for alt in range.split("||").map(str::trim) {
        if alt.is_empty() {
            return Err(format!("empty alternative in range '{range}'"));
        }
        if alt == "*" {
            continue;
        }
        // Tokens may be operator-only (">="), version-only ("4.17.21"),
        // or compact glued forms ("<4.17.21", "^1.2.3" — npm style).
        // Token có thể là riêng toán tử (">="), riêng version ("4.17.21"),
        // hoặc dạng dính gọn ("<4.17.21", "^1.2.3" — kiểu npm).
        let mut tokens = alt.split_whitespace().peekable();
        while let Some(token) = tokens.next() {
            if matches!(token, ">=" | "<=" | ">" | "<" | "=" | "==" | "^" | "~") {
                let Some(ver) = tokens.next() else {
                    return Err(format!(
                        "operator '{token}' in '{alt}' of range '{range}' has no version"
                    ));
                };
                if Version::parse(ver).is_err() {
                    return Err(format!(
                        "version '{ver}' in '{alt}' of range '{range}' is not parseable semver"
                    ));
                }
            } else {
                // Compact form: strip glued operators, then parse.
                // Dạng dính: bóc toán tử dính liền, rồi parse.
                let stripped = token.trim_start_matches(['>', '<', '=', '^', '~']);
                if Version::parse(stripped).is_err() {
                    return Err(format!(
                        "token '{token}' in '{alt}' of range '{range}' is not a parseable semver version"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Detect rival JS-runtime lockfiles (migration inputs) in the project
/// root — read-only existence check, used ONLY for the fail-closed
/// remediation error when mgc.lock is missing (P0-3 2026-09-10).
/// Phát hiện lockfile runtime đối thủ (input migration) — chỉ check sự
/// tồn tại, dùng riêng cho lỗi remediation fail-closed khi thiếu mgc.lock.
fn detect_rival_lockfiles(project_root: &Path) -> Vec<String> {
    ["bun.lock", "deno.lock", "bun.lockb"]
        .iter()
        .filter(|name| project_root.join(name).is_file())
        .map(|name| name.to_string())
        .collect()
}

pub async fn run_audit_fix<F, Fut>(
    project_root: &Path,
    vulnerable: &[PackageId],
    resolve_fn: F,
) -> MgResult<usize>
where
    F: FnOnce(mgc_types::Manifest) -> Fut,
    Fut: std::future::Future<Output = MgResult<mgc_types::adapter::ResolvedGraph>>,
{
    if vulnerable.is_empty() {
        return Ok(0);
    }

    let mut manifest = parse_manifest(project_root)?;
    let names: std::collections::HashSet<&str> =
        vulnerable.iter().map(|id| id.name_str()).collect();

    let flags = [
        (false, false, false),
        (true, false, false),
        (false, false, true),
        (false, true, false),
    ];
    let mut bumps: Vec<(DependencySpec, bool, bool, bool)> = Vec::new();
    for (group, (dev, optional, peer)) in manifest.dep_groups().iter().zip(flags) {
        for spec in group.1 {
            if names.contains(spec.name.as_str()) {
                bumps.push((spec.clone(), dev, optional, peer));
            }
        }
    }
    if bumps.is_empty() {
        return Err(MgError::Other(
            "audit --fix: found no matching dependency to bump (validate manifest)".into(),
        ));
    }

    for (spec, dev, optional, peer) in &bumps {
        let new_spec = DependencySpec::new(spec.name.clone(), VersionRange::star());
        manifest.remove_dep(spec.name.as_str());
        manifest.add_dep(new_spec, *dev, *optional, *peer);
    }

    let graph = resolve_fn(manifest.clone()).await?;
    write_manifest(project_root, &manifest)?;
    write_web_lockfile_with_state(project_root, &graph, "fixed")?;
    Ok(bumps.len())
}
