//! React Native layered dependencies (JS / Android / iOS tiers).
//! Dependency phân tầng của React Native (tier JS / Android / iOS).
//!
//! React Native has no single registry: a project carries three
//! independent dependency universes.
//! - JS tier: npm `package.json` — resolved by the web adapter's npm
//!   pipeline (the app adapter delegates; no npm logic here).
//! - Android tier: Gradle's pinned `gradle.lockfile`
//!   (`group:artifact:version:configuration` lines) — every pin is an
//!   EXACT Maven coordinate, verified/materimized by the native Maven
//!   engine (ecosystem=maven in mgc.lock).
//! - iOS tier: CocoaPods `Podfile.lock` (`PODS:` closure + `SPEC
//!   CHECKSUMS:` sha1 map) — each pod's spec is fetched from the CocoaPods
//!   CDN at `{cdn}/{seg}/{name}/{version}/{name}.podspec.json` (seg = the
//!   lowercased first character, or `0-9` for digits) and its sha1 must
//!   match the lock's recorded checksum (fail-closed on mismatch).
//!   A missing Podfile.lock skips the tier with guidance — never a silent
//!   empty iOS graph.
//!
//! React Native không có một registry duy nhất: project mang ba vũ trụ
//! dependency độc lập.
//! - Tier JS: npm `package.json` — do pipeline npm của adapter web resolve
//!   (app adapter ủy quyền; không có logic npm ở đây).
//! - Tier Android: `gradle.lockfile` đã ghim của Gradle (dòng
//!   `group:artifact:version:configuration`) — mọi pin là toạ độ Maven
//!   CHÍNH XÁC, được engine Maven native xác minh/materialize
//!   (ecosystem=maven trong mgc.lock).
//! - Tier iOS: CocoaPods `Podfile.lock` (bao đóng `PODS:` + map sha1
//!   `SPEC CHECKSUMS:`) — spec của mỗi pod tải từ CDN CocoaPods tại
//!   `{cdn}/{seg}/{name}/{version}/{name}.podspec.json` (seg = ký tự đầu
//!   viết thường, hoặc `0-9` với chữ số) và sha1 phải khớp checksum ghi
//!   trong lock (lệch là fail-closed). Thiếu Podfile.lock thì bỏ tier kèm
//!   hướng dẫn — không bao giờ graph iOS rỗng âm thầm.

use mgc_types::{MgError, MgResult};
use serde_json::Value;
use sha1::Digest as _;
use sha1::Sha1;

const DEFAULT_CDN_URL: &str = "https://cdn.cocoapods.org";

/// One pinned Gradle coordinate from `gradle.lockfile`.
/// Một toạ độ Gradle đã ghim từ `gradle.lockfile`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GradleLockEntry {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub configuration: String,
}

impl GradleLockEntry {
    /// Maven coordinate name (`group:artifact`).
    /// Tên toạ độ Maven (`group:artifact`).
    pub fn coordinate(&self) -> String {
        format!("{}:{}", self.group, self.artifact)
    }
}

/// Parse `gradle.lockfile` text: `group:artifact:version:configuration`
/// lines (comments/empty lines skipped). A line with fewer than three
/// `:`-separated fields is malformed — the whole parse fails closed rather
/// than silently dropping a pinned dependency.
/// Parse nội dung `gradle.lockfile`: dòng
/// `group:artifact:version:configuration` (bỏ comment/dòng trống). Dòng
/// thiếu ba trường `:` là sai định dạng — cả parse fail-closed thay vì âm
/// thầm bỏ một dependency đã ghim.
pub fn parse_gradle_lockfile(text: &str) -> MgResult<Vec<GradleLockEntry>> {
    let mut out = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() < 3 {
            return Err(MgError::Other(format!(
                "gradle.lockfile line {} is not group:artifact:version[:configuration]: '{line}' (fail-closed)",
                idx + 1
            )));
        }
        out.push(GradleLockEntry {
            group: parts[0].to_string(),
            artifact: parts[1].to_string(),
            version: parts[2].to_string(),
            configuration: parts.get(3).unwrap_or(&"").to_string(),
        });
    }
    Ok(out)
}

