//! Language detection for mgc-lib-adapter.
//! Nhận diện ngôn ngữ lib từ mgc.toml và manifest ecosystem chuẩn.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibLanguage {
    Ts,
    Rust,
    Python,
    Go,
    Java,
    DotNet,
}

impl LibLanguage {
    /// Canonical C0 firewall ecosystem id — the ONLY string
    /// `dep_gate::owner_for` matches on. Single source of truth so CLI
    /// lanes can never drift from the gate table.
    /// (Id ecosystem chuẩn cho tường lửa C0.)
    pub fn ecosystem(&self) -> &'static str {
        match self {
            LibLanguage::Ts => "ts",
            LibLanguage::Rust => "rust",
            LibLanguage::Python => "python",
            LibLanguage::Go => "go",
            LibLanguage::Java => "java",
            LibLanguage::DotNet => "dotnet",
        }
    }
}

type ManifestProbe = fn(&Path) -> Option<String>;

pub fn detect_language(root: &Path) -> Option<LibLanguage> {
    let mgc_toml = root.join("mgc.toml");
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Ok(Some(content)) =
        mgc_config::project::read_regular_project_text(&mgc_toml, "project config")
        && let Ok(v) = toml::from_str::<toml::Value>(&content)
    {
        if v.get("ecosystem")
            .and_then(|e| e.as_str())
            .is_some_and(|eco| eco != "lib")
            && v.get("lib").is_none()
        {
            // A package.json project belongs to the JS lane — the lib
            // lane must not claim it. But a NON-JS manifest (go.mod /
            // Cargo.toml / pyproject / ...) with NO package.json is a
            // backend the lib machinery can serve (web-lane delegation
            // for create-web backend scaffolds) — fall through to marker
            // detection instead of refusing blindly.
            // (Project có package.json thuộc lane JS; manifest non-JS mà
            // không có package.json thì rơi xuống detect marker.)
            if is_regular_manifest(&root.join("package.json")) {
                return None;
            }
        }
        if let Some(lang) = v
            .get("lib")
            .and_then(|l| l.get("language"))
            .and_then(|l| l.as_str())
        {
            return match lang {
                "ts" | "typescript" => Some(LibLanguage::Ts),
                "rust" => Some(LibLanguage::Rust),
                "python" => Some(LibLanguage::Python),
                "go" => Some(LibLanguage::Go),
                "java" | "kotlin" => Some(LibLanguage::Java),
                "dotnet" | "csharp" | "cs" => Some(LibLanguage::DotNet),
                _ => None,
            };
        }
    }
    if is_regular_manifest(&root.join("package.json")) {
        return Some(LibLanguage::Ts);
    }
    if is_regular_manifest(&root.join("Cargo.toml")) {
        return Some(LibLanguage::Rust);
    }
    if is_regular_manifest(&root.join("go.mod")) {
        return Some(LibLanguage::Go);
    }
    // Java/Kotlin: the gradle verification metadata (lockfile) is the
    // audit source — prefer it over the plain build file. A pom.xml
    // project is also Java (native Maven engine, Phase 2).
    // Java/Kotlin: metadata verification gradle (lockfile) là nguồn
    // audit — ưu tiên trước build file thường. Project pom.xml cũng là
    // Java (engine Maven native, Phase 2).
    if is_regular_manifest(&root.join("gradle/verification-metadata.xml"))
        || is_regular_manifest(&root.join("build.gradle"))
        || is_regular_manifest(&root.join("build.gradle.kts"))
        || is_regular_manifest(&root.join("pom.xml"))
    {
        return Some(LibLanguage::Java);
    }
    // .NET: packages.lock.json is the lockfile the audit reads; a bare
    // *.csproj is also .NET (native NuGet engine, Phase 2).
    // .NET: packages.lock.json là lockfile audit đọc; *.csproj trần cũng
    // là .NET (engine NuGet native, Phase 2).
    if is_regular_manifest(&root.join("packages.lock.json")) || find_csproj(root).is_some() {
        return Some(LibLanguage::DotNet);
    }
    if is_regular_manifest(&root.join("pyproject.toml")) {
        return Some(LibLanguage::Python);
    }
    None
}

