/// deps_bundler.rs — Dependency Pre-bundler cho MgDevServer
///
/// Cơ chế:
///   1. Khi browser request `/@magicore/deps/react`, `DepsCache::get_or_bundle("react")` được gọi.
///   2. Nếu đã có trong in-memory cache → trả về ngay (0ms).
///   3. Nếu không, dùng esbuild_rs để bundle dependency đó thành 1 file ESM đơn lẻ.
///   4. Lưu kết quả vào in-memory cache (hoặc trên disk ở CompiledCache nếu muốn dùng chung giữa sessions).
///
/// Thiết kế:
///   - DepsCache là Arc<RwLock<HashMap>> để nhiều request đọc song song không block nhau.
///   - Mỗi package được bundle 1 lần duy nhất cho toàn session.
///   - CSS được xử lý riêng (trả về empty nếu dependency không có CSS).
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;
use tracing::{debug, warn};

#[derive(Clone, Default)]
pub struct DepsCache {
    /// Map: package_name → bundled JS content
    inner: Arc<RwLock<HashMap<String, CachedDep>>>,
    pub node_modules: PathBuf,
}

#[derive(Clone, Debug)]
pub struct CachedDep {
    pub js: String,
    pub css: Option<String>,
}

impl DepsCache {
    pub fn new(node_modules: PathBuf) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            node_modules,
        }
    }

    /// Tìm entry point cho một package trong node_modules.
    /// Ưu tiên: browser > module > main > index.js
    fn resolve_package_entry(&self, pkg_name: &str) -> Option<PathBuf> {
        let pkg_dir = self.node_modules.join(pkg_name);
        let pkg_json = pkg_dir.join("package.json");

        if pkg_json.exists() {
            if let Ok(raw) = std::fs::read_to_string(&pkg_json) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                    // Thử browser field trước (UMD/browser builds)
                    for field in ["browser", "module", "main"] {
                        if let Some(s) = val.get(field).and_then(|v| v.as_str()) {
                            let entry = pkg_dir.join(s);
                            if entry.exists() {
                                return Some(entry);
                            }
                        }
                    }
                }
            }
        }

        // Fallback: index.js
        let fallback = pkg_dir.join("index.js");
        if fallback.exists() {
            return Some(fallback);
        }

        None
    }

    /// Lấy cached dep hoặc bundle nó lần đầu.
    pub async fn get_or_bundle(&self, pkg_name: &str) -> Option<CachedDep> {
        // 1. Fast path: đọc từ in-memory cache (không lock write)
        {
            let cache = self.inner.read().await;
            if let Some(dep) = cache.get(pkg_name) {
                debug!("deps-cache hit: {}", pkg_name);
                return Some(dep.clone());
            }
        }

        // 2. Bundle: tìm entry point
        let entry = self.resolve_package_entry(pkg_name)?;
        debug!("bundling dep: {} → {}", pkg_name, entry.display());

        let working_dir = self
            .node_modules
            .parent()
            .unwrap_or(&self.node_modules)
            .to_string_lossy()
            .to_string();

        // 3. Gọi esbuild: bundle=true, format=ESModule
        let mut builder = esbuild_rs::BuildOptionsBuilder::new();
        builder.entry_points = vec![entry.to_string_lossy().to_string()];
        builder.bundle = true;
        builder.abs_working_dir = working_dir;
        builder.platform = esbuild_rs::Platform::Browser;
        builder.format = esbuild_rs::Format::ESModule;
        builder.write = false;
        builder.resolve_extensions = vec![
            ".js".to_string(),
            ".ts".to_string(),
            ".jsx".to_string(),
            ".tsx".to_string(),
            ".json".to_string(),
            ".css".to_string(),
        ];
        builder.main_fields = vec![
            "browser".to_string(),
            "module".to_string(),
            "main".to_string(),
        ];
        // Minify nhẹ để giảm kích thước nhưng giữ readable cho dev
        builder.minify_whitespace = false;
        builder.minify_syntax = true;
        builder.minify_identifiers = false;

        let options = builder.build();
        let result = esbuild_rs::build(options).await;

        if !result.errors.as_slice().is_empty() {
            let msgs: Vec<String> = result
                .errors
                .as_slice()
                .iter()
                .map(|e| e.to_string())
                .collect();
            warn!(
                "failed to pre-bundle dep '{}': {}",
                pkg_name,
                msgs.join("; ")
            );
            return None;
        }

        let js = result
            .output_files
            .as_slice()
            .iter()
            .find(|f| f.path.as_str().ends_with(".js"))
            .map(|f| f.data.as_str().to_string())
            .unwrap_or_default();

        let css = result
            .output_files
            .as_slice()
            .iter()
            .find(|f| f.path.as_str().ends_with(".css"))
            .map(|f| f.data.as_str().to_string());

        let dep = CachedDep { js, css };

        // 4. Lưu vào cache
        {
            let mut cache = self.inner.write().await;
            cache.insert(pkg_name.to_string(), dep.clone());
        }

        Some(dep)
    }
}