/// Collapse Gradle lock entries to one exact pin per coordinate: repeated
/// configurations of the same version collapse; two DIFFERENT versions for
/// the same coordinate are ambiguous and fail closed.
/// Gộp entry lock Gradle thành một pin chính xác mỗi toạ độ: cùng version
/// lặp ở nhiều configuration thì gộp; hai version KHÁC nhau cho cùng toạ độ
/// là mơ hồ và fail-closed.
pub fn collapse_gradle_pins(entries: &[GradleLockEntry]) -> MgResult<Vec<(String, String)>> {
    let mut pins: Vec<(String, String)> = Vec::new();
    for entry in entries {
        let coordinate = entry.coordinate();
        match pins.iter_mut().find(|(name, _)| name == &coordinate) {
            Some((_, version)) if version != &entry.version => {
                return Err(MgError::Other(format!(
                    "{coordinate} is pinned to both {version} and {} in gradle.lockfile — refusing to guess (fail-closed)",
                    entry.version
                )));
            }
            Some(_) => {}
            None => pins.push((coordinate, entry.version.clone())),
        }
    }
    Ok(pins)
}

/// One pod of the Podfile.lock `PODS:` closure. The ROOT pod name is kept
/// (`Firebase/Core` → `Firebase`) — subspecs share the root spec.
/// Một pod trong bao đóng `PODS:` của Podfile.lock. Giữ tên pod GỐC
/// (`Firebase/Core` → `Firebase`) — subspec dùng chung spec gốc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodfileLockPod {
    pub name: String,
    pub version: String,
}

/// Parsed Podfile.lock: the pod closure plus the `SPEC CHECKSUMS:` sha1
/// map (root pod name → sha1 hex).
/// Podfile.lock đã parse: bao đóng pod cộng map sha1 `SPEC CHECKSUMS:`
/// (tên pod gốc → sha1 hex).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PodfileLock {
    pub pods: Vec<PodfileLockPod>,
    pub checksums: Vec<(String, String)>,
}

impl PodfileLock {
    pub fn checksum_for(&self, pod: &str) -> Option<&str> {
        self.checksums
            .iter()
            .find(|(name, _)| name == pod)
            .map(|(_, sha)| sha.as_str())
    }
}

/// Parse Podfile.lock text: `PODS:` entries (`- Name/Subspec (1.2.3)` and
/// nested `- Sub (= 1.2.3)` dependency lines) and the `SPEC CHECKSUMS:`
/// map. A pod declared without a version (`- LocalPod (from ...)`) is
/// skipped — it is not a registry artifact.
/// Parse nội dung Podfile.lock: entry `PODS:` (dòng `- Name/Subspec
/// (1.2.3)` và dòng dep lồng `- Sub (= 1.2.3)`) cùng map
/// `SPEC CHECKSUMS:`. Pod khai báo không có version (`- LocalPod (from
/// ...)`) bị bỏ — đó không phải artifact registry.
pub fn parse_podfile_lock(text: &str) -> PodfileLock {
    let mut lock = PodfileLock::default();
    #[derive(PartialEq, Eq, Clone, Copy)]
    enum Section {
        None,
        Pods,
        Checksums,
    }
    let mut section = Section::None;
    for raw in text.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        // Top-level section headers are unindented `KEY:` lines.
        // (Header section cấp cao là dòng `KEY:` không thụt lề.)
        if !raw.starts_with(' ') && !raw.starts_with('\t') {
            section = match raw.trim() {
                "PODS:" => Section::Pods,
                "SPEC CHECKSUMS:" => Section::Checksums,
                _ => Section::None,
            };
            continue;
        }
        match section {
            Section::Pods => {
                let trimmed = raw.trim_start();
                // Nested dependency lines (`  - Sub (= 1.0)`) belong to the
                // pod above, not to the closure — only two-space-indented
                // entries are pods.
                // (Dòng dep lồng (`  - Sub (= 1.0)`) thuộc pod bên trên,
                // không thuộc bao đóng — chỉ entry thụt lề hai khoảng là
                // pod.)
                let indent = raw.len() - trimmed.len();
                if indent != 2 || !trimmed.starts_with("- ") {
                    continue;
                }
                let body = trimmed.trim_start_matches("- ").trim();
                if let Some((name, version)) = pod_name_version(body) {
                    let root = name.split('/').next().unwrap_or(&name).to_string();
                    if !lock.pods.iter().any(|p| p.name == root) {
                        lock.pods.push(PodfileLockPod {
                            name: root,
                            version,
                        });
                    }
                }
            }
            Section::Checksums => {
                let trimmed = raw.trim();
                if let Some((name, sha)) = trimmed.split_once(':') {
                    let name = name.trim().to_string();
                    let sha = sha.trim().to_ascii_lowercase();
                    if !name.is_empty() && !sha.is_empty() {
                        lock.checksums.push((name, sha));
                    }
                }
            }
            Section::None => {}
        }
    }
    lock
}

