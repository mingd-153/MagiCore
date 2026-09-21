//! Dev-server request-security E2E (P1-1/P1-2/P1-3, fresh-context review
//! 2026-09-15): the source/static route joined the raw request path onto
//! the project root WITHOUT a traversal gate, and the deps route joined
//! the raw package name onto node_modules — both served files OUTSIDE the
//! project on `GET /../...`. This file drives the REAL MgDevServer over
//! the real network stack and pins the fail-closed contract:
//!   - traversal paths → 403/404, NEVER the outside file's bytes;
//!   - percent-encoded traversal (%2e%2e) → blocked identically;
//!   - deps route traversal (`/@magicore/deps/../../...`) → blocked;
//!   - the CSS template-literal escape (`${`) → pinned at unit level
//!     (css_template_literal_is_escaped).
//!
//! Port DYNAMIC (bind :0) per RULE §13 (tests never fight over 4315).
//! (Bảo mật request dev-server: route source/static nối path request thô
//! vào root project KHÔNG cổng traversal, route deps nối tên package thô
//! vào node_modules — cả hai phục vụ file NGOÀI project với
//! `GET /../...`. File này chạy MgDevServer THẬT qua network stack thật
//! và ghim hợp đồng fail-closed. Port ĐỘNG theo RULE §13.)

#![allow(clippy::unwrap_used)]

use std::time::Duration;

use mgc::bundler::dev_server::{DevServerConfig, MgDevServer};

/// Pick a free port by binding :0 — never the RULE §13 fixed defaults.
/// (Chọn port tự do bằng bind :0 — không bao giờ port cố định RULE §13.)
fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Await server readiness on the dynamic port (bounded poll: 10s —
///
/// full-workspace parallel runs can starve a 5s budget and flake).
/// (Chờ server sẵn sàng — poll 10s vì chạy song song full suite đói CPU.)
async fn wait_ready(port: u16) {
    for _ in 0..200 {
        if reqwest::get(format!("http://127.0.0.1:{port}/"))
            .await
            .is_ok()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("dev server must become ready on the dynamic port");
}

/// One adversarial dev-server fixture: a project root with a SECRET
/// file OUTSIDE the root (parent dir) that a traversal request must
/// NEVER be able to read, plus an in-root src file for the happy path.
/// (Một fixture dev-server đối kháng: project root kèm file BÍ MẬT nằm
/// NGOÀI root (thư mục cha) mà request traversal KHÔNG BAO GIỜ được
/// đọc, cùng file src trong root cho đường hợp lệ.)
struct AdversarialSite {
    _tmp: tempfile::TempDir,
    root: std::path::PathBuf,
    // Underscore-prefixed: the file's existence is exercised via the
    // traversal REQUEST, not by reading this field directly.
    // (Prefix gạch-dưới: sự tồn tại của file được kiểm qua REQUEST
    // traversal, không đọc field này trực tiếp.)
    _secret_path: std::path::PathBuf,
    secret_marker: String,
}

impl AdversarialSite {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("site");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("index.html"), "<html><body>ok</body></html>").unwrap();
        std::fs::write(
            root.join("src").join("main.ts"),
            "export const MARKER = 'in-root-marker';\n",
        )
        .unwrap();
        // SECRET outside the root: traversal targets must never reach it.
        // (File BÍ MẬT ngoài root: path traversal không bao giờ được tới.)
        let secret_marker = "TOP-SECRET-OUTSIDE-ROOT-4315".to_string();
        let secret_path = tmp.path().join("outside-secret.txt");
        std::fs::write(&secret_path, &secret_marker).unwrap();
        Self {
            _tmp: tmp,
            root,
            _secret_path: secret_path,
            secret_marker,
        }
    }
}

/// P1-1: `GET /../outside-secret.txt` (and every spelling of traversal)
/// must NEVER return the secret file's bytes — 403/404 before the fix
/// served them (path join follows `..`). Encoded traversal `%2e%2e` is
/// the CSRF-relevant spelling (browsers normalize, proxies don't).
/// (P1-1: `GET /../outside-secret.txt` và mọi cách viết traversal
/// KHÔNG BAO GIỜ trả bytes file bí mật — trước fix chúng được phục vụ
/// (path join theo `..`). Traversal mã hóa `%2e%2e` là dạng liên quan
/// CSRF — phải chặn y hệt.)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn source_route_traversal_never_serves_outside_files() {
    let site = AdversarialSite::new();
    let port = free_port();
    let server_root = site.root.clone();
    let server = tokio::spawn(async move {
        let config = DevServerConfig {
            root: server_root,
            entry: "src/main.ts".into(),
            host: "localhost".to_string(),
            port,
        };
        MgDevServer::new(config).serve().await
    });
    wait_ready(port).await;

    // Every traversal spelling of "read the parent-dir secret".
    // (Mọi cách viết traversal của "đọc file bí mật ở thư mục cha".)
    let attacks = [
        "/../outside-secret.txt",
        "/%2e%2e/outside-secret.txt",
        "/%2E%2E/outside-secret.txt",
        "/..%2foutside-secret.txt",
        "/src/../../outside-secret.txt",
        "/public/../../outside-secret.txt",
    ];
    for attack in attacks {
        let resp = reqwest::get(format!("http://127.0.0.1:{port}{attack}"))
            .await
            .unwrap();
        let status = resp.status();
        let body = resp.text().await.unwrap();
        assert!(
            !body.contains(&site.secret_marker),
            "traversal request {attack:?} leaked the OUTSIDE-ROOT secret (status {status}, body {body:?}) — P1-1 regression"
        );
        assert!(
            status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::NOT_FOUND,
            "traversal request {attack:?} must fail closed (403/404), got {status}"
        );
    }

    // The happy path still serves the IN-ROOT file (fail-closed, not
    // fail-broken: legitimate requests must keep working).
    // (Đường hợp lệ vẫn phục vụ file TRONG ROOT — fail-closed không có
    // nghĩa fail-vô-đụng: request chính đáng phải chạy tiếp.)
    let ok = reqwest::get(format!("http://127.0.0.1:{port}/src/main.ts"))
        .await
        .unwrap();
    assert!(ok.status().is_success());
    let body = ok.text().await.unwrap();
    assert!(
        body.contains("in-root-marker"),
        "in-root source must still be served after the traversal gate, body: {body:?}"
    );

    server.abort();
    server.await.ok();
}

