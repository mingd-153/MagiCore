/// dev_server.rs — MgDevServer: Native ESM Dev Server
///
/// # Kiến trúc (Native ESM + Dependency Pre-bundling)
///
/// Giống Vite, nhưng viết 100% bằng Rust, không cần Node.js:
///
/// ```
/// Browser request                 MgDevServer xử lý
/// ─────────────────────────────────────────────────────
/// GET /                        → serve index.html (inject HMR script)
/// GET /@megagate/hmr.js        → serve HMR client script
/// GET /@megagate/hmr           → WebSocket endpoint
/// GET /@megagate/deps/react    → DepsCache: bundle react từ node_modules
/// GET /src/App.tsx             → CompiledCache → esbuild (transpile only)
///                                + rewrite bare imports → /@megagate/deps/…
/// GET /src/App.css             → serve as text/css
/// GET /public/logo.png         → serve static
/// ```
///
/// # Import Rewriting
///
/// ```ts
/// // Input (App.tsx):
/// import React from 'react';
/// import { motion } from 'framer-motion';
///
/// // Output (sau khi rewrite):
/// import React from '/@megagate/deps/react';
/// import { motion } from '/@megagate/deps/framer-motion';
/// ```
///
/// # Compile Cache
///
/// Mỗi file .ts/.tsx/.jsx được hash bằng Blake3 → tra CompiledCache.
/// Nếu hit: serve ngay (~0ms). Nếu miss: esbuild transpile → lưu vào cache.
/// Cache được dùng chung giữa tất cả project trên máy (global ~/.megagate store).

use crate::bundler::deps_bundler::DepsCache;
use crate::bundler::hmr::{hmr_ws_handler, HmrManager, HMR_CLIENT_SCRIPT};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};

use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{debug, error, info};

// Regex-based import rewriter — khớp cả ESM static và dynamic imports
// Ví dụ: import x from 'react' → import x from '/@megagate/deps/react'
//        import('react') → import('/@megagate/deps/react')
static BARE_IMPORT_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();

fn bare_import_re() -> &'static regex::Regex {
    BARE_IMPORT_RE.get_or_init(|| {
        // Khớp:
        //   from 'pkg'          from "pkg"
        //   import 'pkg'        import "pkg"
        //   import('pkg')       import("pkg")
        //   export from 'pkg'
        // Đường dẫn tương đối/absolute/@megagate được lọc trong rewrite_imports()
        regex::Regex::new(
            r#"(?P<keyword>from|import(?:\s*\()|import|export\s+(?:\*|(?:\{[^}]*\}))\s+from)\s*['"](?P<pkg>[^'"]+)['"]"#
        ).expect("invalid bare import regex")
    })
}

/// Rewrite bare imports trong JS/TS output thành /@megagate/deps/ paths.
fn rewrite_imports(js: &str) -> String {
    let re = bare_import_re();
    re.replace_all(js, |caps: &regex::Captures| {
        let keyword = &caps["keyword"];
        let pkg = &caps["pkg"];
        // Bỏ qua đường dẫn tương đối, absolute, và deps đã rewrite
        if pkg.starts_with("./")
            || pkg.starts_with("../")
            || pkg.starts_with('/')
            || pkg.starts_with("@megagate/")
        {
            return caps.get(0).unwrap().as_str().to_string();
        }
        // Xử lý scoped packages: @org/pkg → @org/pkg (giữ nguyên, chỉ thay separator)
        // Ví dụ: @tanstack/react-query → /@megagate/deps/@tanstack/react-query
        let is_dynamic = keyword.starts_with("import(");
        if is_dynamic {
            format!("import('/@megagate/deps/{}')", pkg)
        } else {
            format!("{} '/@megagate/deps/{}'", keyword.trim_end(), pkg)
        }
    })
    .to_string()
}

pub struct DevServerConfig {
    pub root: PathBuf,
    pub entry: PathBuf,
    pub host: String,
    pub port: u16,
}

#[derive(Clone)]
struct ServerState {
    config: Arc<DevServerConfig>,
    hmr_manager: Arc<HmrManager>,
    /// Pre-bundling cache cho node_modules (/@megagate/deps/*)
    deps_cache: DepsCache,
}