/// `Name/Subspec (1.2.3)` → ("Name/Subspec", "1.2.3"); version-less
/// declarations (local pods / `from`) return None.
/// `Name/Subspec (1.2.3)` → ("Name/Subspec", "1.2.3"); khai báo không
/// version (pod local / `from`) trả None.
fn pod_name_version(body: &str) -> Option<(String, String)> {
    let open = body.find('(')?;
    let close = body[open..].find(')')? + open;
    let version = body[open + 1..close].trim();
    // Only a plain semantic version counts — `(= 1.2.3)`, `(from ...)` and
    // `(1.2.3-beta)` on the DEVELOPMENT line are not pod declarations.
    // (Chỉ version ngữ nghĩa trần được tính — `(= 1.2.3)`, `(from ...)`
    // và `(1.2.3-beta)` trên dòng DEVELOPMENT không phải khai báo pod.)
    if version.starts_with('=') || version.starts_with("from") || version.contains(' ') {
        return None;
    }
    let name = body[..open].trim().trim_end_matches(':').trim();
    if name.is_empty() {
        return None;
    }
    Some((name.to_string(), version.to_string()))
}

/// CocoaPods CDN url: `{cdn}/{seg}/{name}/{version}/{name}.podspec.json`
/// (seg = lowercased first character, `0-9` for digits).
/// URL CDN CocoaPods: `{cdn}/{seg}/{name}/{version}/{name}.podspec.json`
/// (seg = ký tự đầu viết thường, `0-9` với chữ số).
pub fn cocoapods_spec_url(cdn: &str, name: &str, version: &str) -> String {
    let seg = name
        .chars()
        .next()
        .map(|c| {
            if c.is_ascii_digit() {
                "0-9".to_string()
            } else {
                c.to_ascii_lowercase().to_string()
            }
        })
        .unwrap_or_else(|| "0-9".to_string());
    format!(
        "{}/{}/{}/{}/{}.podspec.json",
        cdn.trim_end_matches('/'),
        seg,
        name,
        version,
        name
    )
}

/// A verified pod spec: the sha1 (hex) of the exact spec bytes.
/// Spec pod đã xác minh: sha1 (hex) của đúng byte spec.
#[derive(Debug, Clone)]
pub struct VerifiedPodSpec {
    pub sha1: String,
    pub spec: Value,
}

/// CocoaPods CDN client: fetch a podspec and verify its sha1 against the
/// checksum recorded in Podfile.lock.
/// Client CDN CocoaPods: tải podspec và xác minh sha1 theo checksum ghi
/// trong Podfile.lock.
#[derive(Debug, Clone)]
pub struct CocoaPodsProtocol {
    cdn: String,
    client: mgc_http::HttpClient,
}

impl CocoaPodsProtocol {
    pub fn with_cdn(cdn: &str) -> Self {
        Self {
            cdn: cdn.trim_end_matches('/').to_string(),
            client: mgc_http::HttpClient::default(),
        }
    }

