// Compile-fail regression (P0-1): struct-literal + field-write bypasses of
// the private IntegrityHash fields MUST be rejected by the compiler.
//
// This harness drives `rustc` DIRECTLY against the already-built mgc-store
// rlib (no cargo child process, no dependency resolution, no lockfile) —
// deterministic in every context: standalone, `cargo test --workspace`,
// parallel runs, offline environments. trybuild was tried first but its
// internal `cargo build --offline` child is fragile under workspace-level
// cache contention.
//
// Test regression compile-fail (P0-1): struct-literal + ghi-field bypass
// field private của IntegrityHash PHẢI bị compiler từ chối.
//
// Harness này gọi `rustc` TRỰC TIẾP vào rlib mgc-store đã build (không
// process cargo con, không resolve dependency, không lockfile) — tất định
// trong mọi ngữ cảnh: chạy riêng, `cargo test --workspace`, chạy song song,
// môi trường offline. Đã thử trybuild trước nhưng process con
// `cargo build --offline` bên trong nó mong manh dưới contention cache
// ở cấp workspace.
#![allow(clippy::unwrap_used)] // Test harness: unwraps fail loudly on setup errors.
// (Bộ test: unwrap hét to khi setup lỗi.)

use std::path::PathBuf;
use std::process::Command;

