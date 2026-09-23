//! Unit tests for MagiCore Optimizer Engine

use super::detect::*;
use super::generators::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_hardware_detect_returns_valid_info() {
    // Environment-independent shape assertions: cores/arch/os are always
    // known; RAM may be Unknown (None) on machines where detection is
    // unavailable (containers, sandboxes) — that is honest, not a failure.
    // A detected value, when present, must be positive and consistent
    // with a non-Constrained profile.
    // (Assert hình dạng, không assert môi trường: RAM có thể Unknown.)
    let hw = HardwareInfo::detect();
    assert!(hw.cpu_cores > 0);
    assert!(!hw.arch.is_empty());
    assert!(!hw.os.is_empty());
    match hw.total_memory_gb {
        Some(gb) => assert!(gb > 0, "detected RAM must be positive, got {gb}"),
        None => assert_eq!(
            hw.profile,
            SystemProfile::Constrained,
            "unknown RAM must degrade to Constrained"
        ),
    }
}

#[test]
fn test_generate_optimizations_for_web_core() {
    let dir = tempdir().unwrap();
    let project_root = dir.path();

    // Create package.json to trigger Node.js detection
    fs::write(project_root.join("package.json"), "{}").unwrap();

    let hw = HardwareInfo {
        cpu_cores: 8,
        arch: "aarch64".to_string(),
        os: "macos".to_string(),
        total_memory_gb: Some(16),
        profile: SystemProfile::HighPerformance,
        gpus: vec![],
    };

    let files = generate_optimizations_for_core("web", &hw, project_root);
    // Profile JSON + Node.js Env (from adapter)
    assert!(files.len() >= 2);
    assert!(files[0].relative_path.contains("profile.json"));
    // Node.js adapter should generate config
    assert!(
        files
            .iter()
            .any(|f| f.content.contains("max-old-space-size"))
    );
}

#[test]
fn test_generate_optimizations_for_game_core() {
    let dir = tempdir().unwrap();
    let project_root = dir.path();

    // Create Cargo.toml to trigger Rust detection
    fs::write(
        project_root.join("Cargo.toml"),
        "[package]\nname = \"test\"\n",
    )
    .unwrap();

    let hw = HardwareInfo {
        cpu_cores: 12,
        arch: "x86_64".to_string(),
        os: "linux".to_string(),
        total_memory_gb: Some(32),
        profile: SystemProfile::HighPerformance,
        gpus: vec![],
    };

    let files = generate_optimizations_for_core("game", &hw, project_root);
    assert!(files.len() >= 2);
    // Rust lib adapter should generate Cargo profile config
    assert!(files.iter().any(|f| f.content.contains("jobs")));
}

#[test]
fn test_profile_for_unknown_ram_degrades_to_constrained() {
    // Unknown RAM must never tune from a guessed size: even a 64-core
    // machine degrades to Constrained when memory cannot be measured.
    // (RAM unknown không bao giờ tuning từ số đoán: máy 64 core cũng hạ
    // về Constrained khi không đo được RAM.)
    assert_eq!(
        HardwareInfo::profile_for(64, None),
        SystemProfile::Constrained
    );
    assert_eq!(
        HardwareInfo::profile_for(8, Some(16)),
        SystemProfile::HighPerformance
    );
    assert_eq!(
        HardwareInfo::profile_for(4, Some(8)),
        SystemProfile::Standard
    );
    assert_eq!(
        HardwareInfo::profile_for(2, Some(4)),
        SystemProfile::Constrained
    );
}

#[test]
fn test_unknown_ram_skips_memory_derived_configs() {
    // Unknown RAM (None — not Some(0), which would claim a measured
    // zero): only the honest manifest + RAM-independent GPU facts may be
    // emitted, never adapter files with degenerate memory-derived values.
    // (RAM Unknown: chỉ manifest trung thực + fact GPU.)
    let dir = tempdir().unwrap();
    let project_root = dir.path();
    fs::write(project_root.join("package.json"), "{}").unwrap();

    let hw = HardwareInfo {
        cpu_cores: 8,
        arch: "x86_64".to_string(),
        os: "linux".to_string(),
        total_memory_gb: None,
        profile: SystemProfile::Constrained,
        gpus: vec![],
    };

    let files = generate_optimizations_for_core("web", &hw, project_root);
    assert_eq!(files.len(), 2);
    assert!(files[0].relative_path.contains("profile.json"));
    assert!(files[1].relative_path.contains("gpu.env"));
}