/// Return the dependency-manifest format whose native writer/resolver owns
/// this lib lane. Language detection alone is insufficient for Java (.NET
/// is likewise native only when a root-level csproj exists).
/// Trả format manifest dependency mà writer/resolver native sở hữu; chỉ biết
/// ngôn ngữ là chưa đủ để xác nhận Java hoặc .NET.
pub fn dependency_manifest_format(root: &Path, language: LibLanguage) -> Option<&'static str> {
    match language {
        LibLanguage::Ts if is_regular_manifest(&root.join("package.json")) => Some("package-json"),
        LibLanguage::Rust if is_regular_manifest(&root.join("Cargo.toml")) => Some("cargo-toml"),
        LibLanguage::Go if is_regular_manifest(&root.join("go.mod")) => Some("go-mod"),
        LibLanguage::Python if is_regular_manifest(&root.join("pyproject.toml")) => {
            if crate::manifest::supports_native_python_project(root) {
                Some("pep621-native")
            } else {
                Some("pyproject-unsupported")
            }
        }
        LibLanguage::Java => {
            let has_pom = is_regular_manifest(&root.join("pom.xml"));
            let has_gradle = is_regular_manifest(&root.join("build.gradle"))
                || is_regular_manifest(&root.join("build.gradle.kts"));
            match (has_pom, has_gradle) {
                (true, false) => Some("maven-pom"),
                (false, true) => Some("gradle"),
                (true, true) => Some("ambiguous"),
                (false, false) => None,
            }
        }
        LibLanguage::DotNet if find_csproj(root).is_some() => Some("csproj"),
        LibLanguage::Ts
        | LibLanguage::Rust
        | LibLanguage::Python
        | LibLanguage::Go
        | LibLanguage::DotNet => None,
    }
}

/// `Path::is_file` follows symlinks; ownership must not treat an external
/// symlink as a project-owned dependency manifest.
/// `Path::is_file` theo symlink; chỉ nhận manifest file thường thuộc project.
fn is_regular_manifest(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
}

/// Find the unique `*.csproj` at the project root. Multiple projects need
/// explicit project selection; choosing whichever `read_dir` returns first
/// would make install/add nondeterministic.
/// Tìm `.csproj` duy nhất ở root; nhiều project cần chọn rõ, không chọn ngẫu
/// nhiên theo thứ tự `read_dir`.
pub(crate) fn find_csproj(root: &Path) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    let mut projects = entries.flatten().map(|e| e.path()).filter(|p| {
        is_regular_manifest(p)
            && p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("csproj"))
    });
    let first = projects.next()?;
    projects.next().is_none().then_some(first)
}

pub(crate) fn manifest_is_lib(root: &Path) -> bool {
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Ok(Some(content)) =
        mgc_config::project::read_regular_project_text(&root.join("mgc.toml"), "project config")
        && let Ok(v) = toml::from_str::<toml::Value>(&content)
    {
        if v.get("ecosystem").and_then(|e| e.as_str()) == Some("lib") {
            return true;
        }
        if v.get("lib").is_some() {
            return true;
        }
    }
    let probes: [(&Path, ManifestProbe); 3] = [
        (&root.join("package.json"), probe_package_json),
        (&root.join("Cargo.toml"), probe_cargo_toml),
        (&root.join("pyproject.toml"), probe_pyproject),
    ];
    for (path, probe) in probes {
        if is_regular_manifest(path)
            && let Some(eco) = probe(path)
            && eco == "lib"
        {
            return true;
        }
    }
    false
}

fn probe_package_json(path: &Path) -> Option<String> {
    let content =
        mgc_config::project::read_regular_project_text(path, "package manifest").ok()??;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v.get("magicore")
        .and_then(|m| m.get("core"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
}

fn probe_cargo_toml(path: &Path) -> Option<String> {
    let content = mgc_config::project::read_regular_project_text(path, "Cargo manifest").ok()??;
    let v: toml::Value = toml::from_str(&content).ok()?;
    v.get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("magicore"))
        .and_then(|mgc| mgc.get("core"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
}

fn probe_pyproject(path: &Path) -> Option<String> {
    let content =
        mgc_config::project::read_regular_project_text(path, "Python manifest").ok()??;
    let v: toml::Value = toml::from_str(&content).ok()?;
    v.get("tool")
        .and_then(|t| t.get("magicore"))
        .and_then(|mgc| mgc.get("core"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
}
