//! Unit tests for MagiCore Optimizer Engine

use super::detect::*;
use super::generators::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_hardware_detect_returns_valid_info() {
    // Environment-independent shape assertions: arch/os are known; CPU and
    // RAM may be Unknown where the OS does not expose them. Unknown resources
    // must not be converted into fabricated tuning inputs.
    // (CPU/RAM có thể Unknown; không biến giá trị thiếu thành số đo giả.)
    let hw = HardwareInfo::detect();
    assert!(hw.cpu_cores.is_some_and(|cores| cores > 0));
    assert!(!hw.arch.is_empty());
    assert!(!hw.os.is_empty());
    match hw.total_memory_gb {
        Some(gb) => assert!(gb > 0, "detected RAM must be positive, got {gb}"),
        None => assert_eq!(hw.profile, SystemProfile::Constrained),
    }
}

#[test]
fn test_generate_optimizations_for_web_core() {
    let dir = tempdir().unwrap();
    let project_root = dir.path();

    // Create package.json to trigger Node.js detection
    fs::write(project_root.join("package.json"), "{}").unwrap();

    let hw = HardwareInfo {
        cpu_cores: Some(8),
        arch: "aarch64".to_string(),
        os: "macos".to_string(),
        total_memory_gb: Some(16),
        profile: SystemProfile::HighPerformance,
        gpus: vec![],
        gpu_detection_status: GpuDetectionStatus::Available,
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
        cpu_cores: Some(12),
        arch: "x86_64".to_string(),
        os: "linux".to_string(),
        total_memory_gb: Some(32),
        profile: SystemProfile::HighPerformance,
        gpus: vec![],
        gpu_detection_status: GpuDetectionStatus::Available,
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
        HardwareInfo::profile_for(Some(64), None),
        SystemProfile::Constrained
    );
    assert_eq!(
        HardwareInfo::profile_for(Some(8), Some(16)),
        SystemProfile::HighPerformance
    );
    assert_eq!(
        HardwareInfo::profile_for(Some(4), Some(8)),
        SystemProfile::Standard
    );
    assert_eq!(
        HardwareInfo::profile_for(Some(2), Some(4)),
        SystemProfile::Constrained
    );
}