    /// Build from environment: `MGC_COCOAPODS_CDN_URL`.
    /// Dựng từ môi trường: `MGC_COCOAPODS_CDN_URL`.
    pub fn from_env() -> Self {
        let cdn = std::env::var("MGC_COCOAPODS_CDN_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_CDN_URL.to_string());
        Self::with_cdn(&cdn)
    }

    pub fn cdn(&self) -> &str {
        &self.cdn
    }

    /// Fetch the spec for `name@version`, verify sha1 == `expected_sha1`
    /// (from Podfile.lock) and return the parsed spec. A missing spec, a
    /// malformed checksum, or any mismatch fails closed.
    /// Tải spec cho `name@version`, xác minh sha1 == `expected_sha1` (từ
    /// Podfile.lock) và trả spec đã parse. Thiếu spec, checksum sai định
    /// dạng, hoặc lệch bất kỳ đều fail-closed.
    pub async fn fetch_verified_spec(
        &self,
        name: &str,
        version: &str,
        expected_sha1: &str,
    ) -> MgResult<VerifiedPodSpec> {
        let expected = expected_sha1.trim().to_ascii_lowercase();
        if expected.len() != 40 || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(MgError::Integrity(format!(
                "Podfile.lock checksum for {name} is not a sha1 hex digest: '{expected_sha1}'"
            )));
        }
        let url = cocoapods_spec_url(&self.cdn, name, version);
        let resp = self
            .client
            .get(&url)
            .await
            .map_err(|e| MgError::Network(format!("GET {url} failed: {e}")))?;
        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| MgError::Network(format!("read {url} body failed: {e}")))?;
        if !status.is_success() {
            return Err(MgError::Network(format!("GET {url} returned {status}")));
        }
        let actual = hex::encode(Sha1::digest(&bytes));
        if actual != expected {
            return Err(MgError::Integrity(format!(
                "podspec sha1 mismatch for {name} {version}: Podfile.lock says {expected}, CDN spec is {actual} (fail-closed)"
            )));
        }
        let spec: Value = serde_json::from_slice(&bytes).map_err(|e| {
            MgError::Other(format!(
                "parse podspec JSON for {name} {version} failed: {e}"
            ))
        })?;
        Ok(VerifiedPodSpec { sha1: actual, spec })
    }

    /// Declared dependencies of a verified spec as `(name, requirement)`
    /// edges (`FirebaseCore (= 10.18.0)` → `("FirebaseCore", "= 10.18.0")`).
    /// Dep khai báo của spec đã xác minh dạng cạnh `(name, requirement)`
    /// (`FirebaseCore (= 10.18.0)` → `("FirebaseCore", "= 10.18.0")`).
    pub fn spec_dependencies(spec: &Value) -> Vec<(String, String)> {
        let Some(deps) = spec.get("dependencies").and_then(Value::as_object) else {
            return Vec::new();
        };
        deps.iter()
            .map(|(name, reqs)| {
                let range = reqs
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                (name.clone(), range)
            })
            .collect()
    }
}

/// The pod name a CocoaPods dependency string refers to (subspecs resolve
/// to their root spec).
/// Tên pod mà một chuỗi dependency CocoaPods trỏ tới (subspec quy về spec
/// gốc).
pub fn pod_root(name: &str) -> &str {
    name.split('/').next().unwrap_or(name)
}

/// Marker written for the iOS tier's verified spec sha1.
/// Marker ghi cho sha1 spec đã xác minh của tier iOS.
pub const PODSPEC_SHA1_MARKER_PREFIX: &str = "podspec-sha1:";

