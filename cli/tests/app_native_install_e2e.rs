//! Native `install-app` E2E: flutter install MUST run through MGC itself
//! (pubspec parse → pub.dev resolve → verified fetch → mgc.lock + pub
//! cache). ZERO `flutter` spawn — canary fakes prove it. Live pub.dev,
//! online by design.
//! (Install-app native E2E: chạy trong MGC, không spawn flutter.)

#![allow(clippy::unwrap_used)]

use std::process::Command;
use tempfile::TempDir;

fn mgc_binary() -> String {
    std::env::var_os("CARGO_BIN_EXE_mgc")
        .map(std::path::PathBuf::from)
        .map(|p| p.to_string_lossy().to_string())
        .expect("CARGO_BIN_EXE_mgc unavailable — run via cargo test")
}

struct NoSpawnSandbox {
    bin_dir: TempDir,
    home_dir: TempDir,
    canary_log: std::path::PathBuf,
}

impl NoSpawnSandbox {
    fn multi(tools: &[&str]) -> Self {
        let bin_dir = TempDir::new().unwrap();
        let home_dir = TempDir::new().unwrap();
        let log_dir = bin_dir.path().join(".canary");
        std::fs::create_dir_all(&log_dir).unwrap();
        let log = log_dir.join("spawned.log");
        for tool in tools {
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
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&sh).unwrap().permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&sh, perms).unwrap();
            }
        }
        Self {
            bin_dir,
            home_dir,
            canary_log: log,
        }
    }

    fn run_mgc(&self, args: &[&str], cwd: &std::path::Path) -> (Option<i32>, String, String) {
        let mut cmd = Command::new(mgc_binary());
        cmd.args(args)
            .current_dir(cwd)
            .env("PATH", self.path_env())
            .env("HOME", self.home_dir.path())
            .env_remove("MGC_COMPAT_RUNTIME");
        let out = cmd.output().expect("spawn mgc");
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    }

    fn path_env(&self) -> std::ffi::OsString {
        let mut paths = vec![self.bin_dir.path().to_path_buf()];
        if let Some(existing) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&existing));
        }
        std::env::join_paths(paths).unwrap()
    }

    fn marker_text(&self) -> String {
        std::fs::read_to_string(&self.canary_log).unwrap_or_default()
    }
}

fn flutter_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"nativeapp\"\necosystem = \"app\"\n[app]\nlanguage = \"flutter\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pubspec.yaml"),
        "name: nativeapp\nenvironment:\n  sdk: \">=3.0.0 <4.0.0\"\ndependencies:\n  meta: ^1.12.0\n",
    )
    .unwrap();
}

#[test]
fn flutter_install_app_runs_inside_mgc_without_spawning_flutter() {
    let project = TempDir::new().unwrap();
    flutter_project(project.path());
    let sandbox = NoSpawnSandbox::multi(&["flutter", "dart"]);

    let (code, stdout, stderr) = sandbox.run_mgc(&["install-app"], project.path());
    let output = format!("{stdout}{stderr}");
    assert_eq!(
        code,
        Some(0),
        "native flutter `install-app` must succeed:\n{output}"
    );
    assert!(
        sandbox.marker_text().is_empty(),
        "ZERO toolchain spawn allowed, got:\n{}",
        sandbox.marker_text()
    );
    let lock = std::fs::read_to_string(project.path().join("mgc.lock")).unwrap_or_default();
    assert!(
        lock.contains("meta") && lock.contains("sha256-"),
        "mgc.lock must record meta with registry integrity:\n{lock}"
    );
}

fn real_swift() -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("swift"))
        .find(|p| p.is_file())
}