/// Locate the built mgc-store rlib via the env cargo sets for test runs.
/// (Tìm rlib mgc-store đã build qua env mà cargo set cho test run.)
fn find_mgc_store_rlib() -> Option<PathBuf> {
    // CARGO_MANIFEST_DIR = core/crates/mgc-store in every context; the rlib
    // lives under <workspace target>/debug/deps. Walk candidate target dirs
    // (crate-local, nested core ws, root ws) and pick the NEWEST hashed rlib
    // (a stale one may lack the private fields).
    // (CARGO_MANIFEST_DIR = core/crates/mgc-store trong mọi ngữ cảnh; rlib
    // nằm dưới <workspace target>/debug/deps. Duyệt các target ứng viên
    // (crate-local, core ws lồng, root ws) và chọn rlib có hash MỚI NHẤT
    // (bản cũ có thể chưa có field private).)
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let roots = [
        manifest.join("target"),                  // crate-local target (rare)
        manifest.join("../..").join("target"),    // core/target (nested ws)
        manifest.join("../../..").join("target"), // root/target (main ws)
    ];
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for root in &roots {
        let deps = root.join("debug").join("deps");
        let Ok(rd) = std::fs::read_dir(&deps) else {
            continue;
        };
        for e in rd.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().to_string();
            // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
            if (name == "libmgc_store.rlib"
                || (name.starts_with("libmgc_store-") && name.ends_with(".rlib")))
                && let Ok(mt) = e.metadata().and_then(|m| m.modified())
                && best.as_ref().is_none_or(|(t, _)| mt > *t)
            {
                best = Some((mt, e.path()));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Compile `source` against the built rlib and return stderr text.
/// (Compile `source` với rlib đã build rồi trả text stderr.)
fn rustc_compile(source: &str) -> String {
    let rlib = find_mgc_store_rlib().expect(
        "mgc-store rlib must be built before compile-fail tests (run cargo build -p mgc-store)",
    );
    let tmp = std::env::temp_dir().join(format!(
        "mgc-p01-compile-fail-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    let src = tmp.join("bypass.rs");
    std::fs::write(&src, source).unwrap();

    let out = Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
        .arg("--edition=2024")
        .arg("--crate-type=bin")
        .arg("-L")
        .arg(rlib.parent().expect("rlib has parent"))
        .arg("--extern")
        .arg(format!("mgc_store={}", rlib.display()))
        .arg("-o")
        .arg(tmp.join("bypass_bin"))
        .arg(&src)
        .output()
        .expect("rustc must be runnable");

    let _ = std::fs::remove_dir_all(&tmp);
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[test]
fn struct_literal_bypass_is_rejected_by_compiler() {
    let stderr = rustc_compile(
        r#"
// Struct literal bypass: fields are private (P0-1) — must be E0451.
// (Bypass struct literal: field đã private (P0-1) — phải ra E0451.)
use mgc_store::cas::IntegrityHash;
fn main() {
    let _forged = IntegrityHash {
        hash: "x".to_string(),
        executable: false,
    };
}
"#,
    );
    assert!(
        stderr.contains("E0451"),
        "struct-literal construction must fail with E0451 (private fields), got:\n{stderr}"
    );
    assert!(
        stderr.contains("private"),
        "the error must name the fields private, got:\n{stderr}"
    );
}

#[test]
fn field_write_bypass_is_rejected_by_compiler() {
    let stderr = rustc_compile(
        r#"
// Field-write bypass: fields are private (P0-1) — must be E0616.
// (Bypass ghi field: field đã private (P0-1) — phải ra E0616.)
use mgc_store::cas::IntegrityHash;
fn main() {
    let mut h = IntegrityHash::from_bytes(b"data", false);
    h.hash = "short".to_string();
    let _ = h;
}
"#,
    );
    assert!(
        stderr.contains("E0616"),
        "field write must fail with E0616 (field is private), got:\n{stderr}"
    );
}

#[test]
fn field_read_bypass_is_rejected_by_compiler() {
    let stderr = rustc_compile(
        r#"
// Field-read bypass: fields are private (P0-1) — must be E0616 too.
// (Bypass đọc field: field đã private (P0-1) — cũng phải ra E0616.)
use mgc_store::cas::IntegrityHash;
fn main() {
    let h = IntegrityHash::from_bytes(b"data", false);
    let raw: String = h.hash;
    let _ = raw;
}
"#,
    );
    assert!(
        stderr.contains("E0616"),
        "field read must fail with E0616 (field is private), got:\n{stderr}"
    );
}

#[test]
fn compilation_key_struct_literal_bypass_is_rejected() {
    // P0-1 (vòng-8): CompilationKey fields are now PRIVATE — a struct
    // literal forging a cache address must fail with E0451, mirroring the
    // IntegrityHash proofs above.
    // (P0-1 (vòng-8): field CompilationKey giờ PRIVATE — struct literal
    // giả địa chỉ cache phải fail với E0451, phản chiếu bằng chứng
    // IntegrityHash phía trên.)
    let stderr = rustc_compile(
        r#"
// Struct-literal bypass of the private CompilationKey fields (P0-1).
// (Bypass struct-literal field private của CompilationKey (P0-1).)
use mgc_store::cas::{CompilationKey, Loader};
fn main() {
    let _forged = CompilationKey {
        schema_version: 2,
        source_digest: "x".to_string(),
        loader: Loader::Ts,
        compiler: "fake".to_string(),
        compiler_version: "9.9".to_string(),
        options_digest: "y".to_string(),
    };
}
"#,
    );
    assert!(
        stderr.contains("E0451"),
        "CompilationKey struct literal must fail with E0451 (private fields), got:\n{stderr}"
    );
}

#[test]
fn compilation_key_field_tampering_is_rejected() {
    // P0-1 (vòng-8): reading/writing a private CompilationKey field from
    // OUTSIDE the crate must fail with E0616.
    // (P0-1 (vòng-8): đọc/ghi field private CompilationKey TỪ NGOÀI crate
    // phải fail với E0616.)
    let stderr = rustc_compile(
        r#"
// Field tampering on a validated key (P0-1) — must be E0616.
// (Sửa field trên key đã validate (P0-1) — phải ra E0616.)
use mgc_store::cas::{CompilationKey, Loader};
fn main() {
    let mut key = CompilationKey::new(
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        Loader::Ts,
        "esbuild-rs",
        "0.13.8",
        "{}",
    )
    .unwrap();
    key.compiler_version = "0.0.0-forged".to_string();
}
"#,
    );
    assert!(
        stderr.contains("E0616"),
        "CompilationKey field write must fail with E0616 (field is private), got:\n{stderr}"
    );
}
