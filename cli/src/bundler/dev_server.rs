/// dev_server.rs — MgDevServer: Native ESM Dev Server
///
/// # Kiến trúc (Native ESM + Dependency Pre-bundling)
///
/// Giống Vite, nhưng viết 100% bằng Rust, không cần Node.js:
///
/// ```text
/// Browser request                 MgDevServer xử lý
/// ─────────────────────────────────────────────────────
/// GET /                        -> serve index.html (inject HMR script)
/// GET /@magicore/hmr.js        -> serve HMR client script
/// GET /@magicore/hmr           -> WebSocket endpoint
/// GET /@magicore/deps/react    -> DepsCache: bundle react từ node_modules
/// GET /src/App.tsx             -> CompiledCache -> esbuild (transpile only)
///                                + rewrite bare imports -> /@magicore/deps/...
/// GET /src/App.css             -> serve as text/css
/// GET /public/logo.png         -> serve static
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
/// import React from '/@magicore/deps/react';
/// import { motion } from '/@magicore/deps/framer-motion';
/// ```
///
/// # Compile Cache
///
/// Mỗi file .ts/.tsx/.jsx được hash bằng Blake3 → tra CompiledCache.
/// Nếu hit: serve ngay (~0ms). Nếu miss: esbuild transpile → lưu vào cache.
/// Cache được dùng chung giữa tất cả project trên máy (global ~/.magicore store).
use crate::bundler::deps_bundler::DepsCache;
use crate::bundler::hmr::{HMR_CLIENT_SCRIPT, HmrManager, hmr_ws_handler};
use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};

use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{debug, error, info};

// Regex-based import rewriter — khớp cả ESM static và dynamic imports
// Ví dụ: import x from 'react' → import x from '/@magicore/deps/react'
//        import('react') → import('/@magicore/deps/react')
static BARE_IMPORT_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();

fn bare_import_re() -> &'static regex::Regex {
    BARE_IMPORT_RE.get_or_init(|| {
        // Khớp:
        //   from 'pkg'          from "pkg"
        //   import 'pkg'        import "pkg"
        //   import('pkg')       import("pkg")
        //   export from 'pkg'
        // Đường dẫn tương đối/absolute/@magicore được lọc trong rewrite_imports()
        regex::Regex::new(
            r#"(?P<keyword>from|import(?:\s*\()|import|export\s+(?:\*|(?:\{[^}]*\}))\s+from)\s*['"](?P<pkg>[^'"]+)['"]"#
        ).expect("invalid bare import regex")
    })
}