/// Craft a minimal static SwiftPM registry (scope.json + sha256 + zips)
/// served over HTTP for a hermetic native-update E2E (no outside network).
fn serve_fixture_registry(dir: &std::path::Path) -> (std::process::Child, String) {
    // Minimal Package.swift at zip root (no transitive deps).
    fn make_zip() -> Vec<u8> {
        // Stored-method zip with one file (built by hand, no deps).
        let name = b"Package.swift";
        let data = b"// swift-tools-version: 5.9\nimport PackageDescription\nlet package = Package(name: \"lib\")\n";
        let mut out = Vec::new();
        let crc = crc32_ieee(data);
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(data);
        let cd_offset = out.len() as u32;
        out.extend_from_slice(b"PK\x01\x02");
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(name);
        let cd_size = out.len() as u32 - cd_offset;
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }
    fn crc32_ieee(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }
    fn sha256_hex(data: &[u8]) -> String {
        // Pure-Rust SHA-256 (test-only, no new deps).
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut h: [u32; 8] = [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
            0x5be0cd19,
        ];
        let mut msg = data.to_vec();
        let bit_len = (data.len() as u64) * 8;
        msg.push(0x80);
        while msg.len() % 64 != 56 {
            msg.push(0);
        }
        msg.extend_from_slice(&bit_len.to_be_bytes());
        for chunk in msg.chunks(64) {
            let mut w = [0u32; 64];
            for i in 0..16 {
                w[i] = u32::from_be_bytes([
                    chunk[4 * i],
                    chunk[4 * i + 1],
                    chunk[4 * i + 2],
                    chunk[4 * i + 3],
                ]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16]
                    .wrapping_add(s0)
                    .wrapping_add(w[i - 7])
                    .wrapping_add(s1);
            }
            let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
                (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
            for i in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ ((!e) & g);
                let t1 = hh
                    .wrapping_add(s1)
                    .wrapping_add(ch)
                    .wrapping_add(K[i])
                    .wrapping_add(w[i]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let t2 = s0.wrapping_add(maj);
                hh = g;
                g = f;
                f = e;
                e = d.wrapping_add(t1);
                d = c;
                c = b;
                b = a;
                a = t1.wrapping_add(t2);
            }
            h[0] = h[0].wrapping_add(a);
            h[1] = h[1].wrapping_add(b);
            h[2] = h[2].wrapping_add(c);
            h[3] = h[3].wrapping_add(d);
            h[4] = h[4].wrapping_add(e);
            h[5] = h[5].wrapping_add(f);
            h[6] = h[6].wrapping_add(g);
            h[7] = h[7].wrapping_add(hh);
        }
        h.iter().map(|w| format!("{w:08x}")).collect()
    }
    let reg = dir.join("registry");
    let scope = reg.join("scope").join("lib");
    std::fs::create_dir_all(&scope).unwrap();
    std::fs::write(
        scope.join("scope.json"),
        r#"{"versions":["1.0.0","1.1.0"]}"#,
    )
    .unwrap();
    for v in ["1.0.0", "1.1.0"] {
        let zip = make_zip();
        std::fs::write(scope.join(format!("{v}.zip")), &zip).unwrap();
        std::fs::write(scope.join(format!("{v}.sha256")), sha256_hex(&zip)).unwrap();
    }
    // Free port, then serve.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let child = std::process::Command::new("python3")
        .args([
            "-m",
            "http.server",
            &port.to_string(),
            "--bind",
            "127.0.0.1",
        ])
        .current_dir(&reg)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("python3 http.server");
    std::thread::sleep(std::time::Duration::from_millis(800));
    (child, format!("http://127.0.0.1:{port}"))
}

fn swift_registry_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"nativeswift\"\necosystem = \"app\"\n[app]\nlanguage = \"swift\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Package.swift"),
        concat!(
            "// swift-tools-version: 5.9\n",
            "import PackageDescription\n\n",
            "let package = Package(\n",
            "    name: \"nativeswift\",\n",
            "    dependencies: [\n",
            "        .package(id: \"scope.lib\", from: \"1.0.0\"),\n",
            "    ],\n",
            "    targets: [\n",
            "        .target(name: \"nativeswift\", dependencies: [\n",
            "            .product(name: \"Lib\", package: \"scope.lib\"),\n",
            "        ]),\n",
            "    ]\n",
            ")\n",
        ),
    )
    .unwrap();
}

