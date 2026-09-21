//! React Native layered resolve/install (JS / Android / iOS tiers).
//! Resolve/install phân tầng React Native (tier JS / Android / iOS).
//!
//! One React Native project spans three dependency universes; each tier
//! keeps its OWN ecosystem tag in mgc.lock.
//! - JS tier → the web adapter's npm pipeline (delegated wholesale: the
//!   same resolve/install engine the web core uses — the JS lane's lock
//!   entries are written by that pipeline).
//! - Android tier → `gradle.lockfile` pins are EXACT Maven coordinates:
//!   the native Maven engine resolves the POM graph and the install lane
//!   materializes the m2 layout (ecosystem=maven).
//! - iOS tier → `Podfile.lock` (CocoaPods' own closure + `SPEC
//!   CHECKSUMS:`): every pod spec is fetched from the CocoaPods CDN and
//!   its sha1 must match the lock checksum. A project with a Podfile but
//!   no Podfile.lock fails closed with `Unsupported` (never a silent empty
//!   iOS graph).
//!
//! Một project React Native trải ba vũ trụ dependency; mỗi tier giữ
//! ecosystem RIÊNG trong mgc.lock.
//! - Tier JS → pipeline npm của adapter web (ủy quyền trọn: cùng engine
//!   resolve/install core web dùng — entry lock của lane JS do pipeline đó
//!   ghi).
//! - Tier Android → pin `gradle.lockfile` là toạ độ Maven CHÍNH XÁC:
//!   engine Maven native resolve graph POM và lane install materialize
//!   layout m2 (ecosystem=maven).
//! - Tier iOS → `Podfile.lock` (bao đóng của chính CocoaPods + `SPEC
//!   CHECKSUMS:`): mọi spec pod tải từ CDN CocoaPods và sha1 phải khớp
//!   checksum trong lock. Project có Podfile nhưng thiếu Podfile.lock thì
//!   fail-closed bằng `Unsupported` (không bao giờ graph iOS rỗng âm thầm).

use mgc_lib_adapter::native::engine::resolve_with_protocol;
use mgc_lockfile::EcosystemTag;
use mgc_resolver::protocols::reactnative::{
    CocoaPodsProtocol, PODSPEC_SHA1_MARKER_PREFIX, RN_TIER_MARKER_PREFIX, cocoapods_spec_url,
    collapse_gradle_pins, parse_gradle_lockfile, parse_podfile_lock, pod_root,
};
use mgc_resolver::protocols::{MavenProtocol, RegistryProtocol};
use mgc_types::adapter::PackageAdapter;
use mgc_types::capabilities::{ContentStoreProvider, DependencyResolver, unsupported_capability};
use mgc_types::{
    DependencySpec, Ecosystem, Manifest, MgError, MgResult, PackageId, PackageName, ResolvedGraph,
    ResolvedPackage, Version, VersionRange,
};
use std::collections::HashSet;
use std::path::Path;

/// A layered React Native resolution: the merged graph plus the non-JS
/// tier lock entries (the JS lane owns its own mgc.lock entries).
/// Một kết quả resolve phân tầng React Native: graph đã gộp cộng entry
/// lock của các tier không-JS (lane JS tự sở hữu entry mgc.lock của nó).
pub struct RNResolution {
    pub graph: ResolvedGraph,
    pub lock_packages: Vec<mgc_lockfile::Package>,
}

