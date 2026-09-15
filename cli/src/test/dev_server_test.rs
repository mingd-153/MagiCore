#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for the dev-server compilation context (Hướng B, P0-2) and the
//! compiler-identity drift gate (P0-3). Lives beside `dev_server.rs`
//! (child module) so the private helpers under test are in scope.
//! (Test cho ngữ cảnh biên dịch dev-server (Hướng B, P0-2) và cổng chống
//! trôi danh tính compiler (P0-3). Nằm cạnh `dev_server.rs` (module con)
//! để helper private được test nằm trong scope.)

use super::*;
use tempfile::tempdir;

/// Drift gate (P0-3, Tech Lead vòng-6/7): the hardcoded `COMPILER_VERSION`
/// in the dev server must equal the esbuild-rs version actually locked in
/// the workspace. A `cargo update` changing esbuild-rs without bumping the
/// const would poison the compiled cache (new compiler → old key → stale
/// entries served as hits). This test finds the repo root via
/// CARGO_MANIFEST_DIR (../.. from cli/) and parses Cargo.lock directly.
///
/// Cổng chống trôi (P0-3): const `COMPILER_VERSION` hardcode trong dev
/// server phải bằng phiên bản esbuild-rs thật sự lock trong workspace.
/// `cargo update` đổi esbuild-rs mà quên nâng const sẽ độc cache đã biên
/// dịch (compiler mới → key cũ → entry stale được phục vụ như hit). Test
/// tìm repo root qua CARGO_MANIFEST_DIR (../.. từ cli/) và parse trực tiếp
/// Cargo.lock.
#[test]
fn compiler_version_matches_locked_esbuild_rs() {
    let manifest_dir = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"),
    );
    let repo_root = manifest_dir
        .parent()
        .expect("cli/ must live inside the repository")
        .to_path_buf();
    let lock = std::fs::read_to_string(repo_root.join("Cargo.lock"))
        .expect("Cargo.lock must be readable from the workspace root");

    // Parse the `[[package]] name = "esbuild-rs"` stanza for its version —
    // plain text scan, no lockfile parser needed for ONE field.
    // (Quét stanza `[[package]] name = "esbuild-rs"` lấy version — text
    // scan, không cần parser lockfile cho MỘT field.)
    let locked = lock
        .split("name = \"esbuild-rs\"")
        .nth(1)
        .and_then(|rest| rest.split("version = \"").nth(1))
        .and_then(|rest| rest.split('"').next())
        .unwrap_or("<esbuild-rs not found in Cargo.lock>");
    assert_eq!(
        locked, COMPILER_VERSION,
        "esbuild-rs in Cargo.lock is {locked} but COMPILER_VERSION is {COMPILER_VERSION} — \
         bump the const in cli/src/bundler/dev_server.rs together with the pin, or the \
         compiled cache serves stale entries across compiler versions (P0-3 drift gate)"
    );
}