/// Marker prefix recording which react-native tier produced a lock entry
/// (`rn-tier:js` / `rn-tier:android` / `rn-tier:ios`).
/// Tiền tố marker ghi tier react-native sinh ra entry lock
/// (`rn-tier:js` / `rn-tier:android` / `rn-tier:ios`).
pub const RN_TIER_MARKER_PREFIX: &str = "rn-tier:";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradle_lockfile_parse_and_collapse() {
        let text = "# This is a Gradle generated file for dependency locking.\n\
                    # Manual edits can break the build.\n\
                    com.google.android.material:material:1.10.0:releaseCompileClasspath\n\
                    com.google.android.material:material:1.10.0:debugCompileClasspath\n\
                    androidx.core:core-ktx:1.12.0:releaseCompileClasspath\n";
        let entries = parse_gradle_lockfile(text).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].group, "com.google.android.material");
        assert_eq!(entries[0].artifact, "material");
        assert_eq!(entries[0].configuration, "releaseCompileClasspath");
        let pins = collapse_gradle_pins(&entries).unwrap();
        assert_eq!(
            pins,
            vec![
                (
                    "com.google.android.material:material".to_string(),
                    "1.10.0".to_string()
                ),
                ("androidx.core:core-ktx".to_string(), "1.12.0".to_string()),
            ]
        );
    }

    #[test]
    fn gradle_lockfile_malformed_and_conflicting_fail_closed() {
        let err = parse_gradle_lockfile("only:two\n").unwrap_err();
        assert!(err.to_string().contains("gradle.lockfile line 1"), "{err}");
        let conflicting = parse_gradle_lockfile("a.b:c:1.0.0:x\na.b:c:2.0.0:y\n").unwrap();
        let err = collapse_gradle_pins(&conflicting).unwrap_err();
        assert!(err.to_string().contains("refusing to guess"), "{err}");
    }

    #[test]
    fn podfile_lock_parse_pods_and_checksums() {
        let text = "PODS:\n  - Alamofire (5.6.4)\n  - Firebase/Core (10.18.0):\n    - FirebaseCore (= 10.18.0)\n  - FirebaseCore (10.18.0)\n  - LocalOnly (from `../local`)\n\nDEPENDENCIES:\n  - Alamofire (~> 5.6)\n  - Firebase/Core\n\nSPEC CHECKSUMS:\n  Alamofire: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n  Firebase: bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n  FirebaseCore: cccccccccccccccccccccccccccccccccccccccc\n\nPODFILE CHECKSUM: dddddddddddddddddddddddddddddddddddddddd\n";
        let lock = parse_podfile_lock(text);
        let names: Vec<&str> = lock.pods.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Alamofire", "Firebase", "FirebaseCore"]);
        assert_eq!(lock.pods[1].version, "10.18.0");
        assert_eq!(
            lock.checksum_for("Alamofire"),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert!(lock.checksum_for("FirebaseCore").is_some());
    }

    #[test]
    fn cocoapods_spec_url_segments() {
        assert_eq!(
            cocoapods_spec_url("https://cdn.cocoapods.org", "Alamofire", "5.6.4"),
            "https://cdn.cocoapods.org/a/Alamofire/5.6.4/Alamofire.podspec.json"
        );
        assert_eq!(
            cocoapods_spec_url("https://cdn.cocoapods.org/", "Bolts", "1.9.0"),
            "https://cdn.cocoapods.org/b/Bolts/1.9.0/Bolts.podspec.json"
        );
        assert_eq!(
            cocoapods_spec_url("https://cdn.cocoapods.org", "2fa", "1.0.0"),
            "https://cdn.cocoapods.org/0-9/2fa/1.0.0/2fa.podspec.json"
        );
    }

    #[test]
    fn spec_dependencies_map_to_edges() {
        let spec: Value = serde_json::from_str(
            r#"{"name":"Firebase","version":"10.18.0","dependencies":{"FirebaseCore":["= 10.18.0"],"nanopb":["~> 2.30908.0"]}}"#,
        )
        .unwrap();
        let deps = CocoaPodsProtocol::spec_dependencies(&spec);
        assert!(deps.contains(&("FirebaseCore".to_string(), "= 10.18.0".to_string())));
        assert!(deps.contains(&("nanopb".to_string(), "~> 2.30908.0".to_string())));
    }
}