/// Resolve all three tiers.
/// Resolve cả ba tier.
pub async fn resolve_rn_layers(manifest: &Manifest, project_root: &Path) -> MgResult<RNResolution> {
    let mut graph = ResolvedGraph::empty();
    let mut lock_packages = Vec::new();

    // ── JS tier: web adapter (npm pipeline) ──
    let web = mgc_web_adapter::WebAdapter::new()
        .map_err(|e| MgError::Other(format!("web adapter construction failed: {e}")))?;
    // P0/F6: arm from THIS operation's project (a fresh adapter starts
    // unarmed — an RN project with [security] policy must still filter).
    // (Nạp cổng tuổi từ project của operation này.)
    web.arm_age_gate_for(project_root)?;
    let js_graph = DependencyResolver::resolve(&web, manifest).await?;
    graph.packages.extend(js_graph.packages);

    // ── Android tier: gradle.lockfile → native Maven engine ──
    let gradle_lock = project_root.join("gradle.lockfile");
    if gradle_lock.is_file() {
        let text = std::fs::read_to_string(&gradle_lock)
            .map_err(|e| MgError::Other(format!("read gradle.lockfile: {e}")))?;
        let pins = collapse_gradle_pins(&parse_gradle_lockfile(&text)?)?;
        if !pins.is_empty() {
            let mut android_manifest = Manifest::new("rn-android", Ecosystem::App);
            for (coordinate, version) in &pins {
                let name = PackageName::new(coordinate.clone()).map_err(|e| {
                    MgError::Other(format!("invalid Maven coordinate {coordinate}: {e}"))
                })?;
                // gradle.lockfile pins are EXACT — Maven's own bare-version
                // form is a caret stream, so the pin rides as `[v]`.
                // (Pin gradle.lockfile là CHÍNH XÁC — dạng version trần của
                // Maven là caret stream, nên pin đi dạng `[v]`.)
                let range = VersionRange::parse(&format!("[{version}]"))?;
                android_manifest.add_dep(DependencySpec::new(name, range), false, false, false);
            }
            let maven = MavenProtocol::from_env();
            let resolution = resolve_with_protocol(
                &maven,
                EcosystemTag::Maven,
                "maven://repo.maven.apache.org",
                &android_manifest,
            )
            .await?;
            graph.packages.extend(resolution.graph.packages);
            for pkg in resolution.lock_packages {
                lock_packages.push(tag_tier(pkg, "android"));
            }
        }
    }

    // ── iOS tier: Podfile.lock closure + CDN sha1 verification ──
    let podfile_lock = project_root.join("Podfile.lock");
    if podfile_lock.is_file() {
        let text = std::fs::read_to_string(&podfile_lock)
            .map_err(|e| MgError::Other(format!("read Podfile.lock: {e}")))?;
        let lock = parse_podfile_lock(&text);
        let cocoapods = CocoaPodsProtocol::from_env();
        for pod in &lock.pods {
            let Some(expected) = lock.checksum_for(&pod.name) else {
                return Err(MgError::Integrity(format!(
                    "Podfile.lock has no SPEC CHECKSUMS entry for pod '{}' — the spec cannot be verified (fail-closed)",
                    pod.name
                )));
            };
            let verified = cocoapods
                .fetch_verified_spec(&pod.name, &pod.version, expected)
                .await?;
            let dep_names: Vec<String> = CocoaPodsProtocol::spec_dependencies(&verified.spec)
                .into_iter()
                .map(|(dep, _)| pod_root(&dep).to_string())
                // Edges only to pods present in the lock closure — a spec
                // dependency outside it is a local/development pod the lock
                // does not pin.
                // (Cạnh chỉ tới pod có trong bao đóng lock — dep của spec
                // ngoài bao đóng là pod local/development mà lock không
                // ghim.)
                .filter(|root| lock.pods.iter().any(|p| &p.name == root))
                .collect();
            let spec_url = cocoapods_spec_url(cocoapods.cdn(), &pod.name, &pod.version);
            let id = PackageId::new(
                PackageName::new(pod.name.clone())
                    .map_err(|e| MgError::Other(format!("invalid pod name {}: {e}", pod.name)))?,
                Version::parse(&pod.version).map_err(|e| {
                    MgError::Other(format!("invalid pod version {}: {e}", pod.version))
                })?,
            );
            graph.packages.push(ResolvedPackage {
                id: id.clone(),
                integrity: String::new(),
                tarball_url: spec_url.clone(),
                deps: Vec::new(),
                peer_deps: Vec::new(),
                direct: false,
                dev: false,
            });
            let mut markers = vec![
                format!("{PODSPEC_SHA1_MARKER_PREFIX}{}", verified.sha1),
                format!("{RN_TIER_MARKER_PREFIX}ios"),
            ];
            markers.push(format!("checked-deps:{}", dep_names.len()));
            lock_packages.push(mgc_lockfile::Package {
                name: pod.name.clone(),
                version: pod.version.clone(),
                resolved: spec_url.clone(),
                // Pod specs are JSON documents, not archives — the spec sha1
                // is the integrity source and rides the marker.
                // (Spec pod là tài liệu JSON, không phải archive — sha1 spec
                // là nguồn toàn vẹn và nằm trong marker.)
                integrity: String::new(),
                dependencies: dep_names,
                ecosystem: EcosystemTag::CocoaPods,
                registry: Some(cocoapods_registry_tag(cocoapods.cdn())),
                artifact: Some(mgc_lockfile::ArtifactRef {
                    url: spec_url,
                    size_bytes: None,
                    content_hash: String::new(),
                    downloaded_from: cocoapods_registry_tag(cocoapods.cdn()),
                }),
                provenance: Some(mgc_lockfile::Provenance {
                    source_kind: mgc_lockfile::SOURCE_KIND_NATIVE_RESOLVE.to_string(),
                    tool: Some("mgc-resolver".to_string()),
                    imported_from: None,
                }),
                markers: Some(markers),
                ..Default::default()
            });
        }
    } else if project_root.join("Podfile").is_file() {
        // Fail closed: a Podfile without Podfile.lock means the iOS tier
        // has no verifiable closure — silently skipping would fake a
        // complete resolution (V1.2: skip paths must never read as pass).
        // (Fail-closed: Podfile thiếu Podfile.lock nghĩa là tier iOS không
        // có bao đóng kiểm chứng được — bỏ qua âm thầm sẽ giả vờ resolve
        // đầy đủ.)
        return Err(unsupported_capability(
            "app",
            "resolve",
            "React Native iOS tier has a Podfile but no Podfile.lock — run `pod install` to produce the lock, then re-run resolve",
        ));
    }

    Ok(RNResolution {
        graph,
        lock_packages,
    })
}

