//! `detect.rs` — ONE manifest-detection table shared by every adapter
//! (R1). Each constructor answers a single question — "does this project
//! root carry a manifest this shared scanner understands?" — and, when
//! yes, returns the ready-to-run ScanStep. Adapters keep their OWN
//! opt-in set (which lanes belong to this core) but must not hand-roll
//! the file-existence rules: one rule per manifest, every core the same.
//!
//! Bảng nhận diện manifest DUY NHẤT cho mọi adapter. Mỗi constructor trả
//! lời một câu hỏi — root có manifest mà scanner chung hiểu không — và
//! trả ScanStep chạy được. Adapter giữ opt-in riêng (lane nào thuộc core
//! nào) nhưng không tự viết luật nhận diện: một manifest một luật, mọi
//! core giống nhau.

use crate::contract::ScanStep;
use std::path::Path;

/// Cargo.toml → MGC's native OSV scanner (rust).
/// Cargo.toml → scanner OSV native của MGC (Rust).
pub fn rust_step(project_root: &Path) -> Option<ScanStep<'static>> {
    if !project_root.join("Cargo.toml").is_file() {
        return None;
    }
    let root = project_root.to_path_buf();
    Some(ScanStep {
        ecosystem: "rust",
        scanner: "mgc-rust-osv",
        run: Box::new(move || {
            let root = root.clone();
            Box::pin(async move { crate::scanners::audit_rust(&root).await })
        }),
    })
}

/// Any Python dependency manifest → MGC's native OSV scanner (python). Recognized set
/// mirrors the scanner's own routing: requirements variants, PEP 751
/// pylock variants, uv.lock, pyproject.toml. A manifest the scanner
/// cannot consume (uv.lock without export, pyproject without lockfile)
/// still yields a step — the SCANNER reports the honest Failed, the
/// gate never hides a recognized manifest.
/// Mọi manifest dependency Python → scanner OSV native của MGC. Manifest scanner không
/// đọc được vẫn cho step — SCANNER báo Failed trung thực, gate không
/// giấu manifest đã nhận diện.
pub fn python_step(project_root: &Path) -> Option<ScanStep<'static>> {
    let recognized = crate::scanners::find_requirements_file(project_root).is_some()
        || crate::scanners::find_pylock_file(project_root).is_some()
        || project_root.join("uv.lock").is_file()
        || project_root.join("pyproject.toml").is_file();
    if !recognized {
        return None;
    }
    let root = project_root.to_path_buf();
    Some(ScanStep {
        ecosystem: "python",
        scanner: "mgc-python-osv",
        run: Box::new(move || {
            let root = root.clone();
            Box::pin(async move { crate::scanners::audit_python(&root).await })
        }),
    })
}

/// go.mod → MGC's native OSV scanner (go).
/// go.mod → scanner OSV native của MGC (Go).
pub fn go_step(project_root: &Path) -> Option<ScanStep<'static>> {
    if !project_root.join("go.mod").is_file() {
        return None;
    }
    let root = project_root.to_path_buf();
    Some(ScanStep {
        ecosystem: "go",
        scanner: "mgc-go-osv",
        run: Box::new(move || {
            let root = root.clone();
            Box::pin(async move { crate::scanners::audit_go(&root).await })
        }),
    })
}

/// build.gradle / build.gradle.kts / pom.xml → OSV Maven (java).
/// The lane reads gradle/verification-metadata.xml pins, else pom.xml
/// dependencies; without either the scanner reports honest Unsupported
/// with remediation.
/// (build.gradle/pom.xml → OSV Maven.)
pub fn java_step(project_root: &Path) -> Option<ScanStep<'static>> {
    if !project_root.join("build.gradle").is_file()
        && !project_root.join("build.gradle.kts").is_file()
        && !project_root.join("pom.xml").is_file()
    {
        return None;
    }
    let root = project_root.to_path_buf();
    Some(ScanStep {
        ecosystem: "java",
        scanner: "osv-maven",
        run: Box::new(move || {
            let root = root.clone();
            Box::pin(async move { crate::scanners::audit_java(&root).await })
        }),
    })
}

/// *.csproj / *.sln → OSV NuGet (dotnet). The lane reads root and
/// per-solution-project packages.lock.json pins; without any the
/// scanner reports honest Unsupported with remediation.
/// (*.csproj/*.sln → OSV NuGet.)
pub fn dotnet_step(project_root: &Path) -> Option<ScanStep<'static>> {
    let has_csproj = std::fs::read_dir(project_root)
        .map(|it| {
            it.filter_map(|e| e.ok()).any(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.ends_with(".csproj") || n.ends_with(".sln"))
            })
        })
        .unwrap_or(false);
    if !has_csproj {
        return None;
    }
    let root = project_root.to_path_buf();
    Some(ScanStep {
        ecosystem: "dotnet",
        scanner: "osv-nuget",
        run: Box::new(move || {
            let root = root.clone();
            Box::pin(async move { crate::scanners::audit_dotnet(&root).await })
        }),
    })
}
