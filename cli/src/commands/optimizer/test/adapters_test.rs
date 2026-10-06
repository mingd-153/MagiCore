//! Unit tests for optimizer adapters

use crate::commands::optimizer::adapters::*;
use crate::commands::optimizer::detect::{GpuDetectionStatus, HardwareInfo, SystemProfile};
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
        cpu_cores: Some(8),
        total_memory_gb: Some(16),
        profile: SystemProfile::HighPerformance,
        gpus: vec![],
        gpu_detection_status: GpuDetectionStatus::Available,
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
        cpu_cores: Some(16),
        total_memory_gb: Some(32),
        profile: SystemProfile::HighPerformance,
        gpus: vec![],
        gpu_detection_status: GpuDetectionStatus::Available,
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
        cpu_cores: Some(8),
        total_memory_gb: Some(16),
        profile: SystemProfile::Standard,
        gpus: vec![],
        gpu_detection_status: GpuDetectionStatus::Available,
    };
    let files = adapter.generate(&hw);
    assert_eq!(files.len(), 1);
    // BUN_JSC_maxHeapSize removed - invalid Bun env var (rejected by Bun runtime)
    assert!(
        files[0]
            .content
            .contains("BUN_RUNTIME_TRANSPILER_CACHE_PATH")
    );
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
        cpu_cores: Some(16),
        total_memory_gb: Some(64),
        profile: SystemProfile::HighPerformance,
        gpus: vec![],
        gpu_detection_status: GpuDetectionStatus::Available,
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
        cpu_cores: Some(10),
        total_memory_gb: Some(32),
        profile: SystemProfile::HighPerformance,
        gpus: vec![],
        gpu_detection_status: GpuDetectionStatus::Available,
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
        cpu_cores: Some(24),
        total_memory_gb: Some(128),
        profile: SystemProfile::HighPerformance,
        gpus: vec![],
        gpu_detection_status: GpuDetectionStatus::Available,
    };
    let files = adapter.generate(&hw);
    assert_eq!(files.len(), 2); // runtime + build
    assert!(files[0].content.contains("GOMAXPROCS=24"));
    assert!(files[0].content.contains("GOGC=100"));
    assert!(files[1].content.contains("CGO_ENABLED=1"));
    assert!(
        files.iter().all(|file| !file.content.contains("${")),
        "generated env files must not contain shell expansion syntax"
    );
}

fn hw_with_gpus(gpus: Vec<crate::commands::optimizer::detect::GpuInfo>) -> HardwareInfo {
    HardwareInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_cores: Some(8),
        total_memory_gb: Some(32),
        profile: SystemProfile::HighPerformance,
        gpus,
        gpu_detection_status: GpuDetectionStatus::Available,
    }
}

fn test_gpu(
    name: &str,
    vendor: Option<&str>,
    vram_mb: Option<usize>,
) -> crate::commands::optimizer::detect::GpuInfo {
    crate::commands::optimizer::detect::GpuInfo {
        name: name.to_string(),
        vendor: vendor.map(str::to_string),
        vram_mb,
    }
}

#[test]
fn test_pytorch_cuda_only_with_nvidia() {
    let adapter = pytorch::PyTorchAdapter;
    let nvidia_hw = hw_with_gpus(vec![test_gpu(
        "NVIDIA GeForce RTX 4090",
        Some("nvidia"),
        Some(24564),
    )]);
    let content = adapter.generate(&nvidia_hw)[0].content.clone();
    assert!(content.contains("PYTORCH_CUDA_ALLOC_CONF"));
    assert!(!content.contains("PYTORCH_ENABLE_MPS_FALLBACK"));
    // Không GPU — không dòng accelerator nào (không fake hardware)
    let bare_hw = hw_with_gpus(vec![]);
    let content = adapter.generate(&bare_hw)[0].content.clone();
    assert!(!content.contains("PYTORCH_CUDA_ALLOC_CONF"));
    assert!(!content.contains("PYTORCH_ENABLE_MPS_FALLBACK"));
    // Apple GPU — MPS fallback, không CUDA
    let apple_hw = hw_with_gpus(vec![test_gpu("Apple M2", Some("apple"), None)]);
    let content = adapter.generate(&apple_hw)[0].content.clone();
    assert!(content.contains("PYTORCH_ENABLE_MPS_FALLBACK=1"));
    assert!(!content.contains("PYTORCH_CUDA_ALLOC_CONF"));
}

