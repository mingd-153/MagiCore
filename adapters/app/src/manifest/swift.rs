//! Swift Package.swift manifest parsing (Phase 2 — native SwiftPM lane).
//! Parse manifest Package.swift của Swift (Phase 2 — lane SwiftPM native).

use mgc_resolver::protocols::swift::{
    SwiftDep, parse_dump_package, parse_package_resolved, parse_swift_package_deps,
};
use mgc_types::{
    DependencySpec, Ecosystem, Manifest, MgError, MgResult, PackageName, VersionRange,
};
use std::path::Path;

/// Parse Package.swift to Manifest via `swift package dump-package`, then
/// apply the pins recorded in an existing Package.resolved (pins win: the
/// resolved file IS the lock).
///
/// The Swift toolchain is REQUIRED to read a Package.swift (it is Swift
/// source, not data — only the compiler can evaluate it). Missing/failing
/// toolchain fails closed with guidance instead of pretending the project
/// has no dependencies.
/// Parse Package.swift thành Manifest qua `swift package dump-package`, rồi
/// áp pin ghi trong Package.resolved có sẵn (pin thắng: file resolved CHÍNH
/// LÀ lock).
///
/// Toolchain Swift là BẮT BUỘC để đọc Package.swift (đây là mã nguồn Swift,
/// không phải dữ liệu — chỉ compiler đánh giá được). Toolchain thiếu/lỗi thì
/// fail-closed kèm hướng dẫn thay vì giả vờ project không có dependency.
pub fn parse_package_swift(project_root: &Path) -> MgResult<Manifest> {
    let swift_path = project_root.join("Package.swift");
    if !swift_path.exists() {
        return Err(MgError::Other("Package.swift not found".to_string()));
    }

    let name = project_root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "app".to_string());
    let mut manifest = Manifest::new(&name, Ecosystem::App);

    let dump = run_dump_package(project_root)?;
    let deps = parse_dump_package(&dump)?;

    // Package.resolved pins (v1 object.pins / v2-v3 pins) override the
    // declared requirement — a pin is the exact resolved identity.
    // (Pin Package.resolved (v1 object.pins / v2-v3 pins) ghi đè yêu cầu
    // khai báo — pin là danh tính đã resolve chính xác.)
    let pins = read_pins(project_root)?;

    for dep in deps {
        let dep_name = dep.dep_name();
        let range = pin_range_for(&dep, &pins).unwrap_or_else(|| dep.requirement_text());
        let Ok(dep_name) = PackageName::new(dep_name) else {
            eprintln!(
                "WARNING: Swift dependency '{}' is not a valid mgc package name — skipped",
                dep.dep_name()
            );
            continue;
        };
        let Ok(range) = VersionRange::parse(&range) else {
            continue;
        };
        manifest.add_dep(DependencySpec::new(dep_name, range), false, false, false);
    }

    Ok(manifest)
}

/// Run `swift package dump-package` through mgc-exec (allowlisted) and
/// return the JSON on stdout. Non-zero exit or a missing toolchain is a
/// fail-closed error carrying the command guidance.
/// Chạy `swift package dump-package` qua mgc-exec (đã allowlist) và trả
/// JSON trên stdout. Exit khác 0 hoặc thiếu toolchain là lỗi fail-closed
/// kèm hướng dẫn lệnh.
fn run_dump_package(project_root: &Path) -> MgResult<String> {
    let args = vec!["package".to_string(), "dump-package".to_string()];
    let opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        // dump-package emits the whole manifest JSON — the line-bounded
        // tail would truncate it.
        // (dump-package xuất toàn bộ JSON manifest — tail giới hạn dòng sẽ
        // cắt cụt.)
        capture_full_stdout: true,
        ..Default::default()
    };
    let report = mgc_exec::run::run("swift", &args, &opts).map_err(|e| {
        MgError::Other(format!(
            "`swift package dump-package` could not run: {e}\n\
             Swift manifest parsing requires the Swift toolchain (install Xcode/\
             swift, or set the toolchain on PATH) — fail-closed"
        ))
    })?;
    if report.exit_code != 0 {
        return Err(MgError::Other(format!(
            "`swift package dump-package` exited {}: {} (fail-closed)",
            report.exit_code,
            report.stderr_tail.trim()
        )));
    }
    if report.stdout_full.trim().is_empty() {
        return Err(MgError::Other(
            "`swift package dump-package` produced no output — cannot read the manifest (fail-closed)"
                .to_string(),
        ));
    }
    Ok(report.stdout_full)
}

