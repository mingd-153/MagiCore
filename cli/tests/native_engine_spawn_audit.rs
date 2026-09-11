//! `native_engine_spawn_audit.rs` — Process-spawn audit (Tech Lead
//! 2026-09-10 #7, P0-1 rewrite 2026-09-10): CI-proof that `mgc
//! dev/test/run/build` NEVER spawn rival runtimes or external PMs
//! outside compatibility mode.
//!
//! Mechanism: HERMETIC PATH CANARY. A fake `bun`/`deno` executable is
//! planted at the FRONT of a sandbox PATH — if any code path spawns the
//! rival runtime, the canary writes a MARKER FILE. Native mode must
//! leave NO marker; only `--compat-runtime <runtime>` may open the
//! gate, and then the canary marker MUST exist (proof the gate — not
//! silence — is what we observed). `--help` greps prove nothing and are
//! no longer accepted as evidence.
//!
//! Kiểm toán process-spawn: chứng minh CI rằng mgc dev/test/run/build
//! KHÔNG BAO GIỜ spawn runtime đối thủ/PM ngoài chế độ compat. Cơ chế:
//! CANARY PATH HERMETIC — fake bun/deno đứng đầu PATH sandbox; spawn
//! thật sẽ ghi MARKER FILE. Native không được để marker; chỉ
//! --compat-runtime mới mở cổng và khi đó marker PHẢI có (chứng minh
//! quan sát được cổng, không phải im lặng). Grep --help không còn được
//! coi là evidence.

#![allow(clippy::unwrap_used)]

use std::process::Command;
use tempfile::TempDir;

fn mgc_binary() -> String {
    std::env::var_os("CARGO_BIN_EXE_mgc")
        .map(std::path::PathBuf::from)
        .map(|p| p.to_string_lossy().to_string())
        .expect("CARGO_BIN_EXE_mgc unavailable — run via cargo test")
}

fn node_project_with_test_script(dir: &std::path::Path, script: &str) {
    std::fs::write(
        dir.join("package.json"),
        format!(r#"{{"name": "spawn-audit", "scripts": {{"test": "{script}"}}}}"#),
    )
    .unwrap();
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"spawn-audit\"\necosystem = \"web\"\n",
    )
    .unwrap();
}

fn run_mgc(args: &[&str], cwd: &std::path::Path) -> (Option<i32>, String, String) {
    let out = Command::new(mgc_binary())
        .args(args)
        .current_dir(cwd)
        // SAFETY: env_remove on a Command only affects the child
        // process's environment — no ambient mutation, no race.
        // SAFETY: env_remove trên Command chỉ ảnh hưởng môi trường process
        // con — không đụng env toàn cục, không race.
        .env_remove("MGC_COMPAT_RUNTIME")
        .output()
        .expect("spawn mgc");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

// ---------------------------------------------------------------------------
// Hermetic canary tooling — fake rival runtimes that leave proof behind.
// ---------------------------------------------------------------------------

/// Build a sandbox with a fake `bun`/`deno` canary at the FRONT of PATH.
/// The canary appends a line to `<dir>/.canary/spawned.log` whenever the
/// mgc-under-test (or any child) resolves it from PATH. This proves an
/// actual process spawn — a refusal message on stderr proves the opposite.
/// Xây sandbox với canary bun/deno giả đứng ĐẦU PATH. Canary ghi dòng vào
/// spawned.log mỗi khi bị resolve từ PATH — chứng minh spawn process thật.
struct CanarySandbox {
    #[allow(dead_code)]
    bin_dir: TempDir,
    canary_log: std::path::PathBuf,
}

impl CanarySandbox {
    fn new(tool: &str) -> Self {
        let bin_dir = TempDir::new().unwrap();
        let log_dir = bin_dir.path().join(".canary");
        std::fs::create_dir_all(&log_dir).unwrap();
        let log = log_dir.join("spawned.log");
        // UNIX shell script canary; Windows uses a .cmd twin (line endings
        // are irrelevant to cmd's parser here).
        let sh = bin_dir.path().join(tool);
        std::fs::write(
            &sh,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"{} $*\" >> {}\nexit 0\n",
                tool,
                log.display()
            ),
        )
        .unwrap();
        make_executable(&sh);
        #[cfg(windows)]
        {
            let cmd = bin_dir.path().join(format!("{tool}.cmd"));
            std::fs::write(
                &cmd,
                format!(
                    "@echo off\necho {} %* >> {}\nexit /b 0\n",
                    tool,
                    log.display()
                ),
            )
            .unwrap();
        }
        Self {
            bin_dir,
            canary_log: log,
        }
    }

    fn path_env(&self) -> std::ffi::OsString {
        prepend_dir_to_path(self.bin_dir.path())
    }

    /// Marker lines written by the canary (empty = nothing spawned).
    fn marker_text(&self) -> String {
        std::fs::read_to_string(&self.canary_log).unwrap_or_default()
    }
}