/// Cross-project poison gate (Hướng B, P0-2 Tech Lead vòng-6/7): two
/// projects with IDENTICAL source bytes but DIFFERENT tsconfig semantics
/// must derive DIFFERENT compilation keys — the shared global compiled
/// cache can never serve one project's output to the other. The test runs
/// at the KEY level (canonical options JSON + CompilationKey), the exact
/// boundary where a cross-hit would occur.
///
/// Cổng chống độc chéo project (Hướng B, P0-2): hai project có bytes
/// source GIỐNG HỆT nhưng semantics tsconfig KHÁC phải ra compilation key
/// KHÁC — compiled cache toàn cục dùng chung không bao giờ được phục vụ
/// output của project này cho project kia. Test chạy ở tầng KEY (JSON
/// options chuẩn hóa + CompilationKey) — đúng biên nơi cross-hit xảy ra.
#[test]
fn different_tsconfig_projects_never_share_a_compilation_key() {
    let tmp = tempdir().unwrap();
    let project_a = tmp.path().join("a");
    let project_b = tmp.path().join("b");
    let source = "src/index.tsx";
    for project in [&project_a, &project_b] {
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(
            project.join(source),
            "export const App = () => <div>ok</div>;",
        )
        .unwrap();
    }
    // Same source bytes, DIFFERENT tsconfig semantics (jsx runtime + target).
    // (Bytes source giống, semantics tsconfig KHÁC (jsx runtime + target).)
    std::fs::write(
        project_a.join("tsconfig.json"),
        r#"{"compilerOptions":{"jsx":"react-jsx","target":"ES2020"}}"#,
    )
    .unwrap();
    std::fs::write(
        project_b.join("tsconfig.json"),
        r#"{"compilerOptions":{"jsx":"react","jsxFactory":"h","target":"ES5"}}"#,
    )
    .unwrap();

    let loader = mgc_store::cas::Loader::Tsx;
    let source_digest = blake3::Hasher::new()
        .update(b"export const App = () => <div>ok</div>;")
        .finalize()
        .to_hex()
        .to_string();

    let key_a = mgc_store::cas::CompilationKey::new(
        &source_digest,
        loader,
        COMPILER_IDENTITY,
        COMPILER_VERSION,
        &canonical_compile_options_ok(&project_a, &project_a.join(source)),
    )
    .unwrap();
    let key_b = mgc_store::cas::CompilationKey::new(
        &source_digest,
        loader,
        COMPILER_IDENTITY,
        COMPILER_VERSION,
        &canonical_compile_options_ok(&project_b, &project_b.join(source)),
    )
    .unwrap();
    assert_ne!(
        key_a, key_b,
        "same bytes + different tsconfig must NOT share a cache entry (P0-2 cross-project poison)"
    );

    // The tsconfig digest must invalidate on EDIT: same project, change
    // the tsconfig content → new key (no stale serve after a config edit).
    // (Digest tsconfig phải vô hiệu khi SỬA: cùng project, đổi nội dung
    // tsconfig → key mới (không serve stale sau khi sửa cấu hình).)
    std::fs::write(
        project_a.join("tsconfig.json"),
        r#"{"compilerOptions":{"jsx":"react-jsx","target":"ES2022"}}"#,
    )
    .unwrap();
    let key_a2 = mgc_store::cas::CompilationKey::new(
        &source_digest,
        loader,
        COMPILER_IDENTITY,
        COMPILER_VERSION,
        &canonical_compile_options_ok(&project_a, &project_a.join(source)),
    )
    .unwrap();
    assert_ne!(
        key_a, key_a2,
        "editing tsconfig.json must change the key (P0-2 stale-serve gate)"
    );
    // Restore the ORIGINAL content → the digest matches the original key
    // again (content-based digest — no spurious invalidation loop).
    // (Khôi phục nội dung GỐC → digest khớp key gốc (digest theo nội dung
    // — không lặp vô hiệu hóa giả).)
    std::fs::write(
        project_a.join("tsconfig.json"),
        r#"{"compilerOptions":{"jsx":"react-jsx","target":"ES2020"}}"#,
    )
    .unwrap();
    let key_a3 = mgc_store::cas::CompilationKey::new(
        &source_digest,
        loader,
        COMPILER_IDENTITY,
        COMPILER_VERSION,
        &canonical_compile_options_ok(&project_a, &project_a.join(source)),
    )
    .unwrap();
    assert_eq!(
        key_a, key_a3,
        "restoring tsconfig.json content must restore the original key (digest is content-based)"
    );
}

/// Same-content sources at DIFFERENT logical paths in one project get
/// different keys (sourcemap `sources` entries differ per path).
/// (Source cùng nội dung ở path logic KHÁC nhau trong 1 project có key
/// khác nhau (mục `sources` trong sourcemap khác theo path).)
#[test]
fn different_source_paths_get_distinct_keys() {
    let tmp = tempdir().unwrap();
    let project = tmp.path();
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(project.join("lib")).unwrap();
    let bytes = b"export const same = true;";
    std::fs::write(project.join("src/a.ts"), bytes).unwrap();
    std::fs::write(project.join("lib/b.ts"), bytes).unwrap();

    let source_digest = blake3::Hasher::new()
        .update(bytes)
        .finalize()
        .to_hex()
        .to_string();
    let key_src = mgc_store::cas::CompilationKey::new(
        &source_digest,
        mgc_store::cas::Loader::Ts,
        COMPILER_IDENTITY,
        COMPILER_VERSION,
        &canonical_compile_options_ok(project, &project.join("src/a.ts")),
    )
    .unwrap();
    let key_lib = mgc_store::cas::CompilationKey::new(
        &source_digest,
        mgc_store::cas::Loader::Ts,
        COMPILER_IDENTITY,
        COMPILER_VERSION,
        &canonical_compile_options_ok(project, &project.join("lib/b.ts")),
    )
    .unwrap();
    assert_ne!(
        key_src, key_lib,
        "equal bytes at different logical paths must not share a cache entry"
    );
}

