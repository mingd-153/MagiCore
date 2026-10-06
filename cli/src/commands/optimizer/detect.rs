//! `optimizer/detect.rs` — Native OS hardware detection (CPU/RAM/OS/GPU).
//! Reads kernel interfaces directly; it never spawns a shell or vendor CLI.
//! Unsupported GPU probes stay empty rather than claiming a delegated scan.
//! (Đọc giao diện kernel trực tiếp, không spawn shell/tool của vendor;
//! GPU chưa có probe native thì giữ rỗng, không nhận vơ.)

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Thông tin phần cứng được phát hiện
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HardwareInfo {
    /// Measured logical CPU cores available to this process; `None` means
    /// the OS query failed. Never synthesize a count for tuning.
    /// (Số logical core đo được; `None` nếu truy vấn OS thất bại.)
    #[serde(default)]
    pub cpu_cores: Option<usize>,
    /// Kiến trúc hệ điều hành (x86_64, aarch64, ...)
    pub arch: String,
    /// Hệ điều hành (macos, linux, windows)
    pub os: String,
    /// Dung lượng RAM (tính bằng GiB) — None nghĩa là KHÔNG BIẾT
    /// (đọc thất bại). Không bao giờ là số đo giả: mọi tuning dẫn xuất
    /// từ RAM phải bỏ qua khi là None (xem generators).
    /// RAM size in GiB — None means UNKNOWN (detection failed), never a
    /// fabricated measurement; memory-derived tuning must be skipped
    /// when None (see generators).
    /// (None nghĩa là KHÔNG BIẾT — không bao giờ là số đo thật.)
    pub total_memory_gb: Option<usize>,
    /// Profile nhận diện (Desktop, Laptop, Server/Container)
    pub profile: SystemProfile,
    /// GPUs detected on this machine — empty when none found OR when
    /// detection is unavailable; consult `gpu_detection_status` before
    /// treating an empty list as a measured zero.
    /// (GPU phát hiện được; phải xem trạng thái probe trước khi hiểu rỗng là 0.)
    pub gpus: Vec<GpuInfo>,
    /// Whether the native OS probe completed, was partial, or is unsupported.
    /// (Trạng thái probe GPU native: đủ, một phần, hoặc chưa hỗ trợ.)
    #[serde(default)]
    pub gpu_detection_status: GpuDetectionStatus,
}

/// Completeness of the native GPU inventory; missing probe support is not zero GPUs.
/// (Độ đầy đủ của inventory GPU; thiếu probe không đồng nghĩa máy không có GPU.)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum GpuDetectionStatus {
    Available,
    Partial,
    #[default]
    Unavailable,
}

