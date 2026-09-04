//! Unit tests for optimizer adapters

use crate::commands::optimizer::adapters::*;
use crate::commands::optimizer::detect::{HardwareInfo, SystemProfile};
use crate::commands::optimizer::runtime_detect::{DetectedRuntime, PackageManager};

#[test]
fn test_all_adapters_count() {
    let adapters = all_adapters();
    assert_eq!(adapters.len(), 12); // 3 web + 3 ai + 4 lib + 2 app
}

#[test]
fn test_find_adapters_nodejs() {
    let runtimes = vec![DetectedRuntime::NodeJs {
        package_manager: PackageManager::Pnpm,
    }];
    let adapters = find_adapters(&runtimes);
    assert_eq!(adapters.len(), 1);
    assert_eq!(adapters[0].name(), "Node.js");
}

#[test]
fn test_find_adapters_multiple() {
    let runtimes = vec![DetectedRuntime::PythonPyTorch, DetectedRuntime::RustCandle];
    let adapters = find_adapters(&runtimes);
    assert_eq!(adapters.len(), 2);
    let names: Vec<_> = adapters.iter().map(|a| a.name()).collect();
    assert!(names.contains(&"PyTorch"));
    assert!(names.contains(&"Rust/Candle"));
}

// Node.js adapter tests
#[test]
fn test_nodejs_adapter_matches() {
    let adapter = node::NodeJsAdapter;
    let runtime = DetectedRuntime::NodeJs {
        package_manager: PackageManager::Npm,
    };
    assert!(adapter.matches(&runtime));
}

#[test]
fn test_nodejs_adapter_generate() {
    let adapter = node::NodeJsAdapter;
    let hw = HardwareInfo {
        os: "macos".to_string(),
        arch: "aarch64".to_string(),
        cpu_cores: 8,
        total_memory_gb: 16,
        profile: SystemProfile::HighPerformance,
    };
    let files = adapter.generate(&hw);
    assert_eq!(files.len(), 1);
    assert!(files[0].content.contains("max-old-space-size=8192"));
    assert!(files[0].content.contains("UV_THREADPOOL_SIZE=8"));
}

// Deno adapter tests
#[test]
fn test_deno_adapter_matches() {
    let adapter = deno::DenoAdapter;
    assert!(adapter.matches(&DetectedRuntime::Deno));
    assert!(!adapter.matches(&DetectedRuntime::Bun));
}

#[test]
fn test_deno_adapter_generate() {
    let adapter = deno::DenoAdapter;
    let hw = HardwareInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_cores: 16,
        total_memory_gb: 32,
        profile: SystemProfile::HighPerformance,
    };
    let files = adapter.generate(&hw);
    assert_eq!(files.len(), 1);
    assert!(files[0].content.contains("DENO_V8_FLAGS"));
    assert!(files[0].content.contains("DENO_JOBS=16"));
}

// Bun adapter tests
#[test]
fn test_bun_adapter_matches() {
    let adapter = bun::BunAdapter;
    assert!(adapter.matches(&DetectedRuntime::Bun));
    assert!(!adapter.matches(&DetectedRuntime::Deno));
}

#[test]
fn test_bun_adapter_generate() {
    let adapter = bun::BunAdapter;
    let hw = HardwareInfo {
        os: "macos".to_string(),
        arch: "aarch64".to_string(),
        cpu_cores: 8,
        total_memory_gb: 16,
        profile: SystemProfile::Standard,
    };
    let files = adapter.generate(&hw);
    assert_eq!(files.len(), 1);
    // BUN_JSC_maxHeapSize removed - invalid Bun env var (rejected by Bun runtime)
    assert!(files[0]
        .content
        .contains("BUN_RUNTIME_TRANSPILER_CACHE_PATH"));
    assert!(files[0].content.contains("BUN_CONFIG_MAX_HTTP_REQUESTS=80"));
}

// PyTorch adapter tests
#[test]
fn test_pytorch_adapter_matches() {
    let adapter = pytorch::PyTorchAdapter;
    assert!(adapter.matches(&DetectedRuntime::PythonPyTorch));
    assert!(!adapter.matches(&DetectedRuntime::RustCandle));
}

#[test]
fn test_pytorch_adapter_generate() {
    let adapter = pytorch::PyTorchAdapter;
    let hw = HardwareInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_cores: 16,
        total_memory_gb: 64,
        profile: SystemProfile::HighPerformance,
    };
    let files = adapter.generate(&hw);
    assert!(files.len() >= 2); // runtime + docker (sharding removed in fix)
    assert!(files[0].content.contains("TORCH_NUM_THREADS=16"));
}

// Candle adapter tests
#[test]
fn test_candle_adapter_matches() {
    let adapter = candle::CandleAdapter;
    assert!(adapter.matches(&DetectedRuntime::RustCandle));
    assert!(!adapter.matches(&DetectedRuntime::PythonPyTorch));
}

#[test]
fn test_candle_adapter_generate() {
    let adapter = candle::CandleAdapter;
    let hw = HardwareInfo {
        os: "macos".to_string(),
        arch: "aarch64".to_string(),
        cpu_cores: 10,
        total_memory_gb: 32,
        profile: SystemProfile::HighPerformance,
    };
    let files = adapter.generate(&hw);
    assert_eq!(files.len(), 2); // runtime + cargo
    assert!(files[0].content.contains("RAYON_NUM_THREADS=10"));
    assert!(files[1].content.contains("opt-level = 3"));
    assert!(files[1].content.contains("lto = \"thin\""));
}

// Go AI adapter tests
#[test]
fn test_go_ai_adapter_matches() {
    let adapter = go_ai::GoAiAdapter;
    assert!(adapter.matches(&DetectedRuntime::GoTensorFlow));
    assert!(!adapter.matches(&DetectedRuntime::GoLib));
}

#[test]
fn test_go_ai_adapter_generate() {
    let adapter = go_ai::GoAiAdapter;
    let hw = HardwareInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_cores: 24,
        total_memory_gb: 128,
        profile: SystemProfile::HighPerformance,
    };
    let files = adapter.generate(&hw);
    assert_eq!(files.len(), 2); // runtime + build
    assert!(files[0].content.contains("GOMAXPROCS=24"));
    assert!(files[0].content.contains("GOGC=100"));
    assert!(files[1].content.contains("CGO_ENABLED=1"));
}