/// Absent tsconfig is its own namespace: a project WITHOUT tsconfig must
/// not collide with one whose tsconfig exists (empty ≠ absent).
/// (Tsconfig vắng là namespace riêng: project KHÔNG có tsconfig không
/// được đụng project có tsconfig (rỗng ≠ vắng).)
#[test]
fn absent_tsconfig_is_distinct_from_any_present_one() {
    let tmp = tempdir().unwrap();
    let with_config = tmp.path().join("with");
    let without_config = tmp.path().join("without");
    std::fs::create_dir_all(with_config.join("src")).unwrap();
    std::fs::create_dir_all(without_config.join("src")).unwrap();
    std::fs::write(
        with_config.join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"ES2020"}}"#,
    )
    .unwrap();

    let opts_with = canonical_compile_options_ok(&with_config, &with_config.join("src/a.ts"));
    let opts_without =
        canonical_compile_options_ok(&without_config, &without_config.join("src/a.ts"));
    assert_ne!(
        opts_with, opts_without,
        "a project with tsconfig must never share a key with one without (P0-2)"
    );
}

/// Helper: canonical options that must build — panics with the tsconfig
/// failure otherwise (fail-closed contract is tested separately).
/// (Helper: options chuẩn phải dựng được — panic kèm lỗi tsconfig (hợp
/// đồng fail-closed được test riêng).)
#[cfg(test)]
fn canonical_compile_options_ok(root: &std::path::Path, source: &std::path::Path) -> String {
    canonical_compile_options(root, source).expect("canonical options must build")
}

/// Fail-closed tsconfig (Gate 11-C, vòng-11): a tsconfig that EXISTS but
/// cannot be read (chmod 000) must FAIL canonical options — never fold
/// into "absent" and never poison-and-compile. Skipped where chmod is not
/// enforceable (Windows/root).
/// (Tsconfig fail-closed: tsconfig TỒN TẠI nhưng không đọc được (chmod
/// 000) phải FAIL options — không gộp vào "absent" và không
/// poison-rồi-biên-dịch. Bỏ qua khi chmod không hiệu lực (Windows/root).)
#[test]
fn unreadable_tsconfig_fails_options_fail_closed() {
    let tmp = tempdir().unwrap();
    let project = tmp.path();
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(project.join("tsconfig.json"), r#"{"compilerOptions":{}}"#).unwrap();
    let mode_before = {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(project.join("tsconfig.json"))
            .unwrap()
            .permissions()
            .mode()
    };
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(project.join("tsconfig.json"))
            .unwrap()
            .permissions();
        perms.set_mode(0o000);
        std::fs::set_permissions(project.join("tsconfig.json"), perms).unwrap();
    }
    let result = canonical_compile_options(project, &project.join("src/a.ts"));
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(project.join("tsconfig.json"))
            .unwrap()
            .permissions();
        perms.set_mode(mode_before);
        std::fs::set_permissions(project.join("tsconfig.json"), perms).unwrap();
    }
    assert!(
        result.is_err(),
        "an unreadable tsconfig must fail canonical options (fail-closed), got {:?}",
        result
    );
}

// ── P1-1/P1-3 (fresh-context review 2026-09-15): request-security
// unit pins for the dev-server gates. The E2E lives at
// cli/tests/dev_server_request_security.rs; these pin the helpers
// directly so a regression is named at the exact gate that failed.
// (Ghim unit cho các cổng bảo mật request dev-server. E2E nằm ở
// cli/tests/dev_server_request_security.rs; các test này ghim trực tiếp
// từng helper để regression được gọi tên đúng cổng bị hỏng.)