/// Rewrite bare imports trong JS/TS output thành /@magicore/deps/ paths.
fn rewrite_imports(js: &str) -> String {
    let re = bare_import_re();
    re.replace_all(js, |caps: &regex::Captures| {
        let keyword = &caps["keyword"];
        let pkg = &caps["pkg"];
        // Bỏ qua đường dẫn tương đối, absolute, và deps đã rewrite
        if pkg.starts_with("./")
            || pkg.starts_with("../")
            || pkg.starts_with('/')
            || pkg.starts_with("@magicore/")
        {
            return caps
                .get(0)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
        }
        // Xử lý scoped packages: @org/pkg → @org/pkg (giữ nguyên, chỉ thay separator)
        // Ví dụ: @tanstack/react-query → /@magicore/deps/@tanstack/react-query
        let is_dynamic = keyword.starts_with("import(");
        if is_dynamic {
            format!("import('/@magicore/deps/{}')", pkg)
        } else {
            format!("{} '/@magicore/deps/{}'", keyword.trim_end(), pkg)
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
    /// Pre-bundling cache cho node_modules (/@magicore/deps/*)
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
        // Watcher LIFETIME (P0, vòng-11 — found by the WS HMR evidence
        // test): the watcher binding previously lived INSIDE the
        // `if src_dir.exists() {}` block, so it was DROPPED at the end
        // of that block — before the server even bound its port. HMR
        // production never fired a single event; only tests that kept
        // the watcher in the enclosing scope passed. The watcher must
        // outlive `axum::serve` — hold it in THIS frame for the whole
        // serve() call.
        // (VÒNG ĐỜI watcher (P0 vòng-11 — test bằng chứng WS HMR tìm
        // ra): binding watcher trước đây nằm TRONG khối `if
        // src_dir.exists() {}`, nên bị DROP cuối khối đó — trước cả khi
        // server bind port. HMR production chưa từng fire event nào;
        // chỉ test giữ watcher ở scope ngoài mới pass. Watcher phải
        // sống lâu hơn `axum::serve` — giữ nó trong frame NÀY suốt
        // lời gọi serve().)
        let _watcher = if self.config.root.join("src").exists() {
            hmr_manager.watch_dir(&self.config.root.join("src"))
        } else {
            hmr_manager.watch_dir(&self.config.root)
        };
        if let Err(e) = &_watcher {
            // Non-fatal: a project the watcher cannot attach to still
            // serves — HMR is disabled, plain reload still works.
            // (Không chặn: project watcher không gắn được vẫn serve —
            // HMR tắt, reload thường vẫn chạy.)
            eprintln!(
                "warning: HMR file watcher failed to start ({e}) — \
                 hot reload disabled, serving without it"
            );
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
            .route("/@magicore/hmr.js", get(serve_hmr_client))
            .route("/@magicore/hmr", get(hmr_ws_handler))
            // Dependency pre-bundling: /@magicore/deps/{package}
            .route("/@magicore/deps/*pkg", get(serve_dep))
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
            "MgDevServer (Native ESM) listening at http://{}:{}",
            self.config.host, self.config.port
        );

        let listener = TcpListener::bind(&bind_addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// Minimal HTML-text escaping (P1-3): the fallback index.html template
/// interpolates the project FOLDER NAME into <title>. A folder named
/// `<script>alert(1)</script>` would inject live markup. Only `&`,
/// `<`, `>` need escaping for a text context (quotes matter in
/// attributes, not text nodes).
/// (Escape HTML tối thiểu (P1-3): template index.html dự phòng nội suy
/// TÊN THƯ MỤC project vào <title>. Thư mục tên `<script>...` sẽ bơm
/// markup sống. Ch文本 context chỉ cần escape `&`, `<`, `>`.)
fn html_escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Phục vụ index.html với HMR script được inject.
async fn serve_index(State(state): State<ServerState>) -> impl IntoResponse {
    let index_path = state.config.root.join("index.html");
    let mut html = match std::fs::read_to_string(&index_path) {
        Ok(content) => content,
        Err(_) => {
            let title_raw = state
                .config
                .root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("MagiCore App");
            // Fallback template: the folder name is attacker-influenceable
            // (project path) — escape it for the HTML text context.
            // (Template dự phòng: tên thư mục chịu ảnh hưởng của kẻ tấn
            // công (path project) — escape cho ngữ cảnh text HTML.)
            let title = html_escape_text(title_raw);
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
    let hmr_tag = r#"<script type="module" src="/@magicore/hmr.js"></script>"#;
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
    match Response::builder()
        .header(header::CONTENT_TYPE, "application/javascript")
        .body(Body::from(HMR_CLIENT_SCRIPT))
    {
        Ok(response) => response,
        Err(_) => Response::new(Body::from(HMR_CLIENT_SCRIPT)),
    }
}

/// Embed an arbitrary runtime string as a JS STRING LITERAL (P1-3,
/// fresh-context review 2026-09-15): dependency CSS and compiler errors
/// used to ride inside a JS template literal with a hand-rolled escape
/// (backtick-only) — a CSS payload containing `${...}` became a live JS
/// EXPRESSION in the browser page (semi-trusted npm dependency → code
/// execution). Instead of trying to out-escape the template-literal
/// grammar, the payload is embedded as a JSON string literal:
/// `serde_json::to_string` produces a double-quoted JSON string, which
/// is a valid JS string literal by construction (JSON escaping covers
/// `"`, `\`, control chars — no terminator or interpolation sequence can
/// survive). The string then flows into `replaceSync(...)` /
/// `textContent` as DATA, never as code.
/// (Nhúng chuỗi runtime tùy ý thành STRING LITERAL JS (P1-3): CSS
/// dependency và lỗi compiler từng nằm trong template literal JS với
/// escape tự chế (chỉ backtick) — payload CSS chứa `${...}` thành BIỂU
/// THỨC JS chạy trong trang browser. Thay vì đua escape với ngữ pháp
/// template literal, payload được nhúng dạng JSON string literal:
/// `serde_json::to_string` ra chuỗi JSON trích dẫn kép — hợp lệ là JS
/// string literal theo cấu trúc (JSON escape phủ `"`, `\`, control
/// chars — không chuỗi kết thúc hay nội suy nào sống sót). Chuỗi rồi
/// chảy vào replaceSync/textContent như DỮ LIỆU, không bao giờ là code.)
pub(crate) fn js_string_literal(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

/// Serve a pre-bundled dependency.
/// Route: GET /@magicore/deps/*pkg
/// E.g. /@magicore/deps/react → bundle react from node_modules/react
///      /@magicore/deps/@tanstack/react-query → scoped package
/// (Phục vụ dependency đã pre-bundled: /@magicore/deps/*pkg — đứng sau
/// cổng validator tên package P1-1.)
async fn serve_dep(Path(pkg): Path<String>, State(state): State<ServerState>) -> impl IntoResponse {
    debug!("deps request: {}", pkg);

    // P1-1 traversal gate (fresh-context review 2026-09-15): the raw
    // route segment was joined onto node_modules — `..%2f..` escaped the
    // project and read outside files. A package name may ONLY be
    // `name`, `@scope/name` (+ optional subpath `@scope/name/sub`), with
    // every segment a valid npm package-name segment: no `..`, no `.`,
    // no separators beyond `/`, no Windows drive prefix, no empty
    // segment. Fail-closed 403.
    // (Cổng traversal P1-1 cho deps: segment route thô từng được nối
    // vào node_modules — `..%2f..` thoát project đọc file ngoài. Tên
    // package chỉ được là `name`, `@scope/name` (+ subpath), mỗi segment
    // là segment tên npm hợp lệ: không `..`, không `.`, không dấu phân
    // tách ngoài `/`, không prefix ổ đĩa Windows, không segment rỗng.
    // Fail-closed 403.)
    if !crate::bundler::deps_bundler::dep_name_is_valid(&pkg) {
        return (StatusCode::FORBIDDEN, "Forbidden: invalid dependency name").into_response();
    }

    match state.deps_cache.get_or_bundle(&pkg).await {
        Some(dep) => {
            // If generated CSS accompanies the dep, inject the loader —
            // P1-3: the CSS rides as a JS STRING LITERAL (JSON-escaped,
            // see js_string_literal) — never inside a template literal.
            // (CSS kèm theo được bơm qua string literal JS (JSON-escape,
            // xem js_string_literal) — không bao giờ trong template
            // literal.)
            let mut js = dep.js.clone();
            // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
            if let Some(css) = &dep.css
                && !css.is_empty()
            {
                // Inject CSS via the CSSStyleSheet API (modern browsers)
                // — the payload is DATA inside a JSON string literal.
                // (Bơm CSS qua API CSSStyleSheet — payload là DỮ LIỆU
                // trong JSON string literal.)
                let css_literal = js_string_literal(css);
                let css_injector = format!(
                    r#"
// [MgDevServer] Injected CSS from {pkg}
(function() {{
  const sheet = new CSSStyleSheet();
  sheet.replaceSync({css_literal});
  document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
}})();
"#
                );
                js.push_str(&css_injector);
            }

            Response::builder()
                .header(
                    header::CONTENT_TYPE,
                    "application/javascript; charset=utf-8",
                )
                .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
                .body(Body::from(js))
                .unwrap_or_else(|_| {
                    (StatusCode::INTERNAL_SERVER_ERROR, "Server error").into_response()
                })
        }
        None => {
            let err_js =
                format!(r#"console.error('[MgDevServer] Failed to bundle dependency: {pkg}');"#);
            Response::builder()
                .header(header::CONTENT_TYPE, "application/javascript")
                .status(StatusCode::NOT_FOUND)
                .body(Body::from(err_js))
                .unwrap_or_else(|_| (StatusCode::NOT_FOUND, "").into_response())
        }
    }
}

/// Request-path traversal gate (P1-1, fresh-context review 2026-09-15):
/// the raw request path joined onto the project root WITHOUT any check —
/// `GET /..%2f<file>` served files OUTSIDE the project (proved red by
/// dev_server_request_security.rs). The gate rejects, fail-closed:
///   - any `..` path component (raw or percent-DECODED — the router
///     decodes `%2e%2e`/`..%2f` before this code runs);
///   - any `.` / Prefix / RootDir component (no absolute/Windows-drive
///     escapes into the join);
///   - empty requests.
///
/// The REJECT is 403 FORBIDDEN (not 404) so a traversal probe is
/// distinguishable from a missing file in logs.
/// (Cổng traversal path request (P1-1): path request thô từng được nối
/// thẳng vào root project KHÔNG check nào — `GET /..%2f<file>` phục vụ
/// file NGOÀI project (chứng minh đỏ bởi dev_server_request_security).
/// Cổng chặn fail-closed: mọi component `..` (thô hoặc đã percent-DECODE
/// — router decode `%2e%2e`/`..%2f` trước khi chạy tới đây); mọi
/// component `.` / Prefix / RootDir (không tuyệt-đối-hóa/Windows-drive
/// trốn vào phép nối); request rỗng. Từ chối là 403 (không 404) để phân
/// biệt probe traversal với file thiếu trong log.)
fn request_path_is_safe(rel_path: &str) -> bool {
    // Reject early on the RAW string for the undecoded spellings the
    // router may pass through, then re-check every decoded component.
    // (Chặn sớm trên chuỗi THÔ cho các dạng chưa decode router có thể
    // chuyển qua, rồi check lại từng component đã decode.)
    if rel_path.is_empty() {
        return false;
    }
    for component in std::path::Path::new(rel_path).components() {
        match component {
            std::path::Component::Normal(_) => {}
            // CurDir (`.`), ParentDir (`..`), RootDir (`/`), Windows
            // Prefix (`C:`) — every non-normal component is a jailbreak
            // attempt: reject the whole request.
            // (CurDir, ParentDir, RootDir, Windows Prefix — mọi
            // component không-normal là ý đồ vượt ngục: chặn cả request.)
            _ => return false,
        }
    }
    true
}

/// Serve static files or transpiled sources for requests under the
/// project root — behind the P1-1 traversal gate.
/// (Phục vụ file tĩnh hoặc source transpile cho request dưới root
/// project — sau cổng traversal P1-1.)
async fn serve_source_or_static(
    Path(path): Path<String>,
    State(state): State<ServerState>,
) -> impl IntoResponse {
    // Normalize path: strip the leading slash if present.
    // (Chuẩn hoá path: bỏ leading slash nếu có.)
    let rel_path = path.trim_start_matches('/');

    // P1-1 traversal gate — BEFORE any filesystem join. A request whose
    // decoded path escapes the root must fail closed with 403.
    // (Cổng traversal P1-1 — TRƯỚC mọi phép nối filesystem. Request có
    // path decode thoát khỏi root phải fail cứng với 403.)
    if !request_path_is_safe(rel_path) {
        return (
            StatusCode::FORBIDDEN,
            "Forbidden: path escapes the project root",
        )
            .into_response();
    }

    // Try to find the file in src/, then root.
    // (Thử tìm file trong src/, rồi root.)
    let candidates = [
        state.config.root.join(rel_path),
        state.config.root.join("public").join(rel_path),
    ];

    let file_path = candidates.iter().find(|p| p.exists() && p.is_file());

    let Some(file_path) = file_path else {
        return (StatusCode::NOT_FOUND, format!("Not found: /{}", rel_path)).into_response();
    };

    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        // ─── TypeScript / JSX: transpile on-the-fly ───────────────────────
        "ts" | "tsx" | "jsx" | "js" | "mjs" => serve_transpiled(file_path, &state).await,
        // ─── CSS: serve trực tiếp ─────────────────────────────────────────
        "css" => match tokio::fs::read_to_string(file_path).await {
            Ok(content) => Response::builder()
                .header(header::CONTENT_TYPE, "text/css; charset=utf-8")
                .header(header::CACHE_CONTROL, "no-cache")
                .body(Body::from(content))
                .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "").into_response()),
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

/// Compiler identity + version for the compilation key (P0-1 audit
/// vòng-4). esbuild-rs 0.13.8 exposes NO runtime version API, so the
/// version is pinned to the workspace dependency: bumping `esbuild-rs` in
/// Cargo.toml MUST bump this constant together — the cache then
/// self-invalidates (new compiler version → new key → fresh compile).
///
/// (Danh tính + phiên bản compiler cho compilation key (P0-1 audit
/// vòng-4). esbuild-rs 0.13.8 KHÔNG có API runtime version nên version
/// ghim theo dependency workspace: nâng `esbuild-rs` trong Cargo.toml thì
/// PHẢI nâng hằng này theo — cache tự vô hiệu (compiler mới → key mới →
/// biên dịch lại).)
const COMPILER_IDENTITY: &str = "esbuild-rs";
const COMPILER_VERSION: &str = "0.13.8";

/// Canonical compile-options JSON (P0-1): every esbuild option the dev
/// server's transpile output depends on, serialized deterministically
/// (serde_json::json! object → BTreeMap under `preserve_order` OFF →
/// sorted keys) so the options digest is byte-stable across processes and
/// restarts. Change a pipeline option → change this JSON → the cache key
/// changes → entries recompile (no stale cross-option serving).
///
/// (JSON options biên dịch chuẩn hóa (P0-1): mọi option esbuild mà output
/// transpile của dev server phụ thuộc, serialize tất định (object
/// serde_json::json! → BTreeMap khi tắt `preserve_order` → key đã sort)
/// để digest ổn định từng byte giữa các process và restart. Đổi option
/// pipeline → đổi JSON này → key đổi → entry biên dịch lại (không phục vụ
/// stale chéo option).)
/// Effective compilation context (Hướng B, P0-2 Tech Lead vòng-6/7): the
/// cache key must capture EVERYTHING the dev server's transpile output
/// depends on BEYOND the source bytes — the project's tsconfig semantics
/// (jsx, target, decorators, useDefineForClassFields...), the project
/// root (sourcemap paths, resolve base) and the source's logical path.
/// Without this, two projects sharing the global compiled cache could
/// CROSS-HIT: same source bytes + different tsconfig → one project
/// receives the other's output. The context is embedded into the
/// canonical options JSON that feeds `CompilationKey::new` (its digest
/// becomes part of the key), so a context change = a key change = a fresh
/// compile, never a stale cross-project serve.
///
/// Ngữ cảnh biên dịch hiệu dụng (Hướng B, P0-2): cache key phải chứa MỌI
/// thứ mà output transpile phụ thuộc NGOÀI bytes source — semantics
/// tsconfig của project (jsx, target, decorators, useDefineForClassFields...),
/// root project (path sourcemap, resolve base) và path logic của source.
/// Thiếu nó, 2 project dùng chung compiled cache toàn cục có thể CROSS-HIT:
/// cùng bytes source + tsconfig khác → project này nhận output của project
/// kia. Ngữ cảnh được nhúng vào JSON options chuẩn hóa feeding
/// `CompilationKey::new` (digest của nó thành một phần key), nên đổi ngữ
/// cảnh = đổi key = biên dịch lại, không bao giờ serve stale chéo project.
fn canonical_compile_options(
    project_root: &std::path::Path,
    source_path: &std::path::Path,
) -> std::result::Result<String, String> {
    // NOTE: the field SET must mirror every `builder.*` assignment in
    // serve_transpiled below — a drift here silently forks the cache.
    // (LƯU Ý: TẬP field phải phản chiếu mọi gán `builder.*` trong
    // serve_transpiled bên dưới — lệch ở đây là tự ý rẽ nhánh cache ngầm.)
    // Fail-closed tsconfig (Gate 11-C, vòng-11): an unreadable tsconfig
    // must FAIL the options build — the compile never runs under
    // unknown semantics.
    // (Tsconfig fail-closed: tsconfig không đọc được phải FAIL việc dựng
    // options — compile không bao giờ chạy dưới semantics không xác định.)
    let tsconfig = tsconfig_digest(project_root)?;
    let opts = serde_json::json!({
        "bundle": false,
        "format": "esmodule",
        "platform": "browser",
        "resolve_extensions": [".tsx", ".ts", ".jsx", ".js"],
        "source_map": "inline",
        "write": false,
        // ── EffectiveCompilationContext (Hướng B, P0-2) ──
        "project_root_digest": project_root_digest(project_root),
        "normalized_source_path": normalized_source_path(project_root, source_path),
        "tsconfig_digest": tsconfig.key_fragment(),
    });
    Ok(serde_json::to_string(&opts).unwrap_or_default())
}

/// Digest of the project root's canonical path — separates the cache
/// namespaces of two different projects (their sourcemaps embed
/// root-relative paths, so their outputs are NOT interchangeable even for
/// identical sources).
/// (Digest của path chuẩn hóa của project root — tách namespace cache
/// của 2 project khác nhau (sourcemap của chúng nhúng path theo root,
/// nên output KHÔNG hoán đổi được kể cả khi source giống hệt).)
fn project_root_digest(root: &std::path::Path) -> String {
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    blake3::Hasher::new()
        .update(canonical.to_string_lossy().as_bytes())
        .finalize()
        .to_hex()
        .to_string()
}

/// Logical (project-relative) source path — stable across machine
/// checkouts, unique within a project. Two sources at different logical
/// paths are different compilations even with equal bytes (they produce
/// different sourcemap `sources` entries).
/// (Path logic (theo project) của source — ổn định qua các máy checkout,
/// duy nhất trong project. Hai source khác path logic là 2 lần biên dịch
/// khác nhau kể cả khi bytes bằng nhau (chúng sinh `sources` trong
/// sourcemap khác nhau).)
fn normalized_source_path(root: &std::path::Path, source: &std::path::Path) -> String {
    // Prefer the project-relative logical path; fall back to the file
    // name alone (source outside the root — caller wiring), then the raw
    // path as a last resort. All three forms are deterministic for a
    // given (project, file) pair, which is what the key requires.
    // (Ưu tiên path logic theo project; fallback chỉ tên file (source nằm
    // ngoài root — wiring của caller), cuối cùng là path thô. Cả ba dạng
    // đều tất định cho một cặp (project, file) — đúng cái key cần.)
    if let Ok(p) = source.strip_prefix(root) {
        return p.to_string_lossy().into_owned();
    }
    if let Some(name) = source.file_name() {
        return name.to_string_lossy().into_owned();
    }
    source.to_string_lossy().into_owned()
}

/// Content digest of the project's tsconfig.json — recomputed per
/// request (one stat + one read of a small config file; the v2 identity
/// memo (mtime+len) is GONE because it could serve a stale digest on a
/// same-length rewrite with a restored mtime — the audit's standing
/// finding. A tsconfig is tiny; hashing it per request is cheap and
/// always correct).
///
/// Fail-closed semantics (Gate 11-C, vòng-11 audit — replacing the
/// poison-nonce scheme the round-10 report claimed as a fix):
///   - NotFound → `TsconfigState::Absent` — compiled with esbuild's
///     defaults, its own cache namespace (never collides with a digest);
///   - successful read → `TsconfigState::Present(blake3-of-bytes)`;
///   - ANY other error (permission denied, I/O, ELOOP) → `Err` — the
///     dev server answers 500 with an actionable message. The compiler
///     must NEVER compile under unknown semantics: a poison nonce
///     changed the key per call but still served a compile made with
///     WRONG (default) tsconfig options — stale-poison was fixed, wrong
///     semantics was not.
///
/// Digest nội dung tsconfig.json của project — tính lại mỗi request
/// (một stat + một đọc file config nhỏ; memo identity v2 (mtime+len) ĐÃ
/// BỎ vì nó có thể phục vụ digest stale khi ghi lại cùng độ dài và phục
/// hồi mtime — finding còn treo của audit. Tsconfig rất nhỏ; hash mỗi
/// request rẻ và luôn đúng).
///
/// Ngữ nghĩa fail-closed (thay scheme poison-nonce mà báo cáo vòng-10
/// gọi là bản sửa):
///   - NotFound → `TsconfigState::Absent` — biên dịch với default của
///     esbuild, namespace cache riêng (không bao giờ đụng digest);
///   - đọc thành công → `TsconfigState::Present(blake3-của-bytes)`;
///   - MỌI lỗi khác (từ chối quyền, I/O, ELOOP) → `Err` — dev server trả
///     500 kèm thông điệp xử lý được. Compiler KHÔNG BAO GIỜ được biên
///     dịch dưới semantics không xác định: poison nonce đổi key mỗi lần
///     gọi nhưng vẫn phục vụ một bản compile dùng tsconfig SAI (default)
///     — stale-poison được sửa, semantics sai thì không.)
fn tsconfig_digest(root: &std::path::Path) -> std::result::Result<TsconfigState, String> {
    let path = root.join("tsconfig.json");
    // Metadata FIRST, and only NotFound means absent — a PermissionDenied
    // or ELOOP here used to fold into "absent" (compiled with the WRONG
    // semantics while the file existed). Everything else is fatal.
    // (Metadata TRƯỚC, chỉ NotFound là vắng — PermissionDenied hay ELOOP
    // ở đây từng bị gộp vào "absent" (biên dịch với semantics SAI trong
    // khi file tồn tại). Mọi lỗi khác là fatal.)
    match std::fs::metadata(&path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(TsconfigState::Absent);
        }
        Err(e) => {
            return Err(format!(
                "tsconfig.json exists but cannot be accessed ({}): {e} — refusing \
                 to compile with unknown semantics; fix permissions and reload",
                path.display()
            ));
        }
    }
    // Read the CONTENT — the identity cache (mtime+len) only avoids
    // rehashing on the hit path; a read error after a successful stat is
    // fatal (not absent, not poison — the request fails with 500).
    // (Đọc NỘI DUNG — cache identity (mtime+len) chỉ tránh rehash trên
    // đường hit; lỗi đọc sau khi stat thành công là fatal (không phải
    // absent, không phải poison — request fail với 500).)
    let bytes = std::fs::read(&path).map_err(|e| {
        format!(
            "tsconfig.json cannot be read ({}): {e} — refusing to compile \
             with unknown semantics; fix permissions and reload",
            path.display()
        )
    })?;
    let digest = blake3::Hasher::new()
        .update(&bytes)
        .finalize()
        .to_hex()
        .to_string();
    Ok(TsconfigState::Present(digest))
}

/// The three honest states of a project's tsconfig — see
/// `tsconfig_digest`. Absent and Present are both cache-key material
/// (distinct namespaces); the error state never reaches the key: it
/// fails the request instead.
/// (Ba trạng thái trung thực của tsconfig project — xem `tsconfig_digest`.
/// Absent và Present đều là liệu liệu key cache (namespace riêng); trạng
/// thái lỗi không bao giờ tới key: nó fail request thay vì.)
#[derive(Debug, Clone, PartialEq, Eq)]
enum TsconfigState {
    Absent,
    Present(String),
}

impl TsconfigState {
    /// Cache-key fragment — stable, JSON-safe.
    /// (Mảnh key cache — ổn định, an toàn JSON.)
    fn key_fragment(&self) -> String {
        match self {
            TsconfigState::Absent => "absent".to_string(),
            TsconfigState::Present(digest) => format!("present:{digest}"),
        }
    }
}

/// Transpile một file TS/TSX/JSX thành JS ESM.
///
/// Pipeline:
///   1. Đọc source
///   2. Hash source bằng Blake3 → tìm trong CompiledCache bằng CompilationKey đầy đủ
///   3. Nếu cache hit: rewrite imports → serve (0ms)
///   4. Nếu miss: esbuild transpile (bundle=false) → rewrite imports → lưu cache → serve
///
/// Cache key (P0-1 audit vòng-4) là MỘT `CompilationKey` đầy đủ ngữ cảnh:
/// source digest + loader (theo extension) + compiler identity/version +
/// digest của JSON options chuẩn hóa. Hai file cùng nội dung nhưng khác
/// loader (a.ts vs a.tsx) không còn dùng chung entry.
async fn serve_transpiled(file_path: &std::path::Path, state: &ServerState) -> Response {
    // 1. Đọc source
    let source = match tokio::fs::read(file_path).await {
        Ok(b) => b,
        Err(e) => {
            error!("Failed to read {}: {}", file_path.display(), e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file").into_response();
        }
    };

    // 2. Hash source (Blake3) → the CONTENT half of the compilation key.
    //    (Hash source (Blake3) → NỬA NỘI DUNG của compilation key.)
    let source_hash = blake3::Hasher::new()
        .update(&source)
        .finalize()
        .to_hex()
        .to_string();

    // Loader semantics from the file EXTENSION (P0-1): the same text under
    // .ts vs .tsx compiles differently — the extension must reach the key
    // through the CLOSED-SET Loader enum (a free-form string used to fork
    // the cache namespace with "garbage-loader").
    // (Semantics loader theo ĐUÔI FILE (P0-1): cùng text dưới .ts với .tsx
    // biên dịch khác nhau — đuôi file phải vào key qua enum Loader TẬP ĐÓNG
    // (string tự do từng rẽ nhánh namespace cache bằng "garbage-loader").)
    let loader = match file_path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(mgc_store::cas::Loader::from_extension)
    {
        Some(loader) => loader,
        None => {
            // No sanctioned loader for this extension — the bundler router
            // (serve_static_or_transpiled) only routes ts/tsx/jsx/js/mjs
            // here; reaching this arm is a wiring bug, fail loudly.
            // (Không có loader hợp lệ cho đuôi này — router bundler chỉ
            // chuyển ts/tsx/jsx/js/mjs tới đây; vào nhánh này là bug wiring,
            // hét to.)
            error!(
                "no compilation loader for extension: {}",
                file_path.display()
            );
            return (StatusCode::INTERNAL_SERVER_ERROR, "Unsupported file type").into_response();
        }
    };

    // Canonical compile options with the EffectiveCompilationContext
    // (Hướng B, P0-2) — see `canonical_compile_options`. Fail-closed
    // (Gate 11-C, vòng-11): an unreadable tsconfig fails the request
    // with 500 + an actionable message — the compile NEVER runs under
    // unknown semantics (the old poison-nonce still compiled with
    // esbuild defaults while the file was unreadable).
    // (Options biên dịch chuẩn hóa kèm ngữ cảnh hiệu dụng (Hướng B,
    // P0-2) — xem `canonical_compile_options`. Fail-closed: tsconfig
    // không đọc được fail request với 500 + thông điệp xử lý được —
    // compile KHÔNG BAO GIỜ chạy dưới semantics không xác định
    // (poison-nonce cũ vẫn compile với default esbuild trong khi file
    // không đọc được).)
    let project_root = state.config.root.clone();
    let options_json = match canonical_compile_options(&project_root, file_path) {
        Ok(opts) => opts,
        Err(msg) => {
            error!("{msg}");
            return (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response();
        }
    };

    // 3. Tra CompiledCache bằng key đầy đủ (global, shared giữa projects)
    let store_root = mgc_store::default_store_root();
    if let Ok(store) = mgc_store::cas::ContentStore::new(store_root.clone()) {
        let compiled_cache = store.compiled_cache();
        if let Ok(comp_key) = mgc_store::cas::CompilationKey::new(
            &source_hash,
            loader,
            COMPILER_IDENTITY,
            COMPILER_VERSION,
            &options_json,
        ) {
            match compiled_cache.get(&comp_key) {
                // Cache hit — the record already passed self-authentication
                // (P0-3): payload digest verified before serving.
                // (Cache hit — record đã qua tự xác thực (P0-3): digest
                // payload được verify trước khi phục vụ.)
                Ok(Some(cached)) => {
                    debug!(
                        "compiled-cache hit: {} ({}:{})",
                        file_path.display(),
                        loader,
                        &source_hash[..8]
                    );
                    let js_with_rewrites = rewrite_imports(&cached.js);
                    return js_response(js_with_rewrites);
                }
                // Corrupt/poisoned entry (P0-3): quarantined + hard error —
                // fall through and recompile; the put below replaces the
                // slot. Poison is never served.
                // (Entry hỏng/đầu độc (P0-3): cách ly + lỗi cứng — rơi xuống
                // biên dịch lại; put bên dưới thay slot. Không phục vụ độc.)
                Err(e) => {
                    debug!(
                        "compiled-cache entry rejected (quarantined), recompiling {}: {}",
                        file_path.display(),
                        e
                    );
                }
                // Miss → compile below.
                // (Miss → biên dịch bên dưới.)
                Ok(None) => {}
            }
        }
    }

    // 4. Cache miss → transpile bằng esbuild (bundle=false, chỉ transpile TS→JS)
    debug!("transpiling: {}", file_path.display());

    let working_dir = state.config.root.to_string_lossy().to_string();
    let mut builder = esbuild_rs::BuildOptionsBuilder::new();
    builder.entry_points = vec![file_path.to_string_lossy().to_string()];
    builder.bundle = false; // ← QUAN TRỌNG: chỉ transpile, không bundle
    builder.abs_working_dir = working_dir;
    // Honor the project's tsconfig.json (Hướng B, P0-2): jsx mode, target,
    // decorators, useDefineForClassFields... must come from the PROJECT,
    // not from a fixed default — `mgc dev` respects the project's config.
    // The tsconfig CONTENT digest already sits in the cache key via
    // `canonical_compile_options`, so any tsconfig change = new key =
    // fresh compile (no stale cross-project hit can occur).
    // (Tôn trọng tsconfig.json của project (Hướng B, P0-2): jsx mode,
    // target, decorators, useDefineForClassFields... phải đến từ PROJECT,
    // không phải default cố định — `mgc dev` tôn trọng cấu hình project.
    // Digest NỘI DUNG tsconfig đã nằm trong cache key qua
    // `canonical_compile_options`, nên mọi thay đổi tsconfig = key mới =
    // biên dịch lại (không thể xảy ra stale hit chéo project).)
    let tsconfig_path = project_root.join("tsconfig.json");
    if tsconfig_path.exists() {
        builder.tsconfig = tsconfig_path.to_string_lossy().to_string();
    }
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
        // — P1-3 discipline: the error body is a JS STRING LITERAL
        // (JSON-escaped via js_string_literal) — a compile error from
        // attacker-influenced source must never become live JS.
        // (Trả lỗi dạng JS hiển thị được — kỷ luật P1-3: thân lỗi là
        // STRING LITERAL JS (JSON-escape qua js_string_literal) — lỗi
        // biên dịch từ source bị kẻ tấn công ảnh hưởng không được
        // thành JS chạy trong trang.)
        let err_js = format!(
            r#"
const __err = {err_esc};
console.error('[MgDevServer] Build Error:', __err);
const el = document.getElementById('__mgc-error') || document.createElement('div');
el.id = '__mgc-error';
el.style = 'position:fixed;top:0;left:0;right:0;background:#1a0000;color:#ff6b6b;padding:20px;font-family:monospace;white-space:pre;z-index:99999;border-bottom:2px solid #f00';
el.textContent = '[MgDevServer Build Error]\n' + __err;
document.body?.prepend(el);
"#,
            err_esc = js_string_literal(&err_str)
        );
        return js_response(err_js);
    }

    // Output selection (P0, vòng-11 — found by the WS HMR evidence test):
    // esbuild-rs 0.13.8 with `write=false` reports the single output
    // entry's path as `<stdout>` — the old filter
    // (`path.ends_with(".js")`) matched NOTHING, `raw_js` fell to the
    // empty default, and EVERY transpiled module was served as EMPTY JS
    // (the compiled cache stored empty payloads too). Prefer a real
    // `.js` output entry; with none, take the FIRST output —
    // bundle=false + one entry point means the first output IS the
    // transpiled module (verified by probe: exactly 1 output, correct
    // JS body, path `<stdout>`).
    // (Chọn output (P0 vòng-11 — test bằng chứng WS HMR tìm ra):
    // esbuild-rs 0.13.8 với `write=false` báo path của output đơn là
    // `<stdout>` — filter cũ (`ends_with(".js")`) KHÔNG khớp gì, `raw_js`
    // rơi về mặc định rỗng, và MỌI module transpile được phục vụ là JS
    // RỖNG (compiled cache cũng lưu payload rỗng). Ưu tiên entry `.js`
    // thật; không có thì lấy output ĐẦU — bundle=false + 1 entry point
    // nghĩa output đầu chính là module đã transpile (probe xác minh:
    // đúng 1 output, body JS đúng, path `<stdout>`).)
    let raw_js = result
        .output_files
        .as_slice()
        .iter()
        .find(|f| f.path.as_str().ends_with(".js"))
        .or_else(|| result.output_files.as_slice().first())
        .map(|f| f.data.as_str().to_string())
        .unwrap_or_default();

    // 5. Rewrite bare imports → /@magicore/deps/ paths
    let js_with_rewrites = rewrite_imports(&raw_js);

    // 6. Lưu vào CompiledCache (global) để projects khác dùng chung — với
    //    CÙNG CompilationKey đã tra ở bước 3 (P0-1: key đầy đủ context, P0-2:
    //    race conflict được typed, P0-3: record tự xác thực).
    if let Ok(store) = mgc_store::cas::ContentStore::new(store_root) {
        let compiled_cache = store.compiled_cache();
        if let Ok(comp_key) = mgc_store::cas::CompilationKey::new(
            &source_hash,
            loader,
            COMPILER_IDENTITY,
            COMPILER_VERSION,
            &options_json,
        ) {
            let module = mgc_store::cas::CompiledModule {
                js: raw_js, // Lưu raw (trước khi rewrite) vì path có thể thay đổi giữa projects
                source_map: None,
            };
            if let Err(e) = compiled_cache.put(&comp_key, &module) {
                debug!("failed to save to compiled cache: {}", e);
            }
        }
    }

    js_response(js_with_rewrites)
}

fn js_response(js: String) -> Response {
    Response::builder()
        .header(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from(js))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "").into_response())
}

#[cfg(test)]
#[path = "../test/dev_server_test.rs"]
mod tests;
