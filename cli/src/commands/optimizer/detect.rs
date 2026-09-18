//! `optimizer/detect.rs` — Hardware detection (CPU/RAM/OS/arch/GPU).
//! GPU: best-effort per-OS probing (macOS system_profiler, Linux
//! nvidia-smi/lspci, Windows CIM/WMI). Unknown is `None`/empty — never
//! fabricated. Unified-memory GPUs (Apple Silicon) report NO VRAM number:
//! claiming half of system RAM as VRAM would be a fabricated measurement.
//! (GPU: dò theo từng OS, unknown là None/rỗng — không bao giờ bịa. GPU
//! unified-memory không báo VRAM — đoán VRAM từ RAM là bịa số liệu.)

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
    /// GPUs detected on this machine — empty when none found OR when
    /// detection is unavailable (headless/container). Empty is honest:
    /// it means "no GPU claimed", never "no GPU exists".
    /// (GPU phát hiện được — rỗng nghĩa là "không claim GPU nào".)
    pub gpus: Vec<GpuInfo>,
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
            gpus: Self::detect_gpus(),
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

    /// Best-effort GPU detection for this OS — returns every GPU the OS
    /// reports, or empty when none/unavailable. Never fabricates.
    /// Runtime dispatch (not `#[cfg]`): every probe compiles on every
    /// platform so unit tests cover all parsers on all CI runners; a
    /// probe for another OS simply fails closed to empty at runtime.
    /// (Dò GPU theo OS lúc chạy — mọi parser biên dịch mọi nơi để test
    /// được trên mọi runner CI.)
    pub fn detect_gpus() -> Vec<GpuInfo> {
        match std::env::consts::OS {
            "macos" => Self::detect_gpus_macos(),
            "linux" => Self::detect_gpus_linux(),
            "windows" => Self::detect_gpus_windows(),
            _ => Vec::new(),
        }
    }

    /// macOS: `system_profiler SPDisplaysDataType` lists one card block
    /// per GPU (`Chipset Model:` + `Vendor:`). Unified-memory Apple
    /// Silicon reports no VRAM figure — vram stays None (honest).
    /// (macOS: đọc system_profiler; Apple Silicon không có số VRAM.)
    fn detect_gpus_macos() -> Vec<GpuInfo> {
        let text = match std::process::Command::new("system_profiler")
            .arg("SPDisplaysDataType")
            .output()
        {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).to_string()
            }
            _ => return Vec::new(),
        };
        Self::parse_system_profiler(&text)
    }

    /// Parse `system_profiler SPDisplaysDataType` text into GPUs (pure —
    /// unit-tested with a captured sample, no subprocess in tests).
    /// (Parse text system_profiler thành GPU — hàm thuần để test được.)
    pub(crate) fn parse_system_profiler(text: &str) -> Vec<GpuInfo> {
        let mut gpus = Vec::new();
        let mut name: Option<String> = None;
        let mut vendor_raw: Option<String> = None;
        let flush =
            |name: &mut Option<String>, vendor_raw: &mut Option<String>, out: &mut Vec<GpuInfo>| {
                if let Some(n) = name.take() {
                    let vendor = vendor_raw
                        .take()
                        .map(|v| v.split('(').next().unwrap_or(&v).trim().to_string())
                        .filter(|v| !v.is_empty());
                    let vendor = vendor
                        .as_deref()
                        .and_then(Self::classify_vendor)
                        .map(str::to_string)
                        .or(vendor);
                    out.push(GpuInfo {
                        name: n,
                        vendor,
                        // Unified memory has no dedicated VRAM figure.
                        vram_mb: None,
                    });
                }
            };
        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("Chipset Model:") {
                flush(&mut name, &mut vendor_raw, &mut gpus);
                let model = rest.trim().to_string();
                if !model.is_empty() {
                    name = Some(model);
                }
            } else if let Some(rest) = trimmed.strip_prefix("Vendor:") {
                vendor_raw = Some(rest.trim().to_string());
            }
        }
        flush(&mut name, &mut vendor_raw, &mut gpus);
        gpus
    }

    /// Linux: NVIDIA via `nvidia-smi` first (name + MiB VRAM), then PCI
    /// display controllers via `lspci` (name only, no VRAM claim).
    /// (Linux: nvidia-smi trước, lspci sau — lspci không claim VRAM.)
    fn detect_gpus_linux() -> Vec<GpuInfo> {
        // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
        if let Ok(output) = std::process::Command::new("nvidia-smi")
            .args([
                "--query-gpu=name,memory.total",
                "--format=csv,noheader,nounits",
            ])
            .output()
            && output.status.success()
        {
            let gpus = Self::parse_nvidia_smi(&String::from_utf8_lossy(&output.stdout));
            if !gpus.is_empty() {
                return gpus;
            }
        }
        if let Ok(output) = std::process::Command::new("lspci").arg("-mm").output()
            && output.status.success()
        {
            return Self::parse_lspci(&String::from_utf8_lossy(&output.stdout));
        }
        Vec::new()
    }

    /// Parse `nvidia-smi --format=csv,noheader[,nounits]` rows
    /// (`<name>, <MiB>[ MiB]`) — leading-token parse tolerates both the
    /// nounits and the units spelling. Pure, unit-tested.
    /// (Parse dòng nvidia-smi — chịu được cả hai cách viết đơn vị.)
    pub(crate) fn parse_nvidia_smi(text: &str) -> Vec<GpuInfo> {
        text.lines()
            .filter_map(|line| {
                let (name, vram) = line.split_once(',')?;
                let name = name.trim().to_string();
                if name.is_empty() {
                    return None;
                }
                let vram_mb = vram
                    .split_whitespace()
                    .next()
                    .and_then(|tok| tok.parse::<usize>().ok())
                    .filter(|mb| {
                        // Sanity window: 256 MiB – 256 GiB. Outside it the
                        // number is a parse artifact, not VRAM (fail-closed).
                        // (Cửa sổ sanity: ngoài khoảng là số rác, bỏ.)
                        (256..=262_144).contains(mb)
                    });
                Some(GpuInfo {
                    vendor: Self::classify_vendor(&name).map(str::to_string),
                    name,
                    vram_mb,
                })
            })
            .collect()
    }

    /// Parse `lspci -mm` display-controller rows (name only).
    /// (Parse lspci — chỉ tên, không VRAM.)
    pub(crate) fn parse_lspci(text: &str) -> Vec<GpuInfo> {
        text.lines()
            .filter(|line| {
                let lower = line.to_lowercase();
                line.contains('"')
                    && (lower.contains("vga")
                        || lower.contains("3d controller")
                        || lower.contains("display controller"))
            })
            .filter_map(|line| {
                // -mm quotes every field: ... "VGA compatible controller" "NVIDIA ...".
                // Take the LAST quoted field as the device model.
                // (Định dạng -mm quote mọi field — lấy field quote cuối.)
                let mut fields = Vec::new();
                let mut rest = line;
                while let Some(start) = rest.find('"') {
                    let after = &rest[start + 1..];
                    if let Some(end) = after.find('"') {
                        fields.push(after[..end].to_string());
                        rest = &after[end + 1..];
                    } else {
                        break;
                    }
                }
                fields
                    .pop()
                    .filter(|name| !name.is_empty())
                    .map(|name| GpuInfo {
                        vendor: Self::classify_vendor(&name).map(str::to_string),
                        name,
                        vram_mb: None,
                    })
            })
            .collect()
    }

    /// Windows: CIM video controllers (name + AdapterRAM). AdapterRAM is
    /// a uint32 that wraps past 4 GiB on modern cards — values outside
    /// the sanity window are dropped, never reported as fact.
    /// (Windows: đọc CIM; AdapterRAM tràn số trên card mới — ngoài khoảng
    /// sanity thì bỏ, không báo láo.)
    fn detect_gpus_windows() -> Vec<GpuInfo> {
        let ps = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance Win32_VideoController | Select-Object Name,AdapterRAM | ConvertTo-Json",
            ])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string());
        let json = match ps {
            Some(j) if !j.trim().is_empty() => j,
            _ => return Self::detect_gpus_windows_wmic(),
        };
        Self::parse_cim_videocontroller(&json)
    }

    /// Legacy WMI fallback when CIM/powershell is unavailable.
    /// (Fallback WMI khi không có powershell.)
    fn detect_gpus_windows_wmic() -> Vec<GpuInfo> {
        let text = match std::process::Command::new("wmic")
            .args([
                "path",
                "win32_VideoController",
                "get",
                "name",
                "/format:list",
            ])
            .output()
        {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).to_string()
            }
            _ => return Vec::new(),
        };
        text.lines()
            .filter_map(|line| {
                let name = line.strip_prefix("Name=")?.trim().to_string();
                if name.is_empty() {
                    return None;
                }
                Some(GpuInfo {
                    vendor: Self::classify_vendor(&name).map(str::to_string),
                    name,
                    vram_mb: None,
                })
            })
            .collect()
    }

    /// Parse CIM `Win32_VideoController` JSON (single object or array).
    /// (Parse JSON CIM — object đơn hoặc mảng.)
    pub(crate) fn parse_cim_videocontroller(json: &str) -> Vec<GpuInfo> {
        let value: serde_json::Value = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        let items: Vec<&serde_json::Value> = match &value {
            serde_json::Value::Array(arr) => arr.iter().collect(),
            obj if obj.is_object() => vec![obj],
            _ => return Vec::new(),
        };
        items
            .into_iter()
            .filter_map(|item| {
                let name = item.get("Name")?.as_str()?.trim().to_string();
                if name.is_empty() {
                    return None;
                }
                let vram_mb = item
                    .get("AdapterRAM")
                    .and_then(serde_json::Value::as_u64)
                    .map(|bytes| (bytes / (1024 * 1024)) as usize)
                    .filter(|mb| (256..=262_144).contains(mb));
                Some(GpuInfo {
                    vendor: Self::classify_vendor(&name).map(str::to_string),
                    name,
                    vram_mb,
                })
            })
            .collect()
    }

    /// Normalize a vendor from a model string — pure, unit-tested.
    /// Returns the canonical id or None when unrecognized (unknown is
    /// None, never a guess).
    /// (Chuẩn hóa vendor từ tên model — không nhận ra thì None.)
    pub(crate) fn classify_vendor(name: &str) -> Option<&'static str> {
        let lower = name.to_lowercase();
        if lower.contains("apple") || lower.contains("metal") {
            Some("apple")
        } else if lower.contains("nvidia")
            || lower.contains("geforce")
            || lower.contains("quadro")
            || lower.contains("tesla")
        {
            Some("nvidia")
        } else if lower.contains("amd") || lower.contains("radeon") {
            Some("amd")
        } else if lower.contains("intel")
            || lower.contains("uhd graphics")
            || lower.contains("iris")
            || lower.contains("arc")
        {
            Some("intel")
        } else {
            None
        }
    }

    /// True when at least one detected GPU has this canonical vendor id
    /// (`apple` | `nvidia` | `amd` | `intel`). Empty GPU list → false
    /// (no claim from no data).
    /// (Có GPU của vendor này không — không có GPU thì false.)
    pub fn has_gpu_vendor(&self, vendor: &str) -> bool {
        self.gpus
            .iter()
            .any(|g| g.vendor.as_deref() == Some(vendor))
    }
}