/// The request-path gate: every traversal spelling is rejected, every
/// normal relative path passes.
/// (Cổng path-request: mọi cách viết traversal bị chặn, mọi path tương
/// đối bình thường được đi qua.)
#[test]
fn request_path_gate_blocks_traversal_spellings() {
    // Rejects: `..` in ANY component, `.` CurDir, absolute paths,
    // empty requests. (On Unix `C:` is a NORMAL component — the Windows
    // Prefix case only fires on Windows; the deps-route validator
    // covers drive letters cross-platform, and the join on Unix makes
    // `C:` an unreachable subdir, so it is not a Unix jailbreak.)
    // (Chặn: `..` ở BẤT KỲ component nào, `.` CurDir, path tuyệt đối,
    // request rỗng. (Trên Unix `C:` là component BÌNH THƯỜNG — case
    // Windows Prefix chỉ kích trên Windows; validator deps-route phủ
    // ổ đĩa cross-platform, và phép nối trên Unix biến `C:` thành subdir
    // không tới được, nên không phải jailbreak trên Unix.))
    for bad in [
        "../secret.txt",
        "src/../../secret.txt",
        "./main.ts",
        "/etc/passwd",
        "",
        "a/../b",
    ] {
        assert!(
            !request_path_is_safe(bad),
            "request path {bad:?} must be rejected by the traversal gate"
        );
    }
    // Windows drive prefix IS a component-level jailbreak there — pin
    // it where the semantics exist.
    // (Prefix ổ đĩa Windows là jailbreak tầng component ở đó — ghim
    // nơi semantics tồn tại.)
    #[cfg(target_os = "windows")]
    {
        assert!(!request_path_is_safe("C:/boot.ini"));
    }
    // Accepts: plain relative paths (the only legal request shape).
    // (Chấp nhận: path tương đối thuần (dạng request hợp pháp duy nhất).)
    for good in [
        "src/main.ts",
        "index.html",
        "public/logo.png",
        "deep/nested/file.css",
    ] {
        assert!(
            request_path_is_safe(good),
            "request path {good:?} must pass the traversal gate"
        );
    }
}

/// The JS string-literal embed (P1-3): CSS/error payloads ride as JSON
/// string literals — no template literal, so no interpolation, no
/// terminator, no escape-grammar race. What goes in as DATA cannot come
/// out as CODE.
/// (Nhúng string-literal JS (P1-3): payload CSS/lỗi đi dạng JSON string
/// literal — không template literal, nên không nội suy, không kết thúc,
/// không đua ngữ pháp escape. Vào là DỮ LIỆU thì không ra là CODE.)
#[test]
fn js_string_literal_is_jailproof() {
    // `${}` interpolation payload: embedded as plain text inside the
    // JSON string — a JS parser reads the two literal chars `$` `{`.
    // (Payload nội suy `${}`: nhúng dạng text thuần trong chuỗi JSON —
    // parser JS đọc 2 ký tự literal `$` `{`.)
    let css = "body { content: '${alert(1)}'; }";
    let lit = js_string_literal(css);
    assert!(
        lit.starts_with('"') && lit.ends_with('"'),
        "the embed must be a double-quoted string literal: {lit:?}"
    );
    // The payload sits INSIDE a double-quoted literal — `${}` has no
    // interpolation meaning there (only template literals interpolate),
    // and the literal must round-trip as pure data.
    // (Payload nằm TRONG literal trích dẫn kép — `${}` không có nghĩa
    // nội suy ở đó (chỉ template literal mới nội suy), và literal phải
    // round-trip như dữ liệu thuần.)
    let round: String = serde_json::from_str(&lit).expect("literal must be valid JSON");
    assert_eq!(round, css, "the interpolation payload must survive as DATA");
    // And there is NO template literal in the embed at all: no backtick
    // wraps the payload (the old injection vector).
    // (Và KHÔNG có template literal nào trong embed: không backtick bọc
    // payload (vector injection cũ).)
    assert!(
        !lit.starts_with('`') && !lit.ends_with('`'),
        "the embed must not use template-literal quoting: {lit:?}"
    );
    // String TERMINATORS are escaped: no raw `"` and no unbalanced `\`.
    // (Kết thúc chuỗi bị escape: không `"` thô và không `\` lơ lửng.)
    let nasty = "a \" b \\ c ` d ${e} f\n";
    let lit2 = js_string_literal(nasty);
    // serde_json escapes the inner quotes; the literal still round-trips.
    // (serde_json escape trích dẫn trong; literal vẫn round-trip.)
    let round: String = serde_json::from_str(&lit2).expect("literal must be valid JSON");
    assert_eq!(round, nasty, "the embed must round-trip the payload");
    // Raw backticks are irrelevant in a string literal (they are just
    // letters) — pinned so a future "simplification" back to template
    // literals is caught.
    // (Backtick thô vô nghĩa trong string literal (chỉ là ký tự) — ghim
    // để bắt ai "đơn giản hóa" quay lại template literal.)
    assert!(lit2.contains("` d "));
}

/// HTML text escaping (P1-3): the folder-name <title> cannot inject
/// markup.
/// (Escape text HTML: <title> từ tên thư mục không thể bơm markup.)
#[test]
fn html_text_escape_kills_markup() {
    let esc = html_escape_text("<script>alert(1)</script>");
    assert_eq!(esc, "&lt;script&gt;alert(1)&lt;/script&gt;");
    let esc2 = html_escape_text("a & b");
    assert_eq!(esc2, "a &amp; b");
}
