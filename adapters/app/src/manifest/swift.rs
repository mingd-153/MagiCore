//! Swift Package.swift manifest parsing (Phase 2 — native SwiftPM lane).
//! Parse manifest Package.swift của Swift (Phase 2 — lane SwiftPM native).

use mgc_resolver::protocols::swift::{SwiftDep, parse_package_resolved, parse_swift_package_deps};
use mgc_types::{
    DependencySpec, Ecosystem, Manifest, MgError, MgResult, PackageName, VersionRange,
};
use std::path::Path;

/// Parse only literal `.package(...)` declarations from Package.swift; do
/// not execute SwiftPM or project manifest code during dependency
/// resolution. Unsupported/dynamic declaration shapes fail closed. Existing
/// Package.resolved pins override literal declared requirements.
/// Chỉ parse tĩnh khai báo `.package(...)` literal; không chạy SwiftPM hay
/// mã manifest của project khi resolve. Cú pháp động/không hỗ trợ fail-closed.
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

    let source = std::fs::read_to_string(&swift_path)
        .map_err(|e| MgError::Other(format!("read Package.swift: {e}")))?;
    if !source.contains("Package(") {
        return Err(unsupported_static_swift_manifest(
            "no literal Package(...) declaration was found",
        ));
    }
    let package_calls = source.matches(".package(").count();
    let deps = parse_swift_package_deps(&source);
    if deps.len() != package_calls {
        return Err(unsupported_static_swift_manifest(
            "one or more .package(...) declarations use unsupported or dynamic syntax",
        ));
    }
    if deps.iter().any(|dep| dep.requirement_text() == "*") {
        return Err(unsupported_static_swift_manifest(
            "one or more .package(...) requirements are not recognized as literal versions",
        ));
    }
    if let Some(value) = source
        .split_once("dependencies:")
        .map(|(_, value)| value.trim_start())
        && !value.starts_with('[')
    {
        return Err(unsupported_static_swift_manifest(
            "the dependencies argument is computed instead of a literal array",
        ));
    }

    // Package.resolved pins (v1 object.pins / v2-v3 pins) override the
    // declared requirement — a pin is the exact resolved identity.
    // (Pin Package.resolved (v1 object.pins / v2-v3 pins) ghi đè yêu cầu
    // khai báo — pin là danh tính đã resolve chính xác.)
    let pins = read_pins(project_root)?;

    for dep in deps {
        let dep_name = dep.dep_name();
        let range = pin_range_for(&dep, &pins).unwrap_or_else(|| dep.requirement_text());
        let dep_name = PackageName::new(dep_name).map_err(|e| {
            unsupported_static_swift_manifest(&format!("dependency identity is unsupported: {e}"))
        })?;
        let range = VersionRange::parse(&range).map_err(|e| {
            unsupported_static_swift_manifest(&format!(
                "dependency version range is unsupported: {e}"
            ))
        })?;
        manifest.add_dep(DependencySpec::new(dep_name, range), false, false, false);
    }

    Ok(manifest)
}

fn unsupported_static_swift_manifest(reason: &str) -> MgError {
    MgError::Unsupported {
        core: "app",
        capability: "swift_manifest_parse",
        guidance: format!(
            "Package.swift is executable Swift; MagiCore parses only literal .package(...) declarations without running SwiftPM. {reason}. Use a supported literal manifest shape or manage this project with its Swift toolchain outside MagiCore."
        ),
    }
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
