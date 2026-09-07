//! `runtime_detect.rs` — Runtime Detection Layer for Core-Neutral Optimizer
//! `runtime_detect.rs` — Lớp phát hiện runtime cho Optimizer trung lập Core

use std::path::Path;

/// Detected runtime environment for a project — môi trường runtime đã phát hiện cho project
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedRuntime {
    // Web runtimes — runtime web
    NodeJs { package_manager: PackageManager },
    Deno,
    Bun,

    // AI runtimes — runtime AI
    PythonPyTorch,
    RustCandle,
    GoTensorFlow,

    // Lib runtimes — runtime thư viện
    RustLib,
    GoLib,
    PythonLib,
    TypeScriptLib,

    // App runtimes — runtime ứng dụng
    Flutter,
    ReactNative,
    RustNative,

    // Fallback — dự phòng
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
    Deno,
}

/// Detect runtime(s) for a project at given path — phát hiện runtime cho project tại đường dẫn
/// Multiple runtimes can coexist (e.g., monorepo with web + ai) — nhiều runtime có thể cùng tồn tại
pub fn detect_runtimes(project_root: &Path, core: &str) -> Vec<DetectedRuntime> {
    let mut runtimes = vec![];

    match core {
        "web" => {
            runtimes.extend(detect_web_runtime(project_root));
        }
        "ai" => {
            runtimes.extend(detect_ai_runtime(project_root));
        }
        "lib" => {
            runtimes.extend(detect_lib_runtime(project_root));
        }
        "app" => {
            runtimes.extend(detect_app_runtime(project_root));
        }
        _ => {
            // Game/iot/cloud/cicd — generic detection or fallback — phát hiện chung hoặc dự phòng
            if project_root.join("Cargo.toml").exists() {
                runtimes.push(DetectedRuntime::RustLib);
            }
        }
    }

    if runtimes.is_empty() {
        runtimes.push(DetectedRuntime::Unknown);
    }

    runtimes
}

fn detect_web_runtime(project_root: &Path) -> Vec<DetectedRuntime> {
    let mut runtimes = vec![];

    // Check for Deno (deno.json or deno.jsonc) — kiểm tra Deno
    if project_root.join("deno.json").exists() || project_root.join("deno.jsonc").exists() {
        runtimes.push(DetectedRuntime::Deno);
        return runtimes; // Deno is exclusive — Deno là độc quyền
    }

    // Check for Bun (bun.lockb or bunfig.toml) — kiểm tra Bun
    if project_root.join("bun.lockb").exists() || project_root.join("bunfig.toml").exists() {
        runtimes.push(DetectedRuntime::Bun);
        return runtimes; // Bun is exclusive — Bun là độc quyền
    }

    // Check for Node.js (package.json) — kiểm tra Node.js
    if project_root.join("package.json").exists() {
        let pm = detect_package_manager(project_root);
        runtimes.push(DetectedRuntime::NodeJs {
            package_manager: pm,
        });
    }

    runtimes
}

fn detect_package_manager(project_root: &Path) -> PackageManager {
    // Check lockfiles to determine package manager — kiểm tra lockfile để xác định package manager
    if project_root.join("deno.lock").exists() {
        PackageManager::Deno
    } else if project_root.join("pnpm-lock.yaml").exists() {
        PackageManager::Pnpm
    } else if project_root.join("yarn.lock").exists() {
        PackageManager::Yarn
    } else if project_root.join("bun.lockb").exists() {
        PackageManager::Bun
    } else if project_root.join("package-lock.json").exists() {
        PackageManager::Npm
    } else {
        // Default to npm if no lockfile — mặc định npm nếu không có lockfile
        PackageManager::Npm
    }
}

fn detect_ai_runtime(project_root: &Path) -> Vec<DetectedRuntime> {
    let mut runtimes = vec![];

    // Check declared Python/PyTorch dependencies — kiểm tra dependency Python/PyTorch đã khai báo
    if python_project_declares_package(project_root, "torch") {
        runtimes.push(DetectedRuntime::PythonPyTorch);
    }

    // Check for Rust/Candle (Cargo.toml + candle dependency) — kiểm tra Rust/Candle
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if project_root.join("Cargo.toml").exists()
        && let Ok(content) = std::fs::read_to_string(project_root.join("Cargo.toml"))
        && (content.contains("candle") || content.contains("burn"))
    {
        runtimes.push(DetectedRuntime::RustCandle);
    }

    // Check for Go AI (go.mod + tensorflow/onnx) — kiểm tra Go AI
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if project_root.join("go.mod").exists()
        && let Ok(content) = std::fs::read_to_string(project_root.join("go.mod"))
        && (content.contains("tensorflow") || content.contains("onnx"))
    {
        runtimes.push(DetectedRuntime::GoTensorFlow);
    }

    runtimes
}

/// Detect a normalized Python dependency in supported manifest sections.
/// Phát hiện dependency Python đã chuẩn hóa trong các vùng manifest được hỗ trợ.
pub(crate) fn python_project_declares_package(project_root: &Path, package: &str) -> bool {
    let normalized_package = normalize_python_package_name(package);
    requirements_file_declares_package(project_root.join("requirements.txt"), &normalized_package)
        || std::fs::read_to_string(project_root.join("pyproject.toml"))
            .ok()
            .and_then(|content| content.parse::<toml::Value>().ok())
            .is_some_and(|manifest| pyproject_declares_package(&manifest, &normalized_package))
}

fn requirements_file_declares_package(path: std::path::PathBuf, package: &str) -> bool {
    std::fs::read_to_string(path).ok().is_some_and(|content| {
        content.lines().any(|line| {
            let requirement = line.split('#').next().unwrap_or_default().trim();
            normalize_python_requirement(requirement) == package
        })
    })
}