/// Install the RN tiers from a resolved graph: Android pins go through the
/// native Maven install lane (m2 layout), the JS tier is delegated to the
/// web adapter's npm install pipeline, and the iOS tier needs no artifact
/// download — every pod spec was verified against the Podfile.lock
/// checksum at resolve time (the checkouts are owned by CocoaPods itself).
/// Install các tier RN từ graph đã resolve: pin Android đi qua lane install
/// Maven native (layout m2), tier JS ủy quyền cho pipeline install npm của
/// adapter web, và tier iOS không cần tải artifact — mọi spec pod đã được
/// xác minh theo checksum Podfile.lock lúc resolve (checkout do chính
/// CocoaPods giữ).
pub async fn install_rn_layers(
    graph: &ResolvedGraph,
    lock_packages: &[mgc_lockfile::Package],
    project_root: &Path,
    opts: mgc_types::adapter::InstallOptions,
) -> MgResult<mgc_types::adapter::InstallSummary> {
    let maven_names: HashSet<&str> = lock_packages
        .iter()
        .filter(|p| p.ecosystem == EcosystemTag::Maven)
        .map(|p| p.name.as_str())
        .collect();
    let pod_names: HashSet<&str> = lock_packages
        .iter()
        .filter(|p| p.ecosystem == EcosystemTag::CocoaPods)
        .map(|p| p.name.as_str())
        .collect();

    let started = std::time::Instant::now();
    let mut added: Vec<PackageId> = Vec::new();

    // Android tier: native Maven (jar + pom verified, m2 layout).
    // (Tier Android: Maven native (jar + pom đã xác minh, layout m2).)
    let android_graph = subset(graph, |name| maven_names.contains(name));
    if !android_graph.packages.is_empty() {
        added.extend(install_maven_subset(&android_graph).await?.added);
    }

    // iOS tier: verification happened at resolve; the pod checkouts belong
    // to CocoaPods. Recorded, never silently re-downloaded.
    // (Tier iOS: xác minh đã xảy ra lúc resolve; checkout pod thuộc
    // CocoaPods. Ghi nhận, không bao giờ tải lại âm thầm.)
    for pkg in graph.packages.iter() {
        if pod_names.contains(pkg.id.name_str()) {
            added.push(pkg.id.clone());
        }
    }

    // JS tier: delegate the remaining packages to the web adapter.
    // (Tier JS: ủy quyền các package còn lại cho adapter web.)
    let js_graph = subset(graph, |name| {
        !maven_names.contains(name) && !pod_names.contains(name)
    });
    let mut web_summary = None;
    if !js_graph.packages.is_empty() {
        let web = mgc_web_adapter::WebAdapter::new()
            .map_err(|e| MgError::Other(format!("web adapter construction failed: {e}")))?;
        let summary = ContentStoreProvider::install(&web, &js_graph, project_root, opts).await?;
        added.extend(summary.added.iter().cloned());
        web_summary = Some(summary);
    }

    Ok(mgc_types::adapter::InstallSummary {
        added,
        bytes_from_cache: web_summary.as_ref().map_or(0, |s| s.bytes_from_cache),
        duration_ms: started.elapsed().as_millis() as u64,
        // Native Maven bytes land in the CAS; the npm lane reports the same
        // mgc-store mode.
        // (Byte Maven native vào CAS; lane npm báo cùng chế độ mgc-store.)
        cache_mode: web_summary
            .map(|s| s.cache_mode)
            .unwrap_or(mgc_types::adapter::InstallCacheMode::MgCStore),
    })
}