#[test]
fn test_profile_for_unknown_cpu_degrades_to_constrained() {
    assert_eq!(
        HardwareInfo::profile_for(None, Some(128)),
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
        cpu_cores: Some(8),
        arch: "x86_64".to_string(),
        os: "linux".to_string(),
        total_memory_gb: None,
        profile: SystemProfile::Constrained,
        gpus: vec![],
        gpu_detection_status: GpuDetectionStatus::Available,
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
fn test_linux_sysfs_gpu_probe_uses_native_pci_metadata() {
    use std::path::Path;

    let dir = tempdir().unwrap();
    let devices = dir.path().join("bus/pci/devices");
    let nvidia = devices.join("0000:01:00.0");
    let intel = devices.join("0000:00:02.0");
    let network = devices.join("0000:00:1f.6");
    for path in [&nvidia, &intel, &network] {
        fs::create_dir_all(path).unwrap();
    }
    fs::write(nvidia.join("class"), "0x030000\n").unwrap();
    fs::write(nvidia.join("vendor"), "0x10de\n").unwrap();
    fs::write(nvidia.join("device"), "0x2684\n").unwrap();
    fs::write(nvidia.join("product_name"), "NVIDIA GeForce RTX 4090\n").unwrap();
    fs::write(intel.join("class"), "0x030000\n").unwrap();
    fs::write(intel.join("vendor"), "0x8086\n").unwrap();
    fs::write(intel.join("device"), "0x9a49\n").unwrap();
    fs::write(network.join("class"), "0x020000\n").unwrap();
    fs::write(network.join("vendor"), "0x8086\n").unwrap();
    fs::write(network.join("device"), "0x1234\n").unwrap();

    let (detected, status) = HardwareInfo::parse_linux_sysfs_gpus(Path::new(&devices));
    assert_eq!(status, GpuDetectionStatus::Available);
    assert_eq!(
        detected.len(),
        2,
        "non-display PCI devices must be excluded"
    );
    assert!(detected.iter().any(|gpu| {
        gpu.name == "NVIDIA GeForce RTX 4090" && gpu.vendor.as_deref() == Some("nvidia")
    }));
    assert!(detected.iter().any(|gpu| {
        gpu.name == "Intel GPU (PCI 8086:9a49)" && gpu.vendor.as_deref() == Some("intel")
    }));
    assert!(detected.iter().all(|gpu| gpu.vram_mb.is_none()));
}

#[test]
fn test_empty_readable_pci_inventory_is_a_measured_zero() {
    use std::path::Path;

    let dir = tempdir().unwrap();
    let (gpus, status) = HardwareInfo::parse_linux_sysfs_gpus(Path::new(dir.path()));
    assert!(gpus.is_empty());
    assert_eq!(status, GpuDetectionStatus::Available);
}

#[test]
fn test_unreadable_pci_inventory_is_not_reported_as_zero() {
    use std::path::Path;

    let dir = tempdir().unwrap();
    let missing_root = dir.path().join("missing");
    let (gpus, status) = HardwareInfo::parse_linux_sysfs_gpus(Path::new(&missing_root));
    assert!(gpus.is_empty());
    assert_eq!(status, GpuDetectionStatus::Unavailable);
}

#[test]
fn test_incomplete_pci_inventory_does_not_emit_a_zero_count() {
    use std::path::Path;

    let dir = tempdir().unwrap();
    let device = dir.path().join("0000:00:02.0");
    fs::create_dir_all(&device).unwrap();
    fs::write(device.join("class"), "0x030000\n").unwrap();
    fs::write(device.join("vendor"), "malformed\n").unwrap();
    fs::write(device.join("device"), "0x9a49\n").unwrap();

    let (gpus, status) = HardwareInfo::parse_linux_sysfs_gpus(Path::new(dir.path()));
    assert!(gpus.is_empty());
    assert_eq!(status, GpuDetectionStatus::Partial);
    let hw = HardwareInfo {
        cpu_cores: Some(4),
        arch: "x86_64".to_string(),
        os: "linux".to_string(),
        total_memory_gb: Some(8),
        profile: SystemProfile::Standard,
        gpus,
        gpu_detection_status: status,
    };
    assert!(!gpu_env_file(&hw).content.contains("MGC_GPU_COUNT="));
}

#[test]
fn test_gpu_env_file_zero_and_measured() {
    use super::detect::{GpuDetectionStatus, GpuInfo, HardwareInfo, SystemProfile};
    use super::generators::gpu_env_file;
    // Complete inventory with no GPUs is a measured zero.
    let hw = HardwareInfo {
        cpu_cores: Some(4),
        arch: "x86_64".to_string(),
        os: "linux".to_string(),
        total_memory_gb: Some(8),
        profile: SystemProfile::Standard,
        gpus: vec![],
        gpu_detection_status: GpuDetectionStatus::Available,
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
fn test_unknown_gpu_probe_does_not_emit_a_fake_zero_count() {
    use super::detect::{GpuDetectionStatus, HardwareInfo, SystemProfile};
    use super::generators::gpu_env_file;

    let hw = HardwareInfo {
        cpu_cores: Some(4),
        arch: "aarch64".to_string(),
        os: "macos".to_string(),
        total_memory_gb: Some(16),
        profile: SystemProfile::Standard,
        gpus: vec![],
        gpu_detection_status: GpuDetectionStatus::Unavailable,
    };
    let output = gpu_env_file(&hw);

    assert!(!output.content.contains("MGC_GPU_COUNT="));
    assert!(output.content.contains("GPU detection unavailable"));
}

#[test]
fn test_gpu_env_sanitizes_control_characters_without_key_injection() {
    let hw = HardwareInfo {
        cpu_cores: Some(4),
        arch: "x86_64".to_string(),
        os: "linux".to_string(),
        total_memory_gb: Some(8),
        profile: SystemProfile::Standard,
        gpus: vec![GpuInfo {
            name: "Display\nMGC_GPU_COUNT=999".to_string(),
            vendor: Some("nvidia\r\nUNEXPECTED=1".to_string()),
            vram_mb: None,
        }],
        gpu_detection_status: GpuDetectionStatus::Available,
    };

    let output = gpu_env_file(&hw);
    assert!(output.content.contains("MGC_GPU_COUNT=1\n"));
    assert!(
        output
            .content
            .contains("MGC_GPU_0_NAME=Display MGC_GPU_COUNT=999\n")
    );
    assert!(
        output
            .content
            .contains("MGC_GPU_0_VENDOR=nvidia  UNEXPECTED=1\n")
    );
    assert!(!output.content.contains("\nUNEXPECTED=1\n"));
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
    assert_eq!(
        HardwareInfo::parse_cgroup_memory_limit_bytes("8589934592\n"),
        Some(8589934592)
    );
    assert_eq!(HardwareInfo::parse_cgroup_memory_limit_bytes("max\n"), None);
    assert_eq!(HardwareInfo::parse_cgroup_memory_limit_bytes("0\n"), None);
    assert_eq!(
        HardwareInfo::parse_cgroup_memory_limit_bytes("9223372036854771712\n"),
        None
    );
    // Unknown platform RAM degrades without inventing a measurement.
    // (RAM không đọc được thì hạ cấp, không tự tạo số đo.)
    assert_eq!(
        HardwareInfo::profile_for(Some(16), None),
        SystemProfile::Constrained
    );
    assert_eq!(
        HardwareInfo::profile_for(Some(8), Some(16)),
        SystemProfile::HighPerformance
    );
    assert_eq!(
        HardwareInfo::profile_for(Some(4), Some(8)),
        SystemProfile::Standard
    );
    assert_eq!(
        HardwareInfo::profile_for(Some(2), Some(4)),
        SystemProfile::Constrained
    );
}

#[test]
fn test_linux_cgroup_memory_limit_reads_nested_process_membership() {
    use std::path::Path;

    let dir = tempdir().unwrap();
    let cgroup_root = dir.path().join("sys/fs/cgroup");
    let nested = cgroup_root.join("system.slice/mgc.scope");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("memory.max"), "4294967296\n").unwrap();

    assert_eq!(
        HardwareInfo::linux_cgroup_memory_limit_bytes_at(
            Path::new(&cgroup_root),
            "0::/system.slice/mgc.scope\n"
        ),
        Some(4294967296)
    );
    assert_eq!(
        HardwareInfo::linux_cgroup_memory_limit_bytes_at(
            Path::new(&cgroup_root),
            "0::/../../outside\n"
        ),
        None
    );
}

#[test]
fn test_linux_cgroup_memory_limit_uses_effective_ancestor_limit() {
    use std::path::Path;

    let dir = tempdir().unwrap();
    let cgroup_root = dir.path().join("sys/fs/cgroup");
    let parent = cgroup_root.join("tenant");
    let nested = parent.join("worker");
    fs::create_dir_all(&nested).unwrap();
    fs::write(parent.join("memory.max"), "8589934592\n").unwrap();
    fs::write(nested.join("memory.max"), "max\n").unwrap();
    fs::write(cgroup_root.join("memory.max"), "17179869184\n").unwrap();

    assert_eq!(
        HardwareInfo::linux_cgroup_memory_limit_bytes_at(
            Path::new(&cgroup_root),
            "0::/tenant/worker\n"
        ),
        Some(8589934592)
    );
}

#[test]
fn test_unknown_ram_omits_memory_knobs_per_adapter() {
    // Từng adapter bỏ knob dẫn xuất từ RAM khi Unknown (không số bịa).
    // (Each adapter omits its memory knob when RAM is Unknown.)
    use super::adapters::OptimizerAdapter;
    use super::adapters::{flutter::FlutterAdapter, pytorch::PyTorchAdapter};
    let hw = HardwareInfo {
        cpu_cores: Some(8),
        arch: "x86_64".to_string(),
        os: "linux".to_string(),
        total_memory_gb: None,
        profile: SystemProfile::Constrained,
        gpus: vec![],
        gpu_detection_status: GpuDetectionStatus::Available,
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