/// P1-1 (deps side): `GET /@magicore/deps/<traversal>` joined the raw
/// package name onto node_modules — `..%2f..` escaped the store. The
/// route must fail closed and never serve outside files.
/// (P1-1 (phía deps): `GET /@magicore/deps/<traversal>` nối tên package
/// thô vào node_modules — `..%2f..` thoát ra ngoài store. Route phải
/// fail-closed, không bao giờ phục vụ file ngoài.)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deps_route_traversal_never_serves_outside_files() {
    let site = AdversarialSite::new();
    // A node_modules dir so the deps route has a real base to join.
    // (Thư mục node_modules để route deps có base thật để nối.)
    std::fs::create_dir_all(site.root.join("node_modules")).unwrap();
    let port = free_port();
    let server_root = site.root.clone();
    let server = tokio::spawn(async move {
        let config = DevServerConfig {
            root: server_root,
            entry: "src/main.ts".into(),
            host: "localhost".to_string(),
            port,
        };
        MgDevServer::new(config).serve().await
    });
    wait_ready(port).await;

    let attacks = [
        "/@magicore/deps/../../outside-secret.txt",
        "/@magicore/deps/..%2f..%2foutside-secret.txt",
        "/@magicore/deps/%2e%2e%2f%2e%2e%2foutside-secret.txt",
        "/@magicore/deps/../../../etc/passwd",
    ];
    for attack in attacks {
        let resp = reqwest::get(format!("http://127.0.0.1:{port}{attack}"))
            .await
            .unwrap();
        let status = resp.status();
        let body = resp.text().await.unwrap();
        assert!(
            !body.contains(&site.secret_marker),
            "deps traversal {attack:?} leaked the OUTSIDE-ROOT secret (status {status}, body {body:?}) — P1-1 deps regression"
        );
    }

    server.abort();
    server.await.ok();
}

/// P1-2 (deps `<stdout>` sibling bug): with `write=false` esbuild-rs may
/// report the single output entry's path as `<stdout>` — the old
/// `find(ends_with(".js"))` in deps_bundler matched NOTHING and the
/// served dep JS was EMPTY. The transpile path was already fixed; this
/// pins the unit-level selection helper for the deps path: prefer a real
/// `.js` output; else take the FIRST output; never default to empty
/// when outputs exist.
/// (P1-2 (bug anh em `<stdout>` của deps): với `write=false`, esbuild-rs
/// có thể báo path của output đơn là `<stdout>` — `find` cũ trong
/// deps_bundler khớp 0 phần tử và dep được phục vụ là JS RỖNG. Đường
/// transpile đã sửa; test này ghim helper chọn output mức unit cho đường
/// deps: ưu tiên `.js` thật, không có thì lấy output ĐẦU, không bao giờ
/// mặc định rỗng khi có output.)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deps_bundle_of_a_real_package_serves_nonempty_js() {
    // A tiny CJS package in node_modules — bundling it must yield
    // NON-EMPTY JS (the `<stdout>` bug would yield an empty string).
    // (Một package CJS nhỏ trong node_modules — bundle nó phải ra JS
    // KHÔNG RỖNG (bug `<stdout>` sẽ trả chuỗi rỗng).)
    let site = AdversarialSite::new();
    let pkg_dir = site.root.join("node_modules").join("tiny-dep");
    std::fs::create_dir_all(&pkg_dir).unwrap();
    std::fs::write(
        pkg_dir.join("package.json"),
        r#"{ "name": "tiny-dep", "version": "1.0.0", "main": "index.js" }"#,
    )
    .unwrap();
    std::fs::write(
        pkg_dir.join("index.js"),
        "module.exports = function tiny(){ return 'tiny-dep-payload'; };",
    )
    .unwrap();

    let port = free_port();
    let server_root = site.root.clone();
    let server = tokio::spawn(async move {
        let config = DevServerConfig {
            root: server_root,
            entry: "src/main.ts".into(),
            host: "localhost".to_string(),
            port,
        };
        MgDevServer::new(config).serve().await
    });
    wait_ready(port).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/@magicore/deps/tiny-dep"))
        .await
        .unwrap();
    let status = resp.status();
    let body = resp.text().await.unwrap();
    assert!(
        status.is_success(),
        "deps bundle of an installed package must succeed, status {status}, body {body:?}"
    );
    assert!(
        !body.trim().is_empty(),
        "deps bundle must serve NON-EMPTY JS (the <stdout> sibling bug served empty output) — P1-2"
    );
    assert!(
        body.contains("tiny-dep-payload"),
        "deps bundle must carry the package's actual code, body: {body:?}"
    );

    server.abort();
    server.await.ok();
}