#[test]
fn test_optimize_project_without_runtime_fails_closed() {
    // No detectable runtime is NOT a success: automation must observe an
    // error, never Ok(()) for work that never happened.
    // (Không có runtime không phải thành công: automation phải thấy lỗi.)
    let dir = tempdir().unwrap();
    let result = super::optimize_project(dir.path(), "web", false);
    assert!(
        result.is_err(),
        "optimize with no detectable runtime must fail closed"
    );
}

#[test]
fn test_hash_guard_prevents_overwriting_user_custom_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let opt_file = OptimizedConfigFile {
        relative_path: ".mgc-optimizer/test.env".to_string(),
        content: "# Generated by MagiCore Optimizer\nKEY=VALUE\n".to_string(),
        description: "Test env".to_string(),
    };

    // Lần 1: Tạo mới thành công
    let ok = apply_optimized_file(root, &opt_file, false).unwrap();
    assert!(ok);

    // User sửa tay file (mất comment Generated by)
    let file_path = root.join(".mgc-optimizer/test.env");
    fs::write(&file_path, "USER_CUSTOM_KEY=CUSTOM_VALUE\n").unwrap();

    // Lần 2 (force=false): Phải từ chối ghi đè để bảo vệ chỉnh sửa của user
    let ok2 = apply_optimized_file(root, &opt_file, false).unwrap();
    assert!(!ok2);
    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "USER_CUSTOM_KEY=CUSTOM_VALUE\n");

    // Lần 3 (force=true): Bắt buộc ghi đè
    let ok3 = apply_optimized_file(root, &opt_file, true).unwrap();
    assert!(ok3);
    let content_after = fs::read_to_string(&file_path).unwrap();
    assert!(content_after.contains("Generated by MagiCore Optimizer"));
}

#[test]
fn test_classify_vendor_known_and_unknown() {
    // Vendor đã biết — chuẩn hóa id
    assert_eq!(HardwareInfo::classify_vendor("Apple M2"), Some("apple"));
    assert_eq!(
        HardwareInfo::classify_vendor("NVIDIA GeForce RTX 4090"),
        Some("nvidia")
    );
    assert_eq!(
        HardwareInfo::classify_vendor("AMD Radeon Pro 5500M"),
        Some("amd")
    );
    assert_eq!(
        HardwareInfo::classify_vendor("Intel Iris Xe Graphics"),
        Some("intel")
    );
    // Không nhận ra — None, không đoán
    assert_eq!(HardwareInfo::classify_vendor("FooBar 9000"), None);
    assert_eq!(HardwareInfo::classify_vendor(""), None);
}

#[test]
fn test_parse_system_profiler_sample() {
    // Sample thật rút gọn từ Mac Apple Silicon (unified memory: không VRAM)
    let sample = "Graphics/Displays:\n\n    Apple M2:\n\n      Chipset Model: Apple M2\n      Type: GPU\n      Bus: Built-In\n      Total Number of Cores: 10\n      Vendor: Apple (0x106b)\n      Metal Support: Metal 4\n";
    let gpus = HardwareInfo::parse_system_profiler(sample);
    assert_eq!(gpus.len(), 1);
    assert_eq!(gpus[0].name, "Apple M2");
    assert_eq!(gpus[0].vendor.as_deref(), Some("apple"));
    assert_eq!(gpus[0].vram_mb, None);
}

#[test]
fn test_parse_system_profiler_empty_and_multi() {
    assert!(HardwareInfo::parse_system_profiler("").is_empty());
    assert!(HardwareInfo::parse_system_profiler("Graphics/Displays:\n").is_empty());
    // Hai card rời — mỗi Chipset Model một entry
    let sample = "      Chipset Model: NVIDIA GeForce RTX 4090\n      Vendor: NVIDIA (0x10de)\n      Chipset Model: Intel UHD Graphics 630\n      Vendor: Intel (0x8086)\n";
    let gpus = HardwareInfo::parse_system_profiler(sample);
    assert_eq!(gpus.len(), 2);
    assert_eq!(gpus[0].name, "NVIDIA GeForce RTX 4090");
    assert_eq!(gpus[0].vendor.as_deref(), Some("nvidia"));
    assert_eq!(gpus[1].name, "Intel UHD Graphics 630");
    assert_eq!(gpus[1].vendor.as_deref(), Some("intel"));
}

