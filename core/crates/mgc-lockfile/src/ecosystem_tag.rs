//! Ecosystem tag for mgc.lock schema v3 package entries.
//! Thẻ hệ sinh thái cho entry package trong schema v3 của mgc.lock.
//!
//! v3 turns the single web-only lockfile into a canonical unified graph:
//! every package carries the ecosystem it came from so importers, exporters
//! and the (Phase 2) resolver can route it to the right toolchain.
//! v3 biến lockfile thuần web thành đồ thị hợp nhất canonical: mỗi package
//! mang hệ sinh thái gốc để importer, exporter và resolver (Phase 2) điều
//! phối đúng toolchain.

use serde::{Deserialize, Serialize};

/// Ecosystem of a locked package — Hệ sinh thái của package trong lock.
///
/// Serialized lowercase; `cloud-module` carries an explicit rename because
/// `rename_all = "lowercase"` cannot emit hyphenated names.
/// Serialize dạng lowercase; `cloud-module` cần rename tường minh vì
/// `rename_all = "lowercase"` không sinh được tên có gạch nối.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EcosystemTag {
    /// JavaScript/TypeScript packages from npm-compatible registries.
    /// Package JS/TS từ registry tương thích npm.
    Web,
    /// Rust crates (crates.io) — Rust crates (crates.io).
    Rust,
    /// Python distributions (PyPI) — Python distributions (PyPI).
    Python,
    /// Dart packages (pub.dev) — Dart packages (pub.dev).
    Dart,
    /// Go modules — Go modules.
    Go,
    /// Java/Maven artifacts — Java/Maven artifacts.
    Maven,
    /// .NET NuGet packages — .NET NuGet packages.
    NuGet,
    /// Swift packages — Swift packages.
    Swift,
    /// CocoaPods pods — CocoaPods pods.
    CocoaPods,
    /// Unity UPM packages — Unity UPM packages.
    Unity,
    /// Unreal Engine plugins — Unreal Engine plugins.
    Unreal,
    /// IoT firmware dependencies (PlatformIO/Zephyr) — deps firmware IoT (PlatformIO/Zephyr).
    Iot,
    /// ML models / weights — Model ML / weights.
    Model,
    /// Cloud infrastructure modules (terraform, …) — module hạ tầng cloud (terraform, …).
    #[serde(rename = "cloud-module")]
    CloudModule,
    /// Unknown/unclassified — v2 imports and foreign data land here and are
    /// exempt from provenance verification (v2 files carry no ecosystem data).
    /// Không phân loại — import v2 và dữ liệu lạ rơi vào đây, được miễn
    /// verify provenance (file v2 không mang dữ liệu ecosystem).
    #[default]
    Other,
}

impl EcosystemTag {
    /// Canonical schema string — matches the serde serialization exactly.
    /// Chuỗi canonical của schema — khớp chính xác với serialization serde.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Rust => "rust",
            Self::Python => "python",
            Self::Dart => "dart",
            Self::Go => "go",
            Self::Maven => "maven",
            Self::NuGet => "nuget",
            Self::Swift => "swift",
            Self::CocoaPods => "cocoapods",
            Self::Unity => "unity",
            Self::Unreal => "unreal",
            Self::Iot => "iot",
            Self::Model => "model",
            Self::CloudModule => "cloud-module",
            Self::Other => "other",
        }
    }
}

impl std::fmt::Display for EcosystemTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