#[test]
fn test_candle_cuda_only_with_nvidia() {
    let adapter = candle::CandleAdapter;
    let nvidia_hw = hw_with_gpus(vec![test_gpu("Tesla V100", Some("nvidia"), Some(16384))]);
    let content = adapter.generate(&nvidia_hw)[0].content.clone();
    assert!(content.contains("CANDLE_ENABLE_CUDA=auto"));
    let bare_hw = hw_with_gpus(vec![]);
    let content = adapter.generate(&bare_hw)[0].content.clone();
    assert!(!content.contains("CANDLE_ENABLE_CUDA=auto"));
    assert!(content.contains("omitted"));
}

#[test]
fn test_gpu_vendor_queries_preserve_unknown_vs_absent() {
    let hw = hw_with_gpus(vec![]);
    assert_eq!(hw.has_gpu_vendor("nvidia"), Some(false));
    assert_eq!(hw.has_gpu_vendor("apple"), Some(false));
    let hw = hw_with_gpus(vec![test_gpu("Apple M2", Some("apple"), None)]);
    assert_eq!(hw.has_gpu_vendor("apple"), Some(true));
    assert_eq!(hw.has_gpu_vendor("nvidia"), Some(false));
    let unknown = HardwareInfo {
        gpu_detection_status: GpuDetectionStatus::Unavailable,
        ..hw
    };
    assert_eq!(unknown.has_gpu_vendor("apple"), None);
}

#[test]
fn test_unknown_cpu_omits_cpu_derived_settings_across_adapters() {
    let hw = HardwareInfo {
        cpu_cores: None,
        arch: "unknown".to_string(),
        os: "unknown".to_string(),
        total_memory_gb: Some(16),
        profile: SystemProfile::Constrained,
        gpus: vec![],
        gpu_detection_status: GpuDetectionStatus::Unavailable,
    };
    let adapters: Vec<Box<dyn OptimizerAdapter>> = vec![
        Box::new(node::NodeJsAdapter),
        Box::new(deno::DenoAdapter),
        Box::new(bun::BunAdapter),
        Box::new(pytorch::PyTorchAdapter),
        Box::new(candle::CandleAdapter),
        Box::new(go_ai::GoAiAdapter),
        Box::new(go_lib::GoLibAdapter),
        Box::new(rust_lib::RustLibAdapter),
        Box::new(python_lib::PythonLibAdapter),
        Box::new(react_native::ReactNativeAdapter),
    ];
    let cpu_keys = [
        "UV_THREADPOOL_SIZE=",
        "DENO_JOBS=",
        "BUN_CONFIG_MAX_HTTP_REQUESTS=",
        "OMP_NUM_THREADS=",
        "MKL_NUM_THREADS=",
        "TORCH_NUM_THREADS=",
        "CONTAINER_CPUS=",
        "RAYON_NUM_THREADS=",
        "GOMAXPROCS=",
        "UV_CONCURRENT_DOWNLOADS=",
        "METRO_MAX_WORKERS=",
        "jobs = ",
    ];
    for adapter in adapters {
        for file in adapter.generate(&hw) {
            for line in file.content.lines() {
                assert!(
                    !cpu_keys.iter().any(|key| line.starts_with(key)),
                    "{} emitted CPU-derived setting with unknown CPU count: {line}",
                    adapter.name()
                );
            }
        }
    }
}