#[test]
fn test_parse_nvidia_smi_rows_and_sanity_window() {
    let sample = "NVIDIA GeForce RTX 4090, 24564 MiB\nTesla V100-SXM2-16GB, 16384 MiB\n";
    let gpus = HardwareInfo::parse_nvidia_smi(sample);
    assert_eq!(gpus.len(), 2);
    assert_eq!(gpus[0].vram_mb, Some(24564));
    assert_eq!(gpus[0].vendor.as_deref(), Some("nvidia"));
    // Ngoài sanity window (256 MiB – 256 GiB) — số rác bị bỏ, card giữ lại với vram None
    let weird = "Mystery Card, 999999999 MiB\nTiny Card, 12 MiB\n";
    let gpus = HardwareInfo::parse_nvidia_smi(weird);
    assert_eq!(gpus.len(), 2);
    assert!(gpus.iter().all(|g| g.vram_mb.is_none()));
    assert!(HardwareInfo::parse_nvidia_smi("").is_empty());
    assert!(HardwareInfo::parse_nvidia_smi("no-comma-line\n").is_empty());
}

#[test]
fn test_parse_lspci_display_rows_only() {
    let sample = "00:02.0 \"VGA compatible controller\" \"Intel Corporation\" \"UHD Graphics 620\"\n01:00.0 \"3D controller\" \"NVIDIA Corporation\" \"GP108M [GeForce MX150]\"\n00:1f.3 \"Audio device\" \"Intel Corporation\" \"Sunrise Point-LP HD Audio\"\n";
    let gpus = HardwareInfo::parse_lspci(sample);
    assert_eq!(gpus.len(), 2);
    assert_eq!(gpus[0].vendor.as_deref(), Some("intel"));
    assert_eq!(gpus[1].vendor.as_deref(), Some("nvidia"));
    assert!(gpus.iter().all(|g| g.vram_mb.is_none()));
}

#[test]
fn test_parse_cim_single_array_and_garbage() {
    // Object đơn + AdapterRAM hợp lệ (8 GiB)
    let single = r#"{"Name": "NVIDIA GeForce RTX 4070", "AdapterRAM": 8589934592}"#;
    let gpus = HardwareInfo::parse_cim_videocontroller(single);
    assert_eq!(gpus.len(), 1);
    assert_eq!(gpus[0].vram_mb, Some(8192));
    // AdapterRAM uint32 tràn trên card mới: 0xFFFFFFFF byte ≈ 4095 MiB —
    // giá trị OS báo (có thể understated, nhưng là số thật của OS, vẫn
    // trong sanity window nên giữ; không bịa số khác thay thế).
    let wrapped = r#"{"Name": "Some Card", "AdapterRAM": 4294967295}"#;
    let gpus = HardwareInfo::parse_cim_videocontroller(wrapped);
    assert_eq!(gpus.len(), 1);
    assert_eq!(gpus[0].vram_mb, Some(4095));
    // Mảng + JSON rác
    let arr =
        r#"[{"Name": "Intel Iris Xe", "AdapterRAM": 134217728}, {"Name": "", "AdapterRAM": 0}]"#;
    let gpus = HardwareInfo::parse_cim_videocontroller(arr);
    assert_eq!(gpus.len(), 1);
    assert_eq!(gpus[0].name, "Intel Iris Xe");
    assert!(HardwareInfo::parse_cim_videocontroller("not json").is_empty());
}

#[test]
fn test_gpu_env_file_zero_and_measured() {
    use super::detect::{GpuInfo, HardwareInfo, SystemProfile};
    use super::generators::gpu_env_file;
    // Không GPU — COUNT=0 cũng là số liệu
    let hw = HardwareInfo {
        cpu_cores: 4,
        arch: "x86_64".to_string(),
        os: "linux".to_string(),
        total_memory_gb: Some(8),
        profile: SystemProfile::Standard,
        gpus: vec![],
    };
    let file = gpu_env_file(&hw);
    assert_eq!(file.relative_path, ".mgc-optimizer/gpu.env");
    assert!(file.content.contains("MGC_GPU_COUNT=0"));
    // GPU đo được — fact đầy đủ, VRAM None ghi rõ lý do
    let hw = HardwareInfo {
        gpus: vec![
            GpuInfo {
                name: "Apple M2".to_string(),
                vendor: Some("apple".to_string()),
                vram_mb: None,
            },
            GpuInfo {
                name: "NVIDIA GeForce RTX 4090".to_string(),
                vendor: Some("nvidia".to_string()),
                vram_mb: Some(24564),
            },
        ],
        ..hw
    };
    let file = gpu_env_file(&hw);
    assert!(file.content.contains("MGC_GPU_COUNT=2"));
    assert!(file.content.contains("MGC_GPU_0_NAME=Apple M2"));
    assert!(file.content.contains("MGC_GPU_0_VENDOR=apple"));
    assert!(file.content.contains("unified memory or unknown"));
    assert!(file.content.contains("MGC_GPU_1_VRAM_MB=24564"));
}

