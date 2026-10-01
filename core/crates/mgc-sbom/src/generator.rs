//! SBOM generator from lockfile
//! Tạo SBOM từ lockfile

use crate::cyclonedx::*;
use crate::{SbomOptions, SbomResult};
use mgc_lockfile::{
    EcosystemTag, Lockfile, canonical::LockfileV4, policy::LockPolicyMode, schema::Package,
    v4::PackageKey,
};
use std::collections::HashMap;

/// SBOM generator — Trình tạo SBOM
pub struct SbomGenerator {
    options: SbomOptions,
}

impl SbomGenerator {
    /// Create new generator — Tạo generator mới
    pub fn new(options: SbomOptions) -> Self {
        Self { options }
    }

    /// Generate SBOM from lockfile — Tạo SBOM từ lockfile
    pub fn generate(&self, lockfile: &Lockfile) -> SbomResult<Bom> {
        let mut bom = Bom::new();

        let mut components = Vec::new();
        let mut dependency_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut refs_by_legacy_id: HashMap<String, Vec<String>> = HashMap::new();
        let mut refs_by_legacy_name: HashMap<String, Vec<String>> = HashMap::new();
        let mut unique_refs = std::collections::HashSet::new();

        for pkg in &lockfile.packages {
            let bom_ref = legacy_bom_ref(pkg);
            if !unique_refs.insert(bom_ref.clone()) {
                return Err(crate::SbomError::InvalidLockfile(format!(
                    "duplicate legacy package identity: {}@{} ({})",
                    pkg.name, pkg.version, pkg.ecosystem
                )));
            }
            refs_by_legacy_id
                .entry(format!("{}@{}", pkg.name, pkg.version))
                .or_default()
                .push(bom_ref);
            refs_by_legacy_name
                .entry(pkg.name.clone())
                .or_default()
                .push(legacy_bom_ref(pkg));
        }

        for pkg in &lockfile.packages {
            let bom_ref = legacy_bom_ref(pkg);

            let mut component = Component {
                component_type: ComponentType::Library,
                bom_ref: bom_ref.clone(),
                name: component_name(pkg.ecosystem, &pkg.name),
                version: pkg.version.clone(),
                purl: package_purl(pkg.ecosystem, &pkg.name, &pkg.version),
                hashes: None,
                licenses: None,
            };

            // Add hashes if requested
            if self.options.include_hashes && !pkg.integrity.is_empty() {
                // Parse integrity (e.g., "blake3:abc123...")
                if let Some((alg, content)) = pkg.integrity.split_once(':') {
                    component.hashes = Some(vec![Hash {
                        alg: alg.to_uppercase(),
                        content: content.to_string(),
                    }]);
                }
            }

            components.push(component);

            if !pkg.dependencies.is_empty() {
                let deps = pkg
                    .dependencies
                    .iter()
                    .map(|dependency| {
                        resolve_legacy_dependency_ref(
                            dependency,
                            &refs_by_legacy_id,
                            &refs_by_legacy_name,
                        )
                    })
                    .collect::<SbomResult<Vec<_>>>()?;
                dependency_map.insert(bom_ref, deps);
            }
        }

        bom.components = components;

        // Add dependencies graph
        let dependencies: Vec<Dependency> = dependency_map
            .into_iter()
            .map(|(dep_ref, depends_on)| Dependency {
                dependency_ref: dep_ref,
                depends_on: Some(depends_on),
            })
            .collect();

        if !dependencies.is_empty() {
            bom.dependencies = Some(dependencies);
        }

        Ok(bom)
    }

    /// Generate CycloneDX from a v4 graph without collapsing package identity.
    /// Tạo CycloneDX từ graph v4 mà không làm mất identity package.
    pub fn generate_v4(
        &self,
        lockfile: &LockfileV4,
        policy: LockPolicyMode,
        trust_keys: &[String],
    ) -> SbomResult<Bom> {
        self.generate_v4_with_report(lockfile, policy, trust_keys)
            .map(|(bom, _report)| bom)
    }