impl axum::extract::FromRef<ServerState> for Arc<HmrManager> {
    fn from_ref(state: &ServerState) -> Self {
        state.hmr_manager.clone()
    }
}

pub struct MgDevServer {
    config: DevServerConfig,
}

impl MgDevServer {
    pub fn new(config: DevServerConfig) -> Self {
        Self { config }
    }

    pub async fn serve(&self) -> anyhow::Result<()> {
        let hmr_manager = Arc::new(HmrManager::new());
        // Watch src/ dir, fallback về root nếu không có src/
        let src_dir = self.config.root.join("src");
        if src_dir.exists() {
            let _watcher = hmr_manager.watch_dir(&src_dir);
        } else {
            let _watcher = hmr_manager.watch_dir(&self.config.root);
        }

        let node_modules = self.config.root.join("node_modules");
        let deps_cache = DepsCache::new(node_modules);

        let state = ServerState {
            config: Arc::new(DevServerConfig {
                root: self.config.root.clone(),
                entry: self.config.entry.clone(),
                host: self.config.host.clone(),
                port: self.config.port,
            }),
            hmr_manager,
            deps_cache,
        };

        let app = Router::new()
            // Trang chủ
            .route("/", get(serve_index))
            .route("/index.html", get(serve_index))
            // HMR client + WebSocket
            .route("/@megagate/hmr.js", get(serve_hmr_client))
            .route("/@megagate/hmr", get(hmr_ws_handler))
            // Dependency pre-bundling: /@megagate/deps/{package}
            .route("/@megagate/deps/*pkg", get(serve_dep))
            // Source files: transpile on-the-fly
            .route("/*path", get(serve_source_or_static))
            .with_state(state);

        let bind_host = if self.config.host == "localhost" {
            "127.0.0.1"
        } else {
            self.config.host.as_str()
        };
        let bind_addr = format!("{}:{}", bind_host, self.config.port);
        info!(
            "🚀 MgDevServer (Native ESM) đang lắng nghe tại http://{}:{}",
            self.config.host,
            self.config.port
        );

        let listener = TcpListener::bind(&bind_addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// Phục vụ index.html với HMR script được inject.
async fn serve_index(State(state): State<ServerState>) -> impl IntoResponse {
    let index_path = state.config.root.join("index.html");
    let mut html = match std::fs::read_to_string(&index_path) {
        Ok(content) => content,
        Err(_) => {
            let title = state
                .config
                .root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("MegaGate App");
            // Tạo index.html mặc định nếu không có
            let relative_entry = state
                .config
                .entry
                .strip_prefix(&state.config.root)
                .unwrap_or(&state.config.entry)
                .to_string_lossy()
                .replace('\\', "/");
            format!(
                r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>{title}</title>
</head>
<body>
  <div id="root"></div>
  <script type="module" src="/{relative_entry}"></script>
</body>
</html>"#
            )
        }
    };

    // Inject HMR script vào <head>
    let hmr_tag = r#"<script type="module" src="/@megagate/hmr.js"></script>"#;
    let hmr_tag = format!("{}\n", hmr_tag);
    if let Some(idx) = html.find("</head>") {
        html.insert_str(idx, &hmr_tag);
    } else {
        html.push_str(&hmr_tag);
    }

    Html(html)
}

/// Phục vụ HMR client script.
async fn serve_hmr_client() -> impl IntoResponse {
    Response::builder()
        .header(header::CONTENT_TYPE, "application/javascript")
        .body(Body::from(HMR_CLIENT_SCRIPT))
        .unwrap()
}

/// Phục vụ dependency đã pre-bundled.
/// Route: GET /@megagate/deps/*pkg
/// Ví dụ: /@megagate/deps/react → bundle react từ node_modules/react
///         /@megagate/deps/@tanstack/react-query → scoped package
async fn serve_dep(
    Path(pkg): Path<String>,
    State(state): State<ServerState>,
) -> impl IntoResponse {
    debug!("deps request: {}", pkg);

    match state.deps_cache.get_or_bundle(&pkg).await {
        Some(dep) => {
            // Nếu CSS được tạo kèm, inject loader
            let mut js = dep.js.clone();
            if let Some(css) = &dep.css {
                if !css.is_empty() {
                    // Inject CSS dưới dạng CSSStyleSheet API (modern browsers)
                    let css_escaped = css.replace('`', "\\`");
                    let css_injector = format!(
                        r#"
// [MgDevServer] Injected CSS from {pkg}
(function() {{
  const sheet = new CSSStyleSheet();
  sheet.replaceSync(`{css_escaped}`);
  document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
}})();
"#
                    );
                    js.push_str(&css_injector);
                }
            }

            Response::builder()
                .header(header::CONTENT_TYPE, "application/javascript; charset=utf-8")
                .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
                .body(Body::from(js))
                .unwrap_or_else(|_| {
                    (StatusCode::INTERNAL_SERVER_ERROR, "Server error").into_response()
                })
        }
        None => {
            let err_js = format!(
                r#"console.error('[MgDevServer] Failed to bundle dependency: {pkg}');"#
            );
            Response::builder()
                .header(header::CONTENT_TYPE, "application/javascript")
                .status(StatusCode::NOT_FOUND)
                .body(Body::from(err_js))
                .unwrap_or_else(|_| (StatusCode::NOT_FOUND, "").into_response())
        }
    }
}

/// Phục vụ source file (TS/TSX/JSX/JS → transpile on-the-fly) hoặc static file.
async fn serve_source_or_static(
    Path(path): Path<String>,
    State(state): State<ServerState>,
) -> impl IntoResponse {
    // Chuẩn hoá path: loại bỏ leading slash nếu có
    let rel_path = path.trim_start_matches('/');

    // Thử tìm file trong src/, rồi root
    let candidates = [
        state.config.root.join(rel_path),
        state.config.root.join("public").join(rel_path),
    ];

    let file_path = candidates.iter().find(|p| p.exists() && p.is_file());

    let Some(file_path) = file_path else {
        return (StatusCode::NOT_FOUND, format!("Not found: /{}", rel_path)).into_response();
    };

    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match ext {
        // ─── TypeScript / JSX: transpile on-the-fly ───────────────────────
        "ts" | "tsx" | "jsx" | "js" | "mjs" => {
            serve_transpiled(file_path, &state).await
        }
        // ─── CSS: serve trực tiếp ─────────────────────────────────────────
        "css" => match tokio::fs::read_to_string(file_path).await {
            Ok(content) => Response::builder()
                .header(header::CONTENT_TYPE, "text/css; charset=utf-8")
                .header(header::CACHE_CONTROL, "no-cache")
                .body(Body::from(content))
                .unwrap_or_else(|_| {
                    (StatusCode::INTERNAL_SERVER_ERROR, "").into_response()
                }),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read CSS").into_response(),
        },
        // ─── Static assets: serve với mime-type đúng ─────────────────────
        _ => match tokio::fs::read(file_path).await {
            Ok(bytes) => {
                let mime = mime_guess::from_path(file_path).first_or_octet_stream();
                Response::builder()
                    .header(header::CONTENT_TYPE, mime.as_ref())
                    .header(header::CACHE_CONTROL, "public, max-age=86400")
                    .body(Body::from(bytes))
                    .unwrap_or_else(|_| {
                        (StatusCode::INTERNAL_SERVER_ERROR, "Server error").into_response()
                    })
            }
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file").into_response(),
        },
    }
}

/// Transpile một file TS/TSX/JSX thành JS ESM.
///
/// Pipeline:
///   1. Đọc source
///   2. Hash source bằng Blake3 → tìm trong CompiledCache
///   3. Nếu cache hit: rewrite imports → serve (0ms)
///   4. Nếu miss: esbuild transpile (bundle=false) → rewrite imports → lưu cache → serve
async fn serve_transpiled(file_path: &std::path::Path, state: &ServerState) -> Response {
    // 1. Đọc source
    let source = match tokio::fs::read(file_path).await {
        Ok(b) => b,
        Err(e) => {
            error!("Failed to read {}: {}", file_path.display(), e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file").into_response();
        }
    };

    // 2. Hash source (Blake3) → cache key
    let source_hash = blake3::Hasher::new()
        .update(&source)
        .finalize()
        .to_hex()
        .to_string();

    // 3. Kiểm tra CompiledCache (global, shared giữa projects)
    let store_root = mg_store::default_store_root();
    if let Ok(store) = mg_store::cas::ContentStore::new(store_root.clone()) {
        let compiled_cache = store.compiled_cache();
        let hash_key = mg_store::cas::IntegrityHash::from_hash_str(&source_hash, false);
        if let Ok(Some(cached)) = compiled_cache.get(&hash_key) {
            debug!(
                "compiled-cache hit: {} ({})",
                file_path.display(),
                &source_hash[..8]
            );
            let js_with_rewrites = rewrite_imports(&cached.js);
            return js_response(js_with_rewrites);
        }
    }

    // 4. Cache miss → transpile bằng esbuild (bundle=false, chỉ transpile TS→JS)
    debug!("transpiling: {}", file_path.display());

    let working_dir = state.config.root.to_string_lossy().to_string();
    let mut builder = esbuild_rs::BuildOptionsBuilder::new();
    builder.entry_points = vec![file_path.to_string_lossy().to_string()];
    builder.bundle = false; // ← QUAN TRỌNG: chỉ transpile, không bundle
    builder.abs_working_dir = working_dir;
    builder.platform = esbuild_rs::Platform::Browser;
    builder.format = esbuild_rs::Format::ESModule;
    builder.source_map = esbuild_rs::SourceMap::Inline;
    builder.write = false;
    builder.resolve_extensions = vec![
        ".tsx".to_string(),
        ".ts".to_string(),
        ".jsx".to_string(),
        ".js".to_string(),
    ];

    let options = builder.build();
    let result = esbuild_rs::build(options).await;

    if !result.errors.as_slice().is_empty() {
        let msgs: Vec<String> = result
            .errors
            .as_slice()
            .iter()
            .map(|e| e.to_string())
            .collect();
        let err_str = msgs.join("\n");
        error!("esbuild error for {}: {}", file_path.display(), err_str);

        // Trả về lỗi dưới dạng JS để browser có thể hiển thị (không crash silent)
        let err_js = format!(
            r#"
const __err = `{err_esc}`;
console.error('[MgDevServer] Build Error:', __err);
const el = document.getElementById('__mg-error') || document.createElement('div');
el.id = '__mg-error';
el.style = 'position:fixed;top:0;left:0;right:0;background:#1a0000;color:#ff6b6b;padding:20px;font-family:monospace;white-space:pre;z-index:99999;border-bottom:2px solid #f00';
el.textContent = '[MgDevServer Build Error]\n' + __err;
document.body?.prepend(el);
"#,
            err_esc = err_str.replace('`', "\\`")
        );
        return js_response(err_js);
    }

    let raw_js = result
        .output_files
        .as_slice()
        .iter()
        .find(|f| f.path.as_str().ends_with(".js"))
        .map(|f| f.data.as_str().to_string())
        .unwrap_or_default();

    // 5. Rewrite bare imports → /@megagate/deps/ paths
    let js_with_rewrites = rewrite_imports(&raw_js);

    // 6. Lưu vào CompiledCache (global) để projects khác dùng chung
    if let Ok(store) = mg_store::cas::ContentStore::new(store_root) {
        let compiled_cache = store.compiled_cache();
        let hash_key = mg_store::cas::IntegrityHash::from_hash_str(&source_hash, false);
        let module = mg_store::cas::CompiledModule {
            js: raw_js, // Lưu raw (trước khi rewrite) vì path có thể thay đổi giữa projects
            source_map: None,
        };
        if let Err(e) = compiled_cache.put(&hash_key, &module) {
            debug!("failed to save to compiled cache: {}", e);
        }
    }

    js_response(js_with_rewrites)
}

fn js_response(js: String) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, "application/javascript; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from(js))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "").into_response())
}
