//! SPDX 2.3 JSON serialization for lockfile dependency graphs.
//! Tuần tự hóa đồ thị dependency từ lockfile sang SPDX 2.3 JSON.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::{Bom, SbomError, SbomResult};

const SPDX_VERSION: &str = "SPDX-2.3";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpdxDocument {
    spdx_version: &'static str,
    data_license: &'static str,
    #[serde(rename = "SPDXID")]
    spdx_id: &'static str,
    name: String,
    document_namespace: String,
    creation_info: CreationInfo,
    document_describes: Vec<String>,
    packages: Vec<SpdxPackage>,
    relationships: Vec<SpdxRelationship>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreationInfo {
    creators: Vec<String>,
    created: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpdxPackage {
    #[serde(rename = "SPDXID")]
    spdx_id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    version_info: Option<String>,
    download_location: &'static str,
    files_analyzed: bool,
    license_concluded: &'static str,
    license_declared: &'static str,
    copyright_text: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    checksums: Vec<SpdxChecksum>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    external_refs: Vec<SpdxExternalRef>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpdxChecksum {
    algorithm: &'static str,
    checksum_value: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpdxExternalRef {
    reference_category: &'static str,
    reference_type: &'static str,
    reference_locator: String,
}

#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
struct SpdxRelationship {
    spdx_element_id: String,
    relationship_type: &'static str,
    related_spdx_element: String,
}

/// Convert a generated CycloneDX graph to SPDX 2.3 JSON without exposing
/// registry URLs, credentials, or private source identifiers.
/// Chuyển graph CycloneDX đã tạo sang SPDX 2.3 JSON, không lộ URL registry,
/// thông tin xác thực hoặc định danh nguồn riêng tư.
pub(crate) fn serialize(bom: &Bom, document_name: &str) -> SbomResult<String> {
    if document_name.trim().is_empty() {
        return Err(SbomError::InvalidLockfile(
            "SPDX document name must not be empty".to_string(),
        ));
    }

    let mut package_ids = BTreeMap::new();
    for component in &bom.components {
        if component.name.trim().is_empty() {
            return Err(SbomError::InvalidLockfile(
                "SPDX package name must not be empty".to_string(),
            ));
        }
        let spdx_id = package_spdx_id(&component.bom_ref);
        if package_ids
            .insert(component.bom_ref.clone(), spdx_id)
            .is_some()
        {
            return Err(SbomError::InvalidLockfile(format!(
                "duplicate SPDX package identity: {}",
                component.bom_ref
            )));
        }
    }

    let mut packages = Vec::with_capacity(bom.components.len());
    for component in &bom.components {
        let spdx_id = package_ids
            .get(&component.bom_ref)
            .cloned()
            .ok_or_else(|| {
                SbomError::InvalidLockfile(format!(
                    "SPDX package identity disappeared: {}",
                    component.bom_ref
                ))
            })?;
        let checksums = component
            .hashes
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter_map(spdx_checksum)
            .collect();
        let external_refs = component
            .purl
            .as_deref()
            .filter(|purl| purl.starts_with("pkg:") && !purl.chars().any(char::is_whitespace))
            .map(|purl| {
                vec![SpdxExternalRef {
                    reference_category: "PACKAGE-MANAGER",
                    reference_type: "purl",
                    reference_locator: purl.to_string(),
                }]
            })
            .unwrap_or_default();
        packages.push(SpdxPackage {
            spdx_id,
            name: component.name.clone(),
            version_info: (!component.version.is_empty()).then(|| component.version.clone()),
            download_location: "NOASSERTION",
            files_analyzed: false,
            license_concluded: "NOASSERTION",
            license_declared: "NOASSERTION",
            copyright_text: "NOASSERTION",
            checksums,
            external_refs,
        });
    }
    packages.sort_by(|left, right| left.spdx_id.cmp(&right.spdx_id));

    let mut relationship_set = BTreeSet::new();
    let mut document_describes = Vec::with_capacity(packages.len());
    for package in &packages {
        document_describes.push(package.spdx_id.clone());
        relationship_set.insert(SpdxRelationship {
            spdx_element_id: "SPDXRef-DOCUMENT".to_string(),
            relationship_type: "DESCRIBES",
            related_spdx_element: package.spdx_id.clone(),
        });
    }
    for dependency in bom.dependencies.as_deref().unwrap_or_default() {
        let source = package_ids.get(&dependency.dependency_ref).ok_or_else(|| {
            SbomError::InvalidLockfile(format!(
                "SPDX dependency source is missing from package graph: {}",
                dependency.dependency_ref
            ))
        })?;
        for target_ref in dependency.depends_on.as_deref().unwrap_or_default() {
            let target = package_ids.get(target_ref).ok_or_else(|| {
                SbomError::InvalidLockfile(format!(
                    "SPDX dependency target is missing from package graph: {target_ref}"
                ))
            })?;
            relationship_set.insert(SpdxRelationship {
                spdx_element_id: source.clone(),
                relationship_type: "DEPENDS_ON",
                related_spdx_element: target.clone(),
            });
        }
    }

    let version = env!("CARGO_PKG_VERSION");
    let document = SpdxDocument {
        spdx_version: SPDX_VERSION,
        data_license: "CC0-1.0",
        spdx_id: "SPDXRef-DOCUMENT",
        name: document_name.to_string(),
        document_namespace: format!("urn:uuid:{}", uuid::Uuid::new_v4()),
        creation_info: CreationInfo {
            creators: vec![format!("Tool: mgc-{version}")],
            created: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        },
        document_describes,
        packages,
        relationships: relationship_set.into_iter().collect(),
    };
    Ok(serde_json::to_string_pretty(&document)?)
}

fn package_spdx_id(bom_ref: &str) -> String {
    format!(
        "SPDXRef-Package-{}",
        blake3::hash(bom_ref.as_bytes()).to_hex()
    )
}

fn spdx_checksum(hash: &crate::cyclonedx::Hash) -> Option<SpdxChecksum> {
    let normalized = hash
        .alg
        .chars()
        .filter(|character| *character != '-')
        .map(|character| character.to_ascii_uppercase())
        .collect::<String>();
    let (algorithm, hex_length) = match normalized.as_str() {
        "SHA1" => ("SHA1", 40),
        "SHA224" => ("SHA224", 56),
        "SHA256" => ("SHA256", 64),
        "SHA384" => ("SHA384", 96),
        "SHA512" => ("SHA512", 128),
        "SHA3256" => ("SHA3-256", 64),
        "SHA3384" => ("SHA3-384", 96),
        "SHA3512" => ("SHA3-512", 128),
        "BLAKE3" => ("BLAKE3", 64),
        "BLAKE2B256" => ("BLAKE2b-256", 64),
        "BLAKE2B384" => ("BLAKE2b-384", 96),
        "BLAKE2B512" => ("BLAKE2b-512", 128),
        "MD5" => ("MD5", 32),
        "ADLER32" => ("ADLER32", 8),
        _ => return None,
    };
    if hash.content.len() != hex_length
        || !hash.content.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    Some(SpdxChecksum {
        algorithm,
        checksum_value: hash.content.to_ascii_lowercase(),
    })
}