    /// Generate a v4 BOM and return the single verification report used to
    /// enforce policy, so callers can present trust warnings without hashing
    /// and verifying the complete lock graph a second time.
    pub fn generate_v4_with_report(
        &self,
        lockfile: &LockfileV4,
        policy: LockPolicyMode,
        trust_keys: &[String],
    ) -> SbomResult<(Bom, mgc_lockfile::policy::V4VerifyReport)> {
        if lockfile.version != mgc_lockfile::v4::LOCKFILE_SCHEMA_V4 {
            return Err(crate::SbomError::InvalidLockfile(format!(
                "expected lockfile schema v4, got '{}'",
                lockfile.version
            )));
        }
        let report = mgc_lockfile::policy::verify_v4_math(lockfile)?;
        mgc_lockfile::policy::enforce_policy(&report, policy, trust_keys)?;
        let mut bom = Bom::new();
        let mut refs_by_key = HashMap::with_capacity(lockfile.packages.len());
        for package in &lockfile.packages {
            let bom_ref = v4_bom_ref(&package.key)?;
            if refs_by_key.insert(package.key.clone(), bom_ref).is_some() {
                return Err(crate::SbomError::InvalidLockfile(format!(
                    "duplicate v4 package identity: {}@{} ({})",
                    package.key.name, package.key.version, package.key.ecosystem
                )));
            }
        }

        let mut components = Vec::with_capacity(lockfile.packages.len());
        let mut dependencies = Vec::new();
        for package in &lockfile.packages {
            let bom_ref = refs_by_key
                .get(&package.key)
                .ok_or_else(|| {
                    crate::SbomError::InvalidLockfile(
                        "package identity disappeared while building SBOM".to_string(),
                    )
                })?
                .clone();
            let mut component = Component {
                component_type: ComponentType::Library,
                bom_ref: bom_ref.clone(),
                name: component_name(package.key.ecosystem, &package.key.name),
                version: package.key.version.clone(),
                purl: package_purl(
                    package.key.ecosystem,
                    &package.key.name,
                    &package.key.version,
                ),
                hashes: None,
                licenses: None,
            };
            if self.options.include_hashes
                && let Some(artifact) = &package.artifact
                && is_hex_digest(&artifact.content_hash)
            {
                component.hashes = Some(vec![Hash {
                    alg: "BLAKE3".to_string(),
                    content: artifact.content_hash.clone(),
                }]);
            }
            components.push(component);

            if !package.edges.is_empty() {
                let mut depends_on = Vec::with_capacity(package.edges.len());
                for edge in &package.edges {
                    let target = refs_by_key.get(&edge.target_key).ok_or_else(|| {
                        crate::SbomError::InvalidLockfile(format!(
                            "dependency edge from {}@{} targets an absent locked package {}@{}",
                            package.key.name,
                            package.key.version,
                            edge.target_key.name,
                            edge.target_key.version
                        ))
                    })?;
                    depends_on.push(target.clone());
                }
                depends_on.sort();
                depends_on.dedup();
                dependencies.push(Dependency {
                    dependency_ref: bom_ref,
                    depends_on: Some(depends_on),
                });
            }
        }
        bom.components = components;
        if !dependencies.is_empty() {
            bom.dependencies = Some(dependencies);
        }
        Ok((bom, report))
    }

    /// Generate SBOM JSON string — Tạo chuỗi JSON SBOM
    pub fn generate_json(&self, lockfile: &Lockfile) -> SbomResult<String> {
        let bom = self.generate(lockfile)?;
        let json = serde_json::to_string_pretty(&bom)?;
        Ok(json)
    }

    pub fn generate_json_v4(
        &self,
        lockfile: &LockfileV4,
        policy: LockPolicyMode,
        trust_keys: &[String],
    ) -> SbomResult<String> {
        self.generate_json_v4_with_report(lockfile, policy, trust_keys)
            .map(|(json, _report)| json)
    }

    pub fn generate_json_v4_with_report(
        &self,
        lockfile: &LockfileV4,
        policy: LockPolicyMode,
        trust_keys: &[String],
    ) -> SbomResult<(String, mgc_lockfile::policy::V4VerifyReport)> {
        let (bom, report) = self.generate_v4_with_report(lockfile, policy, trust_keys)?;
        Ok((serde_json::to_string_pretty(&bom)?, report))
    }
}