/// Read Package.resolved pins (absent file = no pins, not an error).
/// Đọc pin Package.resolved (thiếu file = không pin, không phải lỗi).
fn read_pins(
    project_root: &Path,
) -> MgResult<Vec<mgc_resolver::protocols::swift::SwiftResolvedPin>> {
    let path = project_root.join("Package.resolved");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| MgError::Other(format!("read Package.resolved: {e}")))?;
    parse_package_resolved(&text)
}

/// Pin range for a dependency (registry pins match `scope.name` identity,
/// remote pins match the repository name), or `None` when the pin does not
/// apply (the declared requirement stands).
/// Range pin cho một dependency (pin registry khớp identity `scope.name`,
/// pin remote khớp tên repository), hoặc `None` khi pin không áp dụng (giữ
/// yêu cầu đã khai báo).
fn pin_range_for(
    dep: &SwiftDep,
    pins: &[mgc_resolver::protocols::swift::SwiftResolvedPin],
) -> Option<String> {
    match dep {
        SwiftDep::Registry { identity, .. } => {
            // Registry pins are keyed by identity (dump-package emits
            // `scope.name`; Package.resolved v2/v3 uses the same shape).
            // (Pin registry khóa theo identity (dump-package phát
            // `scope.name`; Package.resolved v2/v3 cùng hình dạng).)
            let normalized = identity.to_lowercase();
            pins.iter()
                .find(|p| p.identity.to_lowercase() == normalized)
                .and_then(pin_requirement)
        }
        SwiftDep::Git { url, .. } => {
            let repo = url
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .trim_end_matches(".git")
                .to_lowercase();
            pins.iter()
                .find(|p| {
                    p.identity.to_lowercase() == repo
                        || p.location.as_ref().is_some_and(|l| {
                            l.trim_end_matches(".git").ends_with(&url_to_tail(url))
                        })
                })
                .and_then(pin_requirement)
        }
    }
}

/// Tail of a git URL (`host/path/repo`) used for pin location matching.
/// Đuôi URL git (`host/path/repo`) dùng để khớp location pin.
fn url_to_tail(url: &str) -> String {
    url.trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit("://")
        .next()
        .unwrap_or(url)
        .trim_end_matches(".git")
        .to_string()
}

/// Pin → requirement string: a version pin is exact for registry deps and
/// a tag for git deps; a revision/branch pin rides the git-only prefixes
/// the engine understands. Registry locations in Package.resolved are
/// `registry+https://…` — the `registry+` scheme prefix marks a REGISTRY
/// pin, never a remote one.
/// Pin → chuỗi requirement: pin version là exact cho dep registry và tag
/// cho dep git; pin revision/branch mang tiền tố chỉ-git mà engine hiểu.
/// Location registry trong Package.resolved là `registry+https://…` — tiền
/// tố scheme `registry+` đánh dấu pin REGISTRY, không bao giờ remote.
fn pin_requirement(pin: &mgc_resolver::protocols::swift::SwiftResolvedPin) -> Option<String> {
    let is_remote = pin
        .location
        .as_ref()
        .is_some_and(|l| l.contains("://") && !l.starts_with("registry+"));
    if let Some(v) = &pin.version {
        return Some(if is_remote {
            format!("tag:{v}")
        } else {
            format!("exact:{v}")
        });
    }
    if let Some(r) = &pin.revision {
        return Some(format!("revision:{r}"));
    }
    if let Some(b) = &pin.branch {
        return Some(format!("branch:{b}"));
    }
    None
}

/// Write Manifest back to Package.swift.
///
/// NOT IMPLEMENTED (honest): Package.swift is Swift SOURCE — mgc never
/// rewrites it (unchanged no-op; the LockfileProvider claim for Swift rests
/// on `Package.resolved`, written by the native install lane).
/// Viết Manifest trả về Package.swift.
///
/// KHÔNG TRIỂN KHAI (trung thực): Package.swift là MÃ NGUỒN Swift — mgc
/// không bao giờ viết lại (no-op giữ nguyên; claim LockfileProvider cho
/// Swift dựa trên `Package.resolved` do lane install native ghi).
pub fn write_package_swift(_project_root: &Path, _manifest: &Manifest) -> MgResult<()> {
    Ok(())
}

/// Deps declared by a Package.swift TEXT (pure-text scan — diagnostics and
/// tests for projects whose toolchain is unavailable).
/// Dep khai báo trong NỘI DUNG Package.swift (quét thuần văn bản — chẩn
/// đoán và test cho project không có toolchain).
pub fn scan_package_swift_text(text: &str) -> Vec<SwiftDep> {
    parse_swift_package_deps(text)
}
