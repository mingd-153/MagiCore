//! `optimizer/detect.rs` — Hardware detection (CPU/RAM/OS/arch).
//! LIMITATION: GPU detection not yet implemented (v1.1.0-rc.1)
//! HẠN CHẾ: Chưa phát hiện GPU (v1.1.0-rc.1)

use serde::{Deserialize, Serialize};

/// Thông tin phần cứng được phát hiện
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HardwareInfo {
    /// Số lượng logical CPU cores
    pub cpu_cores: usize,
    /// Kiến trúc hệ điều hành (x86_64, aarch64, ...)
    pub arch: String,
    /// Hệ điều hành (macos, linux, windows)
    pub os: String,
    /// Dung lượng RAM ước tính (tính bằng GiB).
    /// RAM size estimate in GiB. `0` means UNKNOWN (detection failed) —
    /// never a measured value; memory-derived tuning must be skipped
    /// when this is 0 (see generators).
    /// (0 nghĩa là KHÔNG BIẾT (đọc thất bại) — không bao giờ là số đo
    /// thật; tuning dẫn xuất từ RAM phải bỏ qua khi bằng 0.)
    pub total_memory_gb: usize,
    /// Profile nhận diện (Desktop, Laptop, Server/Container)
    pub profile: SystemProfile,
}

/// Phân loại Profile thiết bị
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SystemProfile {
    /// Máy trạm/máy bàn hiệu năng cao (>= 8 cores, >= 16GB RAM)
    HighPerformance,
    /// Laptop/thiết bị di động tiêu chuẩn (4-8 cores, 8-16GB RAM)
    Standard,
    /// Máy cấu hình thấp hoặc Container giới hạn (< 4 cores, < 8GB RAM)
    Constrained,
}

impl HardwareInfo {
    /// Tự động phát hiện thông số phần cứng từ môi trường runtime
    pub fn detect() -> Self {
        let cpu_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let arch = std::env::consts::ARCH.to_string();
        let os = std::env::consts::OS.to_string();

        // RAM detection failure is NOT patched with a fabricated number:
        // unknown stays 0 (sentinel) and the profile degrades to
        // Constrained — writing tuned values from a guessed 16 GB would
        // be a false measurement (V1.2: no fabricated evidence).
        // (Đọc RAM thất bại KHÔNG vá bằng số bịa: unknown giữ 0 (sentinel)
        // và profile hạ về Constrained — ghi giá trị tuning từ 16 GB đoán
        // mò sẽ là bằng chứng giả.)
        let memory_gb = Self::detect_memory_gb();
        let total_memory_gb = memory_gb.unwrap_or(0);

        let profile = Self::profile_for(cpu_cores, memory_gb);

        Self {
            cpu_cores,
            arch,
            os,
            total_memory_gb,
            profile,
        }
    }

    /// Pure profile selection — unknown RAM always degrades to Constrained
    /// (fail-safe: never tune from a guessed size).
    /// (Chọn profile thuần — RAM unknown luôn hạ về Constrained (an toàn
    /// fail-safe: không bao giờ tuning từ số đoán).)
    pub fn profile_for(cpu_cores: usize, memory_gb: Option<usize>) -> SystemProfile {
        let Some(total_memory_gb) = memory_gb else {
            return SystemProfile::Constrained;
        };
        if cpu_cores >= 8 && total_memory_gb >= 16 {
            SystemProfile::HighPerformance
        } else if cpu_cores >= 4 && total_memory_gb >= 8 {
            SystemProfile::Standard
        } else {
            SystemProfile::Constrained
        }
    }

    /// Đọc dung lượng RAM từ hệ điều hành
    fn detect_memory_gb() -> Option<usize> {
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("sysctl")
                .arg("-n")
                .arg("hw.memsize")
                .output()
                .ok()?;
            let bytes_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let bytes: u64 = bytes_str.parse().ok()?;
            Some((bytes / (1024 * 1024 * 1024)) as usize)
        }
        #[cfg(target_os = "linux")]
        {
            let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
            for line in meminfo.lines() {
                if line.starts_with("MemTotal:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let kb: u64 = parts[1].parse().ok()?;
                        return Some((kb / (1024 * 1024)) as usize);
                    }
                }
            }
            None
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            None
        }
    }
}