fn legacy_bom_ref(package: &Package) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"mgc-sbom-legacy-identity-v1\0");
    for value in [&package.name, &package.version, &package.resolved] {
        hasher.update(&(value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    format!(
        "mgc:lock-v3:{}:{}:{}:{}",
        package.ecosystem,
        percent_encode_path(&package.name),
        percent_encode_path(&package.version),
        hasher.finalize().to_hex()
    )
}

fn v4_bom_ref(key: &PackageKey) -> SbomResult<String> {
    // PackageKey includes source_id, which can contain a private registry URL
    // or credentials. Keep the reference stable without serializing that data
    // into a public SBOM field.
    let identity = serde_json::to_vec(key)?;
    Ok(format!("mgc:lock-v4:{}", blake3::hash(&identity).to_hex()))
}

fn resolve_legacy_dependency_ref(
    dependency: &str,
    refs_by_legacy_id: &HashMap<String, Vec<String>>,
    refs_by_legacy_name: &HashMap<String, Vec<String>>,
) -> SbomResult<String> {
    let refs = refs_by_legacy_id
        .get(dependency)
        .or_else(|| {
            // Some legacy writers store only the dependency name. Accept it only
            // if it identifies exactly one locked package across the graph.
            refs_by_legacy_name.get(dependency)
        })
        .ok_or_else(|| {
            crate::SbomError::InvalidLockfile(format!(
                "dependency '{dependency}' does not resolve to a package in mgc.lock"
            ))
        })?;
    if refs.len() != 1 {
        return Err(crate::SbomError::InvalidLockfile(format!(
            "dependency '{dependency}' is ambiguous in legacy mgc.lock"
        )));
    }
    Ok(refs[0].clone())
}

fn package_purl(ecosystem: EcosystemTag, name: &str, version: &str) -> Option<String> {
    if name.trim().is_empty() || version.trim().is_empty() {
        return None;
    }
    let (kind, path) = match ecosystem {
        EcosystemTag::Web => ("npm", purl_path(name)),
        EcosystemTag::Rust => ("cargo", purl_path(name)),
        EcosystemTag::Python => ("pypi", purl_path(&normalize_pypi_name(name))),
        EcosystemTag::Dart => ("pub", purl_path(&normalize_pub_name(name))),
        EcosystemTag::Go => {
            if name.bytes().any(|byte| byte.is_ascii_uppercase()) {
                return None;
            }
            ("golang", purl_path(name))
        }
        EcosystemTag::Maven => {
            let (group, artifact) = name.split_once(':')?;
            if group.is_empty() || artifact.is_empty() {
                return None;
            }
            (
                "maven",
                format!("{}/{}", purl_path(group), purl_path(artifact)),
            )
        }
        EcosystemTag::NuGet => ("nuget", purl_path(name)),
        EcosystemTag::Swift => ("swift", purl_path(&swift_purl_path(name)?)),
        EcosystemTag::CocoaPods => ("cocoapods", purl_path(name)),
        EcosystemTag::Unity
        | EcosystemTag::Unreal
        | EcosystemTag::Iot
        | EcosystemTag::Model
        | EcosystemTag::CloudModule
        | EcosystemTag::Other => return None,
    };
    Some(format!(
        "pkg:{kind}/{path}@{}",
        percent_encode_path(version)
    ))
}

fn normalize_pypi_name(name: &str) -> String {
    let mut output = String::with_capacity(name.len());
    let mut previous_separator = false;
    for character in name.chars() {
        if matches!(character, '-' | '_' | '.') {
            if !previous_separator {
                output.push('-');
            }
            previous_separator = true;
        } else {
            output.extend(character.to_lowercase());
            previous_separator = false;
        }
    }
    output
}

fn normalize_pub_name(name: &str) -> String {
    name.chars()
        .map(|character| match character {
            'A'..='Z' => character.to_ascii_lowercase(),
            'a'..='z' | '0'..='9' | '_' => character,
            _ => '_',
        })
        .collect()
}

fn swift_purl_path(name: &str) -> Option<String> {
    let mut path = name.trim();
    if let Ok(url) = url::Url::parse(path) {
        if !matches!(url.scheme(), "https" | "http") {
            return None;
        }
        let host = url.host_str()?;
        let mut segments = vec![host.to_string()];
        segments.extend(
            url.path_segments()?
                .filter(|segment| !segment.is_empty())
                .map(percent_decode_path_segment),
        );
        if segments
            .last()
            .is_some_and(|segment| segment.ends_with(".git"))
        {
            let last = segments.last_mut()?;
            last.truncate(last.len() - 4);
        }
        return (segments.len() >= 3).then(|| segments.join("/"));
    }
    if path.contains("://") {
        return None;
    }
    path = path.split(['?', '#']).next()?;
    path = path.strip_suffix(".git").unwrap_or(path);
    let segments: Vec<_> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    (segments.len() >= 3).then(|| segments.join("/"))
}

fn component_name(ecosystem: EcosystemTag, name: &str) -> String {
    if ecosystem == EcosystemTag::Swift {
        return swift_purl_path(name)
            .and_then(|path| path.rsplit('/').next().map(str::to_string))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "swift-package".to_string());
    }
    name.to_string()
}

fn percent_decode_path_segment(segment: &str) -> String {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            decoded.push((high << 4) | low);
            index += 3;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn purl_path(value: &str) -> String {
    value
        .split('/')
        .map(percent_encode_path)
        .collect::<Vec<_>>()
        .join("/")
}

fn percent_encode_path(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn is_hex_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

impl Default for SbomGenerator {
    fn default() -> Self {
        Self::new(SbomOptions::default())
    }
}
