//! `adapters/mod.rs` — Optimizer Adapter Pattern (Runtime-Specific Optimizations)
//! `adapters/mod.rs` — Mẫu Adapter cho Optimizer (Tối ưu hóa theo từng Runtime)

pub mod bun;
pub mod candle;
pub mod deno;
pub mod flutter;
pub mod go_ai;
pub mod go_lib;
pub mod node;
pub mod python_lib;
pub mod pytorch;
pub mod react_native;
pub mod rust_lib;
pub mod typescript_lib;

use crate::commands::optimizer::detect::HardwareInfo;
use crate::commands::optimizer::generators::OptimizedConfigFile;
use crate::commands::optimizer::runtime_detect::DetectedRuntime;

/// Trait for runtime-specific optimizer adapters — trait cho adapter tối ưu theo runtime
pub trait OptimizerAdapter {
    /// Human-readable name for this adapter (e.g., "Node.js", "Deno", "PyTorch") — tên dễ đọc cho adapter
    fn name(&self) -> &'static str;

    /// Check if this adapter matches the detected runtime — kiểm tra adapter khớp với runtime đã phát hiện
    fn matches(&self, runtime: &DetectedRuntime) -> bool;

    /// Generate optimization config files for this runtime — tạo các file config tối ưu cho runtime này
    fn generate(&self, hw: &HardwareInfo) -> Vec<OptimizedConfigFile>;
}

/// Get all available adapters — lấy tất cả adapters có sẵn
pub fn all_adapters() -> Vec<Box<dyn OptimizerAdapter>> {
    vec![
        // Web adapters — adapters web
        Box::new(node::NodeJsAdapter),
        Box::new(deno::DenoAdapter),
        Box::new(bun::BunAdapter),
        // AI adapters — adapters AI
        Box::new(pytorch::PyTorchAdapter),
        Box::new(candle::CandleAdapter),
        Box::new(go_ai::GoAiAdapter),
        // Lib adapters — adapters thư viện
        Box::new(rust_lib::RustLibAdapter),
        Box::new(go_lib::GoLibAdapter),
        Box::new(python_lib::PythonLibAdapter),
        Box::new(typescript_lib::TypeScriptLibAdapter),
        // App adapters — adapters ứng dụng
        Box::new(flutter::FlutterAdapter),
        Box::new(react_native::ReactNativeAdapter),
    ]
}

/// Find adapters matching detected runtimes — tìm adapters khớp với runtimes đã phát hiện
///
/// P0-4 (2026-09-10 native-engine audit): Bun/Deno là runtime ĐỐI THỦ —
/// adapter của chúng chỉ được sinh profile khi dự án CHỌN compat mode
/// tường minh (MGC_COMPAT_RUNTIME / --compat-runtime). Trên đường native
/// mặc định, không sinh profile tối ưu cho runtime đối thủ (tách optimizer
/// rival khỏi optimizer native mặc định).
/// (Bun/Deno adapters only generate profiles under an explicit compat
/// opt-in; the default native lane never generates rival-runtime envs.)
pub fn find_adapters(runtimes: &[DetectedRuntime]) -> Vec<Box<dyn OptimizerAdapter>> {
    let all = all_adapters();
    let mut matched = vec![];

    // Rival runtimes require the explicit compat opt-in (same gate as
    // spawn: cli compat.rs). Without it, Bun/Deno adapters stay dormant.
    // Runtime đối thủ cần compat tường minh (cùng cổng với spawn). Thiếu
    // nó, adapter Bun/Deno ngủ yên.
    let compat = crate::commands::compat::CompatMode::from_flag(None)
        .unwrap_or(crate::commands::compat::CompatMode::Native);
    let rival_allowed = |name: &str| -> bool {
        match compat {
            crate::commands::compat::CompatMode::Native => false,
            crate::commands::compat::CompatMode::Explicit(ref allowed) => {
                allowed.eq_ignore_ascii_case(name)
            }
        }
    };

    for runtime in runtimes {
        for adapter in &all {
            if adapter.matches(runtime) {
                let adapter_name = adapter.name();
                let is_rival = matches!(adapter_name, "Bun" | "Deno");
                if is_rival && !rival_allowed(adapter_name) {
                    continue;
                }
                matched.push(adapter_name);
            }
        }
    }

    // Return fresh instances for matched adapters — trả về instances mới cho adapters khớp
    all.into_iter()
        .filter(|a| matched.contains(&a.name()))
        .collect()
}