fn pyproject_declares_package(manifest: &toml::Value, package: &str) -> bool {
    let project = manifest.get("project");
    let pep621_runtime = project
        .and_then(|value| value.get("dependencies"))
        .is_some_and(|value| dependency_array_contains(value, package));
    let pep621_optional = project
        .and_then(|value| value.get("optional-dependencies"))
        .is_some_and(|value| dependency_group_table_contains(value, package));
    let pep735_groups = manifest
        .get("dependency-groups")
        .is_some_and(|value| dependency_group_table_contains(value, package));
    let poetry = manifest.get("tool").and_then(|value| value.get("poetry"));
    let poetry_runtime = poetry
        .and_then(|value| value.get("dependencies"))
        .is_some_and(|value| dependency_key_table_contains(value, package));
    let poetry_groups = poetry
        .and_then(|value| value.get("group"))
        .is_some_and(|value| poetry_group_table_contains(value, package));

    pep621_runtime || pep621_optional || pep735_groups || poetry_runtime || poetry_groups
}

fn dependency_array_contains(value: &toml::Value, package: &str) -> bool {
    value.as_array().is_some_and(|dependencies| {
        dependencies.iter().any(|dependency| {
            dependency
                .as_str()
                .is_some_and(|requirement| normalize_python_requirement(requirement) == package)
        })
    })
}

fn dependency_group_table_contains(value: &toml::Value, package: &str) -> bool {
    value.as_table().is_some_and(|groups| {
        groups
            .values()
            .any(|dependencies| dependency_array_contains(dependencies, package))
    })
}

fn dependency_key_table_contains(value: &toml::Value, package: &str) -> bool {
    value.as_table().is_some_and(|dependencies| {
        dependencies
            .keys()
            .any(|dependency| normalize_python_package_name(dependency) == package)
    })
}

fn poetry_group_table_contains(value: &toml::Value, package: &str) -> bool {
    value.as_table().is_some_and(|groups| {
        groups.values().any(|group| {
            group
                .get("dependencies")
                .is_some_and(|dependencies| dependency_key_table_contains(dependencies, package))
        })
    })
}

fn normalize_python_requirement(requirement: &str) -> String {
    let name = requirement
        .split(|character: char| {
            character.is_whitespace()
                || matches!(character, '[' | '<' | '>' | '=' | '!' | '~' | '@' | ';')
        })
        .next()
        .unwrap_or_default();
    normalize_python_package_name(name)
}

fn normalize_python_package_name(package: &str) -> String {
    let mut normalized = String::with_capacity(package.len());
    let mut separator_pending = false;
    for character in package.trim().chars() {
        match character {
            '-' | '_' | '.' => separator_pending = !normalized.is_empty(),
            _ => {
                if separator_pending {
                    normalized.push('-');
                    separator_pending = false;
                }
                normalized.extend(character.to_lowercase());
            }
        }
    }
    normalized
}

fn detect_lib_runtime(project_root: &Path) -> Vec<DetectedRuntime> {
    let mut runtimes = vec![];

    // Cargo treats src/lib.rs as an implicit library target even without [lib].
    // Cargo coi src/lib.rs là library target ngầm ngay cả khi không có [lib].
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if project_root.join("Cargo.toml").exists()
        && let Ok(content) = std::fs::read_to_string(project_root.join("Cargo.toml"))
        && (project_root.join("src/lib.rs").is_file()
            || content.contains("[lib]")
            || content.contains("crate-type"))
    {
        runtimes.push(DetectedRuntime::RustLib);
    }

    // Check for Go lib (go.mod) — kiểm tra thư viện Go
    if project_root.join("go.mod").exists() {
        runtimes.push(DetectedRuntime::GoLib);
    }

    // Check for Python lib (pyproject.toml with build-system or setup.py) — kiểm tra thư viện Python
    if project_root.join("pyproject.toml").exists() || project_root.join("setup.py").exists() {
        runtimes.push(DetectedRuntime::PythonLib);
    }

    // Check for TypeScript lib (package.json + tsconfig.json, no framework) — kiểm tra thư viện TypeScript
    if project_root.join("package.json").exists()
        && project_root.join("tsconfig.json").exists()
        && let Ok(content) = std::fs::read_to_string(project_root.join("package.json"))
    {
        // Not a web framework if no "react", "vue", "svelte", "next" etc. — không phải web framework
        if !content.contains("react")
            && !content.contains("vue")
            && !content.contains("svelte")
            && !content.contains("next")
            && !content.contains("vite")
        {
            runtimes.push(DetectedRuntime::TypeScriptLib);
        }
    }

    runtimes
}

fn detect_app_runtime(project_root: &Path) -> Vec<DetectedRuntime> {
    let mut runtimes = vec![];

    // Check for Flutter (pubspec.yaml) — kiểm tra Flutter
    if project_root.join("pubspec.yaml").exists() {
        runtimes.push(DetectedRuntime::Flutter);
        return runtimes; // Flutter is exclusive for app — Flutter là độc quyền cho app
    }

    // Check for React Native (package.json + metro.config.js or react-native dependency) — kiểm tra React Native
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if project_root.join("package.json").exists()
        && let Ok(content) = std::fs::read_to_string(project_root.join("package.json"))
        && (content.contains("react-native") || project_root.join("metro.config.js").exists())
    {
        runtimes.push(DetectedRuntime::ReactNative);
        return runtimes;
    }

    // Check for Rust native (Cargo.toml with bin or no crate-type=lib) — kiểm tra ứng dụng Rust native
    if project_root.join("Cargo.toml").exists() {
        runtimes.push(DetectedRuntime::RustNative);
    }

    runtimes
}