fn make_executable(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).unwrap();
    }
    #[cfg(not(unix))]
    let _ = path;
}

fn prepend_dir_to_path(dir: &std::path::Path) -> std::ffi::OsString {
    let mut paths = vec![dir.to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths).unwrap()
}

/// Run mgc with the canary PATH planted — the ONLY way a rival runtime
/// could run in this test is by resolving our fake from PATH.
/// Chạy mgc với PATH canary — runtime đối thủ chỉ có thể chạy bằng cách
/// resolve fake của ta từ PATH.
fn run_mgc_with_canary(
    args: &[&str],
    cwd: &std::path::Path,
    sandbox: &CanarySandbox,
) -> (Option<i32>, String, String, String) {
    let out = Command::new(mgc_binary())
        .args(args)
        .current_dir(cwd)
        .env("PATH", sandbox.path_env())
        .env_remove("MGC_COMPAT_RUNTIME")
        .output()
        .expect("spawn mgc");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        sandbox.marker_text(),
    )
}

fn web_project_with_dev_script(dir: &std::path::Path, dev_script: &str) {
    std::fs::write(
        dir.join("package.json"),
        format!(
            r#"{{"name": "dev-canary", "scripts": {{"dev": "{dev_script}", "build": "{dev_script}", "test": "{dev_script}"}}}}"#
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"dev-canary\"\necosystem = \"web\"\n",
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Native rejection: mgc dev — process-level canary, no --help greps.
// ---------------------------------------------------------------------------

#[test]
fn mgc_dev_never_spawns_bun_or_deno_by_default_process_canary() {
    // P0-1 evidence: a project whose dev script names bun/deno MUST be
    // refused in native mode AND the canary must stay silent — no rival
    // process ever resolves from PATH. Refusal must point at migration.
    // Project dev script là bun/deno → native TỪ CHỐI + canary im lặng.
    for tool in ["bun", "deno"] {
        let project = TempDir::new().unwrap();
        let script = if tool == "bun" {
            "bun run dev-server"
        } else {
            "deno run dev-server.ts"
        };
        web_project_with_dev_script(project.path(), script);
        let sandbox = CanarySandbox::new(tool);

        let (code, stdout, stderr, marker) =
            run_mgc_with_canary(&["dev"], project.path(), &sandbox);
        let output = format!("{stdout}{stderr}");
        assert_ne!(
            code,
            Some(0),
            "native 'mgc dev' must refuse the {tool} script, got exit {code:?}:\n{output}"
        );
        assert!(
            marker.is_empty(),
            "{tool} canary must NEVER fire under native 'mgc dev' (marker):\n{marker}\noutput:\n{output}"
        );
        assert!(
            output.contains("NATIVE") || output.contains("refusing to spawn"),
            "refusal must explain the native-engine contract:\n{output}"
        );
    }
}

#[test]
fn mgc_dev_compat_flag_gates_rival_runtime_explicitly() {
    // Explicit compat flag OPENS the gate for the named runtime only —
    // the canary MUST fire (proof the spawn happened through the gate,
    // with the loud warning), and the OTHER runtime's canary stays silent.
    // Cờ compat mở cổng CHO ĐÚNG runtime — canary của runtime đó PHẢI
    // chạy (spawn qua cổng + cảnh báo), runtime kia im lặng.
    for (flag, allowed, denied) in [("bun", "bun", "deno"), ("deno", "deno", "bun")] {
        let project = TempDir::new().unwrap();
        let script = if allowed == "bun" {
            "bun run dev-server"
        } else {
            "deno run dev-server.ts"
        };
        web_project_with_dev_script(project.path(), script);
        let allowed_canary = CanarySandbox::new(allowed);
        // Deny-path canary shares the marker file naming but a separate
        // bin dir: if the gate leaks to the other runtime, this fires.
        let denied_canary = CanarySandbox::new(denied);

        // PATH chứa CẢ HAI canary — chỉ runtime được chọn mới được phép chạy.
        let out = Command::new(mgc_binary())
            .args(["dev", "--compat-runtime", flag])
            .current_dir(project.path())
            .env("PATH", {
                let mut paths = vec![
                    allowed_canary.bin_dir.path().to_path_buf(),
                    denied_canary.bin_dir.path().to_path_buf(),
                ];
                if let Some(existing) = std::env::var_os("PATH") {
                    paths.extend(std::env::split_paths(&existing));
                }
                std::env::join_paths(paths).unwrap()
            })
            .env_remove("MGC_COMPAT_RUNTIME")
            .output()
            .expect("spawn mgc");
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let output = format!("{stdout}{stderr}");

        assert!(
            allowed_canary.marker_text().contains(allowed),
            "compat '--compat-runtime {flag}' must actually spawn {allowed} through the gate (marker):\n{}",
            allowed_canary.marker_text()
        );
        assert!(
            denied_canary.marker_text().is_empty(),
            "compat '{flag}' must NOT spawn the other runtime {denied}:\n{}",
            denied_canary.marker_text()
        );
        assert!(
            output.contains("COMPATIBILITY MODE"),
            "every compat spawn must print the loud warning:\n{output}"
        );
    }
}

#[test]
fn mgc_dev_mgc_compat_runtime_env_is_escape_hatch_gated() {
    // MGC_COMPAT_RUNTIME env là escape hatch tương đương cờ — same gate
    // semantics: opens ONLY the named runtime, warning printed, other
    // runtime canary silent.
    // Env MGC_COMPAT_RUNTIME là escape hatch tương đương cờ — chỉ mở
    // đúng runtime được chọn, in cảnh báo, runtime kia im lặng.
    let project = TempDir::new().unwrap();
    web_project_with_dev_script(project.path(), "deno run dev-server.ts");
    let deno_canary = CanarySandbox::new("deno");
    let bun_canary = CanarySandbox::new("bun");

    let out = Command::new(mgc_binary())
        .args(["dev"])
        .current_dir(project.path())
        .env("PATH", {
            let mut paths = vec![
                deno_canary.bin_dir.path().to_path_buf(),
                bun_canary.bin_dir.path().to_path_buf(),
            ];
            if let Some(existing) = std::env::var_os("PATH") {
                paths.extend(std::env::split_paths(&existing));
            }
            std::env::join_paths(paths).unwrap()
        })
        .env("MGC_COMPAT_RUNTIME", "deno")
        .output()
        .expect("spawn mgc");
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        deno_canary.marker_text().contains("deno"),
        "MGC_COMPAT_RUNTIME=deno must open the deno gate (marker):\n{}",
        deno_canary.marker_text()
    );
    assert!(
        bun_canary.marker_text().is_empty(),
        "MGC_COMPAT_RUNTIME=deno must not leak to bun:\n{}",
        bun_canary.marker_text()
    );
    assert!(
        output.contains("COMPATIBILITY MODE"),
        "env escape hatch must warn like the flag:\n{output}"
    );
}

#[test]
fn mgc_test_never_spawns_bun_deno_or_pm_from_package_script() {
    // Each forbidden program must be REFUSED with the migration error —
    // and the canary output (`SPAWNED-<tool>` printed by the tool) must
    // NOT appear on stdout, proving nothing was executed. The refusal
    // error ECHOES the script line (it names the offending program), so
    // the canary is asserted on STDOUT only — a real spawn would print
    // there, the refusal never does.
    // Mọi PM/runtime bị cấm phải BỊ TỪ CHỐI kèm lỗi migration — và canary
    // KHÔNG xuất hiện trên stdout, chứng minh không gì được chạy.
    for tool in ["bun", "deno", "npm", "npx", "pnpm", "yarn", "bunx"] {
        let sandbox = TempDir::new().unwrap();
        node_project_with_test_script(sandbox.path(), &format!("{tool} echo SPAWNED-{tool}"));

        let (code, stdout, stderr) = run_mgc(&["test"], sandbox.path());
        let output = format!("{stdout}{stderr}");
        assert_ne!(
            code,
            Some(0),
            "'mgc test' must refuse the {tool} script, got exit {code:?}:\n{output}"
        );
        assert!(
            !stdout.contains(&format!("SPAWNED-{tool}")),
            "the {tool} canary must never execute (stdout):\n{stdout}"
        );
        assert!(
            output.contains("NATIVE")
                || output.contains("never spawnable")
                || output.contains("forbidden")
                || output.contains("Unsupported script"),
            "the refusal must explain the native-engine contract for {tool}:\n{output}"
        );
    }
}

#[test]
fn mgc_run_never_spawns_rival_runtime_without_compat_flag() {
    let sandbox = TempDir::new().unwrap();
    std::fs::write(
        sandbox.path().join("package.json"),
        r#"{"name": "run-audit", "scripts": {"dev": "deno eval \"console.log('SPAWNED-DENO')\""}}"#,
    )
    .unwrap();
    std::fs::write(
        sandbox.path().join("mgc.toml"),
        "name = \"run-audit\"\necosystem = \"web\"\n",
    )
    .unwrap();

    let (code, stdout, stderr) = run_mgc(&["run", "dev"], sandbox.path());
    let output = format!("{stdout}{stderr}");
    assert_ne!(code, Some(0), "refusal expected:\n{output}");
    assert!(
        !stdout.contains("SPAWNED-DENO"),
        "deno must never execute without --compat-runtime (stdout):\n{stdout}"
    );
}

#[test]
fn mgc_run_compat_flag_gates_rival_runtime_explicitly() {
    // With the explicit flag the gate OPENS for the named runtime — the
    // hermetic canary MUST fire, proving the spawn happened through the
    // gate (not merely that a refusal is absent).
    // Có cờ tường minh cổng MỞ — canary hermetic PHẢI chạy, chứng minh
    // spawn đi qua cổng (không chỉ là "không thấy lỗi từ chối").
    let project = TempDir::new().unwrap();
    std::fs::write(
        project.path().join("package.json"),
        r#"{"name": "compat-audit", "scripts": {"dev": "deno eval \"console.log('x')\""}}"#,
    )
    .unwrap();
    std::fs::write(
        project.path().join("mgc.toml"),
        "name = \"compat-audit\"\necosystem = \"web\"\n",
    )
    .unwrap();
    let deno_canary = CanarySandbox::new("deno");

    let out = Command::new(mgc_binary())
        .args(["run", "dev", "--compat-runtime", "deno"])
        .current_dir(project.path())
        .env("PATH", deno_canary.path_env())
        .env_remove("MGC_COMPAT_RUNTIME")
        .output()
        .expect("spawn mgc");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        deno_canary.marker_text().contains("deno"),
        "explicit compat must actually spawn deno via the gate (marker):\n{}",
        deno_canary.marker_text()
    );
    assert!(
        !text.contains("refusing to spawn 'deno'"),
        "explicit compat flag must open the gate:\n{text}"
    );
}

#[test]
fn mgc_build_never_spawns_rival_runtime_or_pm_from_build_script() {
    // F-B evidence: build scripts naming bun/deno/pnpm must ALL be
    // refused (previously deno fell through silently → fake success).
    // Bun/deno/pnpm trong build script đều phải TỪ CHỐI (trước đây deno
    // rơi qua lưới → build giả thành công).
    for (tool, script) in [
        ("pnpm", "pnpm run build-real"),
        ("bun", "bun run build-real"),
        ("deno", "deno task build"),
    ] {
        let sandbox = TempDir::new().unwrap();
        std::fs::write(
            sandbox.path().join("package.json"),
            format!(r#"{{"name": "build-audit", "scripts": {{"build": "{script}"}}}}"#),
        )
        .unwrap();
        std::fs::write(
            sandbox.path().join("mgc.toml"),
            "name = \"build-audit\"\necosystem = \"web\"\n",
        )
        .unwrap();

        let (code, stdout, stderr) = run_mgc(&["build"], sandbox.path());
        let output = format!("{stdout}{stderr}");
        assert_ne!(
            code,
            Some(0),
            "'mgc build' must refuse the {tool}-delegating script:\n{output}"
        );
        assert!(
            output.contains("NATIVE")
                || output.contains("package manager")
                || output.contains("refusing to spawn"),
            "the refusal must name the {tool} delegation + native contract:\n{output}"
        );
    }
}

#[test]
fn mgc_build_compat_flag_gates_rival_runtime_for_build() {
    // Compat lane for build: bun build script runs THROUGH the gate with
    // the warning; native lane stays refused (covered above).
    // Lane compat cho build: script bun chạy QUA cổng kèm cảnh báo.
    let project = TempDir::new().unwrap();
    std::fs::write(
        project.path().join("package.json"),
        r#"{"name": "build-compat", "scripts": {"build": "bun run build-real"}}"#,
    )
    .unwrap();
    std::fs::write(
        project.path().join("mgc.toml"),
        "name = \"build-compat\"\necosystem = \"web\"\n",
    )
    .unwrap();
    let bun_canary = CanarySandbox::new("bun");

    let out = Command::new(mgc_binary())
        .args(["build", "--compat-runtime", "bun"])
        .current_dir(project.path())
        .env("PATH", bun_canary.path_env())
        .env_remove("MGC_COMPAT_RUNTIME")
        .output()
        .expect("spawn mgc");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        bun_canary.marker_text().contains("bun"),
        "compat build must spawn bun through the gate (marker):\n{}",
        bun_canary.marker_text()
    );
    assert!(
        text.contains("COMPATIBILITY MODE"),
        "compat build must warn loudly:\n{text}"
    );
}
