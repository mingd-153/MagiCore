//! Ecosystem-qualified direct roots for the unified lockfile.
//! Root trực tiếp có định danh ecosystem trong lockfile hợp nhất.

use crate::{EcosystemTag, Lockfile};

const ROOT_PIN_PREFIX: &str = "mgc-root-v1:";

/// Parsed root pin. `ecosystem == None` represents a legacy unqualified pin.
/// Root pin đã phân tích; `ecosystem == None` là pin cũ chưa định danh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootPin<'a> {
    pub ecosystem: Option<EcosystemTag>,
    pub package_id: &'a str,
}

/// Encode a direct package identity without changing the legacy lock schema.
/// Mã hóa định danh package trực tiếp mà không đổi schema lock cũ.
pub fn format_root_pin(ecosystem: EcosystemTag, package_id: &str) -> String {
    format!("{ROOT_PIN_PREFIX}{}:{package_id}", ecosystem.as_str())
}

/// Parse a qualified root pin; unrecognized values remain legacy identities.
/// Phân tích pin có ecosystem; giá trị khác giữ dạng định danh cũ.
pub fn parse_root_pin(value: &str) -> RootPin<'_> {
    let Some(encoded) = value.strip_prefix(ROOT_PIN_PREFIX) else {
        return RootPin {
            ecosystem: None,
            package_id: value,
        };
    };
    let Some((ecosystem, package_id)) = encoded.split_once(':') else {
        return RootPin {
            ecosystem: None,
            package_id: value,
        };
    };
    let ecosystem = match ecosystem {
        "web" => EcosystemTag::Web,
        "rust" => EcosystemTag::Rust,
        "python" => EcosystemTag::Python,
        "dart" => EcosystemTag::Dart,
        "go" => EcosystemTag::Go,
        "maven" => EcosystemTag::Maven,
        "nuget" => EcosystemTag::NuGet,
        "swift" => EcosystemTag::Swift,
        "cocoapods" => EcosystemTag::CocoaPods,
        "unity" => EcosystemTag::Unity,
        "unreal" => EcosystemTag::Unreal,
        "iot" => EcosystemTag::Iot,
        "model" => EcosystemTag::Model,
        "cloud-module" => EcosystemTag::CloudModule,
        "other" => EcosystemTag::Other,
        _ => {
            return RootPin {
                ecosystem: None,
                package_id: value,
            };
        }
    };
    RootPin {
        ecosystem: Some(ecosystem),
        package_id,
    }
}

/// Replace one ecosystem's roots while preserving roots owned by sibling
/// ecosystems in the same core. Legacy unqualified roots are Web roots,
/// matching the only historical writer of this owner-scoped field.
/// Thay root của một ecosystem, giữ nguyên root ecosystem khác cùng core.
pub fn update_owner_root_pins(
    lockfile: &mut Lockfile,
    owner: &str,
    ecosystem: EcosystemTag,
    direct_package_ids: impl IntoIterator<Item = String>,
) {
    let mut roots = lockfile
        .root_dependencies_by_owner
        .remove(owner)
        .unwrap_or_default()
        .into_iter()
        .filter(|root| {
            let parsed = parse_root_pin(root);
            match parsed.ecosystem {
                Some(existing_ecosystem) => existing_ecosystem != ecosystem,
                None if root.starts_with(ROOT_PIN_PREFIX) => true,
                None => ecosystem != EcosystemTag::Web,
            }
        })
        .collect::<Vec<_>>();
    roots.extend(
        direct_package_ids
            .into_iter()
            .map(|id| format_root_pin(ecosystem, &id)),
    );
    roots.sort();
    roots.dedup();
    if !roots.is_empty() {
        lockfile
            .root_dependencies_by_owner
            .insert(owner.to_string(), roots);
    }
}