/// Native Maven install of a graph subset (mirrors the lib adapter lane):
/// jar + POM downloaded, verified, CAS-imported and materialized into
/// `{m2}/repository/…`.
/// Install Maven native cho một tập graph (giống lane lib adapter): jar +
/// POM tải, xác minh, import CAS và materialize vào `{m2}/repository/…`.
async fn install_maven_subset(
    graph: &ResolvedGraph,
) -> MgResult<mgc_types::adapter::InstallSummary> {
    let started = std::time::Instant::now();
    let protocol = MavenProtocol::from_env();
    let m2_root = maven_store_root()?;
    let store = mgc_store::ContentStore::new(mgc_store::default_store_root())
        .map_err(|e| MgError::Store(e.to_string()))?;
    let mut added = Vec::with_capacity(graph.packages.len());
    for pkg in &graph.packages {
        let entry = mgc_resolver::protocols::ResolvedEntry {
            name: pkg.id.name_str().to_string(),
            version: pkg.id.version().to_string(),
            deps: Vec::new(),
            artifact_url: pkg.tarball_url.clone(),
            sha256: pkg
                .integrity
                .strip_prefix("sha256-")
                .unwrap_or("")
                .to_string(),
            extra_markers: Vec::new(),
        };
        let jar = protocol.download(&entry).await?;
        protocol.verify(&entry, &jar)?;
        let (group, artifact) = MavenProtocol::split_coordinate(&entry.name)?;
        let pom = protocol
            .download_pom(&group, &artifact, &entry.version)
            .await?;
        store
            .import_bytes(&jar)
            .map_err(|e| MgError::Store(e.to_string()))?;
        store
            .import_bytes(&pom)
            .map_err(|e| MgError::Store(e.to_string()))?;
        protocol.materialize(&entry, &jar, &pom, &m2_root)?;
        added.push(pkg.id.clone());
    }
    Ok(mgc_types::adapter::InstallSummary {
        added,
        bytes_from_cache: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        cache_mode: mgc_types::adapter::InstallCacheMode::MgCStore,
    })
}

/// The mgc-managed Maven local repository (`~/.magicore/store/maven`;
/// `MGC_MAVEN_STORE_ROOT` overrides for tests/shared placement).
/// Local repository Maven do mgc quản (`~/.magicore/store/maven`;
/// `MGC_MAVEN_STORE_ROOT` ghi đè cho test/vị trí dùng chung).
fn maven_store_root() -> MgResult<std::path::PathBuf> {
    let root = match std::env::var("MGC_MAVEN_STORE_ROOT")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        Some(over) => std::path::PathBuf::from(over),
        None => {
            let home =
                dirs::home_dir().ok_or_else(|| MgError::Other("no home directory".to_string()))?;
            home.join(".magicore").join("store").join("maven")
        }
    };
    std::fs::create_dir_all(&root)
        .map_err(|e| MgError::Other(format!("create maven store root: {e}")))?;
    Ok(root)
}

/// A graph with only the packages whose name passes `keep`.
/// Graph chỉ giữ package có tên thoả `keep`.
fn subset(graph: &ResolvedGraph, keep: impl Fn(&str) -> bool) -> ResolvedGraph {
    ResolvedGraph {
        packages: graph
            .packages
            .iter()
            .filter(|p| keep(p.id.name_str()))
            .cloned()
            .collect(),
    }
}

/// Attach the tier marker to a native lock entry.
/// Gắn marker tier vào một entry lock native.
fn tag_tier(mut pkg: mgc_lockfile::Package, tier: &str) -> mgc_lockfile::Package {
    let mut markers = pkg.markers.take().unwrap_or_default();
    if !markers
        .iter()
        .any(|m| m == &format!("{RN_TIER_MARKER_PREFIX}{tier}"))
    {
        markers.push(format!("{RN_TIER_MARKER_PREFIX}{tier}"));
    }
    pkg.markers = Some(markers);
    pkg
}

/// `cocoapods://{host}` provenance tag for lock entries.
/// Tag provenance `cocoapods://{host}` cho entry lock.
fn cocoapods_registry_tag(cdn: &str) -> String {
    let host = cdn
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(cdn)
        .split('/')
        .next()
        .unwrap_or(cdn)
        .to_string();
    format!("cocoapods://{host}")
}