/// One GPU as reported by the OS — name always present, everything else
/// optional (a missing VRAM/vendor is unknown, not zero/empty-string).
/// (Một GPU theo báo cáo của OS — chỉ name bắt buộc.)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GpuInfo {
    /// OS-reported model string (e.g. "Apple M2", "NVIDIA GeForce RTX 4090").
    pub name: String,
    /// Normalized vendor ("apple" | "nvidia" | "amd" | "intel") or None.
    pub vendor: Option<String>,
    /// Dedicated VRAM in MiB — None when unknown or unified memory.
    pub vram_mb: Option<usize>,
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
        let cpu_cores = std::thread::available_parallelism().ok().map(|n| n.get());
        let arch = std::env::consts::ARCH.to_string();
        let os = std::env::consts::OS.to_string();

        // RAM detection failure stays unknown (None) — never patched
        // with a fabricated number; the profile degrades to Constrained
        // and memory-derived tuning is skipped downstream. Writing tuned
        // values from a guessed size would be false measurement.
        // (Đọc RAM thất bại giữ None — không vá số bịa.)
        let memory_gb = Self::detect_memory_gb();
        let total_memory_gb = memory_gb;
        let profile = Self::profile_for(cpu_cores, memory_gb);
        let (gpus, gpu_detection_status) = Self::detect_gpus();

        Self {
            cpu_cores,
            arch,
            os,
            total_memory_gb,
            profile,
            gpus,
            gpu_detection_status,
        }
    }

    /// Pure profile selection — unknown CPU or RAM always degrades to
    /// Constrained; no profile is derived from a guessed resource count.
    /// (CPU hoặc RAM chưa biết thì hạ về Constrained, không đoán tài nguyên.)
    pub fn profile_for(cpu_cores: Option<usize>, memory_gb: Option<usize>) -> SystemProfile {
        let (Some(cpu_cores), Some(total_memory_gb)) = (cpu_cores, memory_gb) else {
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
            let bytes = Self::macos_total_memory_bytes()?;
            usize::try_from(bytes / (1024 * 1024 * 1024)).ok()
        }
        #[cfg(target_os = "linux")]
        {
            let host_bytes = std::fs::read_to_string("/proc/meminfo")
                .ok()
                .and_then(|meminfo| Self::parse_meminfo_kb(&meminfo))
                .and_then(|kb| kb.checked_mul(1024));
            let cgroup_limit = Self::linux_cgroup_memory_limit_bytes();
            let effective_bytes = match (host_bytes, cgroup_limit) {
                (Some(host), Some(limit)) => host.min(limit),
                (Some(host), None) => host,
                (None, Some(limit)) => limit,
                (None, None) => return None,
            };
            let gb = effective_bytes / (1024 * 1024 * 1024);
            (gb > 0).then(|| usize::try_from(gb).ok()).flatten()
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            None
        }
    }

    /// Read physical RAM through macOS sysctl directly, without spawning `sysctl`.
    /// (Đọc RAM qua API sysctl của kernel macOS, không chạy executable `sysctl`.)
    #[cfg(target_os = "macos")]
    #[allow(unsafe_code)]
    fn macos_total_memory_bytes() -> Option<u64> {
        let mut value = 0_u64;
        let mut value_size = std::mem::size_of::<u64>();
        // SAFETY: the NUL-terminated MIB name and writable u64 buffer remain
        // valid for the duration of the syscall; `value_size` describes it.
        // (An toàn: tên MIB kết thúc NUL, buffer u64 ghi được còn sống suốt syscall.)
        let result = unsafe {
            libc::sysctlbyname(
                c"hw.memsize".as_ptr(),
                (&mut value as *mut u64).cast(),
                &mut value_size,
                std::ptr::null_mut(),
                0,
            )
        };
        (result == 0 && value_size == std::mem::size_of::<u64>() && value > 0).then_some(value)
    }

    /// Prefer the kernel cgroup limit when present, otherwise use host RAM.
    /// (Ưu tiên giới hạn cgroup của kernel; nếu không có thì dùng RAM host.)
    #[cfg(target_os = "linux")]
    fn linux_cgroup_memory_limit_bytes() -> Option<u64> {
        let memberships = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
        Self::linux_cgroup_memory_limit_bytes_at(Path::new("/sys/fs/cgroup"), &memberships)
    }

    /// Read every finite process-to-root limit and use the minimum: a child
    /// cgroup may say unlimited while one of its parents is capped.
    /// (Đọc mọi giới hạn từ cgroup process tới root và lấy min; cgroup cha có
    /// thể bị giới hạn dù cgroup con ghi `max`.)
    #[cfg(any(target_os = "linux", test))]
    pub(crate) fn linux_cgroup_memory_limit_bytes_at(
        root: &Path,
        memberships: &str,
    ) -> Option<u64> {
        let mut paths = Vec::new();
        for line in memberships.lines() {
            let mut fields = line.splitn(3, ':');
            let Some(hierarchy) = fields.next() else {
                continue;
            };
            let Some(controllers) = fields.next() else {
                continue;
            };
            let Some(group) = fields.next() else {
                continue;
            };
            let Ok(relative) = Path::new(group).strip_prefix("/") else {
                continue;
            };
            if relative
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
            {
                continue;
            }
            if hierarchy == "0" && controllers.is_empty() {
                for ancestor in relative.ancestors() {
                    paths.push(root.join(ancestor).join("memory.max"));
                }
            } else if controllers
                .split(',')
                .any(|controller| controller == "memory")
            {
                for ancestor in relative.ancestors() {
                    paths.push(
                        root.join("memory")
                            .join(ancestor)
                            .join("memory.limit_in_bytes"),
                    );
                }
            }
        }
        paths.sort();
        paths.dedup();
        paths
            .into_iter()
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .filter_map(|text| Self::parse_cgroup_memory_limit_bytes(&text))
            .min()
    }

    /// Parse cgroup v1/v2 memory limit; `max` and huge v1 sentinels mean unlimited.
    /// (Đọc giới hạn cgroup v1/v2; `max` và sentinel lớn nghĩa là không giới hạn.)
    #[cfg(any(target_os = "linux", test))]
    pub(crate) fn parse_cgroup_memory_limit_bytes(text: &str) -> Option<u64> {
        let value = text.trim().parse::<u64>().ok()?;
        (value > 0 && value < (1_u64 << 60)).then_some(value)
    }

    /// Parse a decimal byte count from the macOS sysctl value — fixture-tested.
    /// (Parse số byte thập phân từ sysctl macOS — có fixture test.)
    #[cfg(test)]
    pub(crate) fn parse_sysctl_memsize_bytes(text: &str) -> Option<u64> {
        text.trim().parse().ok()
    }

    /// Parse Linux `/proc/meminfo` MemTotal line (kB) — pure,
    /// fixture-tested on every CI OS (the live caller is Linux-only).
    /// (Parse MemTotal meminfo — thuần, test bằng fixture mọi OS.)
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) fn parse_meminfo_kb(text: &str) -> Option<u64> {
        for line in text.lines() {
            if line.starts_with("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let kb: u64 = parts[1].parse().ok()?;
                    return Some(kb);
                }
            }
        }
        None
    }

    /// Detect GPUs only through a native, in-process OS interface.
    /// Linux PCI sysfs is supported; macOS and Windows remain unknown until
    /// native IOKit/DXGI implementations are added. No utility process is run.
    /// (Hiện chỉ Linux có probe PCI sysfs; macOS/Windows giữ unknown tới khi
    /// có IOKit/DXGI native. Không chạy tiến trình tiện ích.)
    fn detect_gpus() -> (Vec<GpuInfo>, GpuDetectionStatus) {
        match std::env::consts::OS {
            "linux" => Self::detect_gpus_linux(),
            _ => (Vec::new(), GpuDetectionStatus::Unavailable),
        }
    }

    /// Linux: enumerate PCI display controllers from kernel sysfs metadata.
    /// (Linux: liệt kê display controller qua metadata sysfs của kernel.)
    fn detect_gpus_linux() -> (Vec<GpuInfo>, GpuDetectionStatus) {
        Self::parse_linux_sysfs_gpus(Path::new("/sys/bus/pci/devices"))
    }

    /// Parse kernel PCI sysfs entries; caller supplies root for hermetic tests.
    /// (Đọc entry PCI sysfs; nhận root để test fixture không phụ thuộc máy.)
    pub(crate) fn parse_linux_sysfs_gpus(root: &Path) -> (Vec<GpuInfo>, GpuDetectionStatus) {
        let Ok(entries) = std::fs::read_dir(root) else {
            return (Vec::new(), GpuDetectionStatus::Unavailable);
        };
        let mut paths = Vec::new();
        let mut status = GpuDetectionStatus::Available;
        for entry in entries {
            match entry {
                Ok(entry) => paths.push(entry.path()),
                Err(_) => status = GpuDetectionStatus::Partial,
            }
        }
        paths.sort();
        let mut gpus = Vec::new();
        for device_path in paths {
            let class = match std::fs::read_to_string(device_path.join("class")) {
                Ok(class) => class,
                Err(_) => {
                    status = GpuDetectionStatus::Partial;
                    continue;
                }
            };
            let Ok(class) = u32::from_str_radix(class.trim().trim_start_matches("0x"), 16) else {
                status = GpuDetectionStatus::Partial;
                continue;
            };
            if class >> 16 != 0x03 {
                continue;
            }
            let Some(vendor_id) = read_pci_id(&device_path.join("vendor")) else {
                status = GpuDetectionStatus::Partial;
                continue;
            };
            let Some(device_id) = read_pci_id(&device_path.join("device")) else {
                status = GpuDetectionStatus::Partial;
                continue;
            };
            let vendor_name = match vendor_id {
                0x10de => Some("nvidia"),
                0x1002 => Some("amd"),
                0x8086 => Some("intel"),
                0x106b => Some("apple"),
                _ => None,
            };
            let product_name = std::fs::read_to_string(device_path.join("product_name"))
                .or_else(|_| std::fs::read_to_string(device_path.join("label")))
                .ok()
                .map(|name| name.trim().to_string())
                .filter(|name| {
                    !name.is_empty() && name.len() <= 256 && !name.chars().any(char::is_control)
                });
            let name = product_name.unwrap_or_else(|| match vendor_name {
                Some("nvidia") => format!("NVIDIA GPU (PCI {vendor_id:04x}:{device_id:04x})"),
                Some("amd") => format!("AMD GPU (PCI {vendor_id:04x}:{device_id:04x})"),
                Some("intel") => format!("Intel GPU (PCI {vendor_id:04x}:{device_id:04x})"),
                Some("apple") => format!("Apple GPU (PCI {vendor_id:04x}:{device_id:04x})"),
                _ => format!("PCI display adapter ({vendor_id:04x}:{device_id:04x})"),
            });
            gpus.push(GpuInfo {
                name,
                vendor: vendor_name.map(str::to_string),
                vram_mb: None,
            });
        }
        (gpus, status)
    }

    /// Return vendor presence only for a complete inventory; otherwise unknown.
    /// (`Some(false)` chỉ hợp lệ khi probe đủ; thiếu/partial phải là `None`.)
    pub fn has_gpu_vendor(&self, vendor: &str) -> Option<bool> {
        if self.gpu_detection_status != GpuDetectionStatus::Available {
            return None;
        }
        Some(
            self.gpus
                .iter()
                .any(|gpu| gpu.vendor.as_deref() == Some(vendor)),
        )
    }
}

fn read_pci_id(path: &Path) -> Option<u16> {
    let value = std::fs::read_to_string(path).ok()?;
    u16::from_str_radix(value.trim().trim_start_matches("0x"), 16).ok()
}