/// Swift native update end-to-end against a LOCAL static registry:
/// resolve-latest → Package.swift bump (verified by re-scan) → lock.
/// The only toolchain use is manifest READING (dump-package); every
/// lifecycle step runs inside mgc. Skipped without swift/python3.
#[test]
fn swift_update_native_against_local_registry() {
    let real = real_swift();
    let has_python = std::process::Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if real.is_none() || !has_python {
        eprintln!("SKIP: swift toolchain or python3 absent");
        return;
    }
    let real = real.unwrap();
    let project = TempDir::new().unwrap();
    swift_registry_project(project.path());
    let regdir = TempDir::new().unwrap();
    let mut server = serve_fixture_registry(regdir.path());
    let base = server.1.clone();

    // Shim swift: dump-package (manifest READING) delegates to the real
    // toolchain; every other swift invocation fails closed. This proves
    // the lifecycle runs inside mgc — the toolchain never resolves,
    // fetches, or updates anything.
    let shim_dir = TempDir::new().unwrap();
    let log = shim_dir.path().join("spawned.log");
    std::fs::write(
        shim_dir.path().join("swift"),
        format!(
            "#!/bin/sh\nif printf '%s' \"$*\" | grep -q dump-package; then exec {} \"$@\"; else printf '%s\\n' \"swift $*\" >> {}; exit 1; fi\n",
            real.display(),
            log.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let sh = shim_dir.path().join("swift");
        let mut perms = std::fs::metadata(&sh).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&sh, perms).unwrap();
    }
    let mut paths = vec![shim_dir.path().to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    let path_env = std::env::join_paths(paths).unwrap();
    let home = TempDir::new().unwrap();

    let run = |args: &[&str]| -> (Option<i32>, String) {
        let out = std::process::Command::new(mgc_binary())
            .args(args)
            .current_dir(project.path())
            .env("PATH", &path_env)
            .env("HOME", home.path())
            .env("MGC_SWIFT_REGISTRY_URL", &base)
            .env_remove("MGC_COMPAT_RUNTIME")
            .output()
            .expect("spawn mgc");
        (
            out.status.code(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    };
    let (code, out) = run(&["update-app", "scope/lib"]);
    // Kill the registry server either way.
    let _ = server.0.kill();
    assert_eq!(code, Some(0), "native swift update must succeed:\n{out}");
    let pkg = std::fs::read_to_string(project.path().join("Package.swift")).unwrap();
    assert!(
        pkg.contains("from: \"1.1.0\""),
        "Package.swift must pin 1.1.0:\n{pkg}"
    );
    let log_text = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(
        !log_text.contains("package update") && !log_text.contains("package resolve"),
        "no lifecycle swift spawn allowed (dump-package reads ok):\n{log_text}"
    );
    let lock = std::fs::read_to_string(project.path().join("mgc.lock")).unwrap_or_default();
    assert!(
        lock.contains("1.1.0"),
        "mgc.lock must record 1.1.0:\n{lock}"
    );
}

fn kotlin_catalog_project(dir: &std::path::Path) {
    std::fs::write(
        dir.join("mgc.toml"),
        "name = \"nativekotlin\"\necosystem = \"app\"\n[app]\nlanguage = \"kotlin\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("settings.gradle.kts"),
        "rootProject.name = \"nativekotlin\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("gradle")).unwrap();
    std::fs::write(
        dir.join("gradle/libs.versions.toml"),
        "[versions]\nlang3 = \"3.12.0\"\n\n[libraries]\ncommons-lang3 = { module = \"org.apache.commons:commons-lang3\", version.ref = \"lang3\" }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("build.gradle.kts"),
        "plugins { kotlin(\"jvm\") version \"2.0.0\" }\ndependencies {\n    implementation(libs.commons.lang3)\n}\n",
    )
    .unwrap();
}

/// Kotlin native update through the version catalog (no JVM, no gradle
/// spawn — pure TOML + Maven Central resolve). canary-shims the whole
/// toolchain set to prove it.
#[test]
fn kotlin_update_native_through_catalog() {
    let project = TempDir::new().unwrap();
    kotlin_catalog_project(project.path());
    let sandbox = NoSpawnSandbox::multi(&[
        "gradle",
        "java",
        "kotlinc",
        "flutter",
        "dart",
        "swift",
        "pod",
        "xcodebuild",
    ]);

    let (code, stdout, stderr) = sandbox.run_mgc(&["update-app", "commons-lang3"], project.path());
    let output = format!("{stdout}{stderr}");
    assert_eq!(
        code,
        Some(0),
        "native kotlin update must succeed:\n{output}"
    );
    let catalog =
        std::fs::read_to_string(project.path().join("gradle/libs.versions.toml")).unwrap();
    assert!(
        !catalog.contains("3.12.0"),
        "old pin must be gone:\n{catalog}"
    );
    // Latest commons-lang3 as resolved live (floats with the registry
    // by design — the bump fn itself fails closed unless the re-parse
    // reads the new pin back).
    let bumped = catalog
        .lines()
        .find(|l| l.trim_start().starts_with("lang3"))
        .unwrap_or_default()
        .to_string();
    let version = bumped.split('"').nth(1).unwrap_or_default().to_string();
    assert!(
        !version.is_empty() && version != "3.12.0",
        "ref must point past 3.12.0, got {version:?}:\n{catalog}"
    );
    assert!(
        sandbox.marker_text().is_empty(),
        "ZERO toolchain spawn allowed, got:\n{}",
        sandbox.marker_text()
    );
}