#[test]
fn test_meminfo_parser_fixtures_macos_linux_windows() {
    // Fixture parser RAM đa nền tảng (không gọi subprocess/syscall).
    // (Cross-platform RAM parser fixtures — no subprocess.)
    use super::detect::HardwareInfo;
    // macOS sysctl bytes.
    assert_eq!(
        HardwareInfo::parse_sysctl_memsize_bytes("17179869184\n"),
        Some(17179869184)
    );
    assert_eq!(
        HardwareInfo::parse_sysctl_memsize_bytes("not-a-number\n"),
        None
    );
    assert_eq!(HardwareInfo::parse_sysctl_memsize_bytes(""), None);
    // Linux meminfo.
    let meminfo = "MemTotal:       16384000 kB\nMemFree:         8000000 kB\n";
    assert_eq!(HardwareInfo::parse_meminfo_kb(meminfo), Some(16384000));
    assert_eq!(
        HardwareInfo::parse_meminfo_kb("MemTotal:       2097152 kB\n"),
        Some(2097152)
    );
    assert_eq!(HardwareInfo::parse_meminfo_kb("MemFree: 1 kB\n"), None);
    assert_eq!(HardwareInfo::parse_meminfo_kb(""), None);
    assert_eq!(
        HardwareInfo::parse_meminfo_kb("MemTotal: garbage kB\n"),
        None
    );
    // Windows/wmic path has no parser (detection returns None there) —
    // profile_for degrades without RAM on every OS identically.
    // (Windows không parser — profile_for hạ cấp giống mọi OS.)
    assert_eq!(
        HardwareInfo::profile_for(16, None),
        SystemProfile::Constrained
    );
    assert_eq!(
        HardwareInfo::profile_for(8, Some(16)),
        SystemProfile::HighPerformance
    );
    assert_eq!(
        HardwareInfo::profile_for(4, Some(8)),
        SystemProfile::Standard
    );
    assert_eq!(
        HardwareInfo::profile_for(2, Some(4)),
        SystemProfile::Constrained
    );
}

#[test]
fn test_unknown_ram_omits_memory_knobs_per_adapter() {
    // Từng adapter bỏ knob dẫn xuất từ RAM khi Unknown (không số bịa).
    // (Each adapter omits its memory knob when RAM is Unknown.)
    use super::adapters::OptimizerAdapter;
    use super::adapters::{flutter::FlutterAdapter, pytorch::PyTorchAdapter};
    let hw = HardwareInfo {
        cpu_cores: 8,
        arch: "x86_64".to_string(),
        os: "linux".to_string(),
        total_memory_gb: None,
        profile: SystemProfile::Constrained,
        gpus: vec![],
    };
    let known = HardwareInfo {
        total_memory_gb: Some(16),
        ..hw.clone()
    };
    let flutter_unknown = FlutterAdapter.generate(&hw);
    let flutter_known = FlutterAdapter.generate(&known);
    assert!(
        !flutter_unknown
            .iter()
            .any(|file| file.content.contains("old_gen_heap_size")),
        "unknown RAM must not emit a heap size"
    );
    assert!(
        flutter_known
            .iter()
            .any(|file| file.content.contains("old_gen_heap_size=4096")),
        "known 16GB RAM must emit the derived heap"
    );
    let pytorch_unknown = PyTorchAdapter.generate(&hw);
    let pytorch_known = PyTorchAdapter.generate(&known);
    assert!(
        !pytorch_unknown
            .iter()
            .any(|file| file.content.contains("CONTAINER_MEMORY")),
        "unknown RAM must not emit a container memory limit"
    );
    assert!(
        pytorch_known
            .iter()
            .any(|file| file.content.contains("CONTAINER_MEMORY=14g")),
        "known 16GB RAM must emit the derived limit"
    );
}
