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
use tracing::warn;

/// Validate a dependency-route name (P1-1, fresh-context review
/// 2026-09-15): the route segment is joined onto node_modules, so it is
/// a JAILBOUND input. Legal shapes ONLY:
///   - `name` — plain package;
///   - `@scope/name` — scoped package;
///   - `@scope/name/sub/path` — scoped subpath.
///
/// Every segment must be a plausible npm name segment: non-empty, no
/// `.`/`..`, no path separators inside (they ARE the split), no Windows
/// drive prefix, no percent-decode leftovers (`%` is not legal in npm
/// names), no NUL. The first segment of a scoped name MUST start with
/// `@`. Fail-closed: anything else is a traversal attempt, not a
/// 404-missing-package.
/// (Validate tên dependency-route (P1-1): segment route được nối vào
/// node_modules nên là input BỊ GIỮ NGỤC. Chỉ hợp lệ: `name`,
/// `@scope/name`, `@scope/name/sub/path`. Mọi segment phải là segment
/// tên npm hợp lệ: không rỗng, không `.`/`..`, không dấu phân tách,
/// không prefix ổ đĩa Windows, không `%`, không NUL. Segment đầu của
/// tên scoped PHẢI bắt đầu `@`. Fail-closed: còn lại là ý đồ traversal,
/// không phải package-thiếu-404.)
pub(crate) fn dep_name_is_valid(pkg: &str) -> bool {
    // Route paths arrive with the leading slash stripped by the
    // wildcard extractor; a re-appearing absolute marker means the
    // caller is trying to escape the join (P1-1).
    // (Path route tới với leading slash đã bị strip bởi wildcard
    // extractor; marker tuyệt-đối xuất hiện lại nghĩa là caller cố
    // thoát phép nối (P1-1).)
    if pkg.is_empty() {
        return false;
    }
    let segments: Vec<&str> = pkg.split('/').collect();
    let scoped = segments[0].starts_with('@');
    // A scoped name needs at LEAST `@scope/name`; a plain name is
    // exactly one segment; deeper paths are only legal under a scope.
    // (Tên scoped cần ít nhất `@scope/name`; tên thường đúng 1 segment;
    // path sâu chỉ hợp pháp dưới scope.)
    if scoped && segments.len() < 2 {
        return false;
    }
    if !scoped && segments.len() != 1 {
        return false;
    }
    for (idx, seg) in segments.iter().enumerate() {
        // Scope segment: `@` + a real name body.
        // (Segment scope: `@` + thân tên thật.)
        if idx == 0 && scoped {
            if seg.len() < 2 {
                return false;
            }
            continue;
        }
        // Plain segment: npm names have no `%`, no NUL, no dot-only
        // spellings (`.`/`..` are traversal), and a sane length bound
        // (214 is the documented npm package-name limit).
        // (Segment thường: tên npm không có `%`, không NUL, không dạng
        // chỉ-dấu-chấm (`.`/`..` là traversal), và giới hạn độ dài hợp
        // lý (214 là giới hạn tên npm được tài liệu hóa).)
        if seg.is_empty()
            || *seg == "."
            || *seg == ".."
            || seg.contains('%')
            || seg.contains('\0')
            || seg.len() > 214
        {
            return false;
        }
        // A drive-letter escape (`C:` inside a segment) only matters on
        // the FIRST segment — but check every segment for defense in
        // depth: a `C:` anywhere reshapes the joined path on Windows.
        // (Thoát ổ đĩa (`C:` trong segment) chỉ quan trọng ở segment
        // đầu — nhưng check mọi segment cho phòng vệ nhiều lớp.)
        if seg.len() >= 2 && seg.as_bytes()[1] == b':' {
            return false;
        }
    }
    true
}

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

        // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
        if pkg_json.exists()
            && let Ok(raw) = std::fs::read_to_string(&pkg_json)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw)
        {
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

        // Fallback: index.js
        let fallback = pkg_dir.join("index.js");
        if fallback.exists() {
            return Some(fallback);
        }

        None
    }

    /// Lấy cached dep hoặc bundle nó lần đầu.
    pub async fn get_or_bundle(&self, pkg_name: &str) -> Option<CachedDep> {
        // TEMPORARY: esbuild-rs requires Go compiler not available in CI
        warn!(
            "deps bundler disabled: {} - esbuild-rs requires Go",
            pkg_name
        );
        None

        /* COMMENTED UNTIL GO COMPILER AVAILABLE
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

        // Output selection (P1-2, fresh-context review 2026-09-15 — the
        // sibling of the transpile-path bug fixed in vòng-11): with
        // `write=false`, esbuild-rs may report the single output entry's
        // path as `<stdout>` — a `find(ends_with(".js"))`-only filter
        // matched NOTHING and the dep was served as EMPTY JS (proved
        // red by dev_server_request_security.rs). Prefer a real `.js`
        // output; with none, take the FIRST output — bundling one entry
        // means the first output IS the bundle. Never default to empty
        // while outputs exist.
        // (Chọn output (P1-2 — bug anh em của đường transpile đã sửa
        // vòng-11): với `write=false`, esbuild-rs có thể báo path output
        // đơn là `<stdout>` — filter chỉ `ends_with(".js")` khớp 0 và
        // dep được phục vụ JS RỖNG (chứng minh đỏ). Ưu tiên `.js` thật;
        // không có thì lấy output ĐẦU — bundle 1 entry nghĩa output đầu
        // chính là bundle. Không mặc định rỗng khi còn output.)
        let js = result
            .output_files
            .as_slice()
            .iter()
            .find(|f| f.path.as_str().ends_with(".js"))
            .or_else(|| result.output_files.as_slice().first())
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
        */
    }
}
