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
    if project_root.join("pnpm-lock.yaml").exists() {
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

    // Check for Python/PyTorch (pyproject.toml + torch dependency) — kiểm tra Python/PyTorch
    if project_root.join("pyproject.toml").exists() {
        if let Ok(content) = std::fs::read_to_string(project_root.join("pyproject.toml")) {
            if content.contains("torch") || content.contains("pytorch") {
                runtimes.push(DetectedRuntime::PythonPyTorch);
            } else {
                // Generic Python AI project — project Python AI chung
                runtimes.push(DetectedRuntime::PythonPyTorch);
            }
        }
    }

    // Check for Rust/Candle (Cargo.toml + candle dependency) — kiểm tra Rust/Candle
    if project_root.join("Cargo.toml").exists() {
        if let Ok(content) = std::fs::read_to_string(project_root.join("Cargo.toml")) {
            if content.contains("candle") || content.contains("burn") {
                runtimes.push(DetectedRuntime::RustCandle);
            }
        }
    }

    // Check for Go AI (go.mod + tensorflow/onnx) — kiểm tra Go AI
    if project_root.join("go.mod").exists() {
        if let Ok(content) = std::fs::read_to_string(project_root.join("go.mod")) {
            if content.contains("tensorflow") || content.contains("onnx") {
                runtimes.push(DetectedRuntime::GoTensorFlow);
            }
        }
    }

    runtimes
}

fn detect_lib_runtime(project_root: &Path) -> Vec<DetectedRuntime> {
    let mut runtimes = vec![];

    // Check for Rust lib (Cargo.toml with [lib]) — kiểm tra thư viện Rust
    if project_root.join("Cargo.toml").exists() {
        if let Ok(content) = std::fs::read_to_string(project_root.join("Cargo.toml")) {
            if content.contains("[lib]") || content.contains("crate-type") {
                runtimes.push(DetectedRuntime::RustLib);
            }
        }
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
    if project_root.join("package.json").exists() && project_root.join("tsconfig.json").exists() {
        if let Ok(content) = std::fs::read_to_string(project_root.join("package.json")) {
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
    if project_root.join("package.json").exists() {
        if let Ok(content) = std::fs::read_to_string(project_root.join("package.json")) {
            if content.contains("react-native") || project_root.join("metro.config.js").exists() {
                runtimes.push(DetectedRuntime::ReactNative);
                return runtimes;
            }
        }
    }

    // Check for Rust native (Cargo.toml with bin or no crate-type=lib) — kiểm tra ứng dụng Rust native
    if project_root.join("Cargo.toml").exists() {
        runtimes.push(DetectedRuntime::RustNative);
    }

    runtimes
}
