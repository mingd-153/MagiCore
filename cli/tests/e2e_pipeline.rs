#![allow(clippy::unwrap_used)]

//! E2E: registry server local → publish → install (18 §18 — không mạng thật)
//! Flow: pack → serve (mg-registry) → publish → install
//! (spawn binary mg-registry + mg — common::mg pattern)

mod common;

use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

fn free_port() -> Option<u16> {
    match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => Some(listener.local_addr().unwrap().port()),
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("skipping socket-backed e2e test in sandbox: {err}");
            None
        }
        Err(err) => panic!("failed to allocate e2e test port: {err}"),
    }
}

fn spawn_server(store_dir: &Path, port: u16) -> Child {
    let workspace_manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml");
    let workspace_root = workspace_manifest.parent().unwrap();
    let bin = workspace_root
        .join("target")
        .join("debug")
        .join("mg-registry");
    assert!(
        bin.exists(),
        "mg-registry binary missing — build the workspace first"
    );
    Command::new(bin)
        .arg("--port")
        .arg(port.to_string())
        .arg("--store-dir")
        .arg(store_dir)
        .arg("--admin-token")
        .arg(TEST_ADMIN_TOKEN)
        .spawn()
        .expect("spawn mg-registry")
}

const TEST_ADMIN_TOKEN: &str = "e2e-admin-token";

fn wait_ready(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if TcpListener::bind(("127.0.0.1", port)).is_err() {
            return; // port đã bị chiếm → server listen
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("mg-registry did not become ready within 15s");
}

/// Lấy cwd gốc workspace (chạy publish/install từ đây — không dính feature chain)
fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn e2e_publish_then_install() {
    let Some(port) = free_port() else {
        return;
    };
    let base = common::work_dir();
    let store = base.join("registry-store");
    let publisher = base.join("publisher");
    let consumer = base.join("consumer");

    // 1. project publisher (package.json + mg.toml + code file)
    fs::create_dir_all(&publisher).unwrap();
    fs::create_dir_all(&consumer).unwrap();
    fs::write(
        publisher.join("package.json"),
        r#"{"name":"@e2e-test/demo","version":"1.0.0","type":"module","main":"index.js"}"#,
    )
    .unwrap();
    fs::write(publisher.join("index.js"), "export const demo = 42;\n").unwrap();
    fs::write(
        publisher.join("mg.toml"),
        format!(
            r#"name = "e2e-demo"
ecosystem = "web"

[[registries]]
name = "e2e"
url = "http://127.0.0.1:{port}"
"#
        ),
    )
    .unwrap();
    fs::write(
        consumer.join("package.json"),
        r#"{"name":"consumer","version":"0.0.1","type":"module"}"#,
    )
    .unwrap();

    // 2. server
    eprintln!("[e2e] step 2: spawn server port={port}");
    let mut server = spawn_server(&store, port);
    wait_ready(port);

    let root = workspace_root();
    let mg = root.join("target").join("debug").join("mg");
    assert!(mg.exists(), "mg binary missing — build first");

    let run = |cwd: &Path, args: &[&str], envs: &[(&str, &str)]| {
        let mut cmd = Command::new(&mg);
        cmd.current_dir(cwd)
            .args(args)
            .env("MEGAGATE_TEMPLATE_DIR", root.join("templates"));
        for (k, v) in envs {
            cmd.env(k, v);
        }
        let out = cmd.output().expect("mg run");
        assert!(
            out.status.success(),
            "mg {:?} thất bại:\n{}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    };

    // 3. publish (registry flag override; bỏ git checks — test dir không phải repo)
    eprintln!("[e2e] step 3: publish");
    let out = run(
        &publisher,
        &[
            "publish",
            "--registry",
            &format!("http://127.0.0.1:{port}"),
            "--no-git-checks",
            "--ignore-scripts",
            "--token",
            TEST_ADMIN_TOKEN,
        ],
        &[],
    );
    assert!(
        out.contains("Published") && out.contains("0.1.0"),
        "publish output thiếu xác nhận: {out}"
    );

    // 4. install từ consumer (registry local — env override, không đụng public npm)
    eprintln!("[e2e] step 4: install");
    let out = run(
        &consumer,
        &["add", "@e2e-test/demo@0.1.0"],
        &[
            (
                "MEGAGATE_WEB_REGISTRY_URL",
                &format!("http://127.0.0.1:{port}"),
            ),
            ("MEGAGATE_WEB_REGISTRY_TOKEN", TEST_ADMIN_TOKEN),
            ("MEGAGATE_WEB_ALLOW_INSECURE_LOCALHOST", "1"),
        ],
    );
    assert!(
        out.to_lowercase().contains("installed"),
        "install output lạ: {out}"
    );
    let node_modules = consumer.join("node_modules").join("@e2e-test").join("demo");
    assert!(
        node_modules.join("index.js").exists(),
        "package chưa được cài vào node_modules"
    );

    // 5. cleanup
    eprintln!("[e2e] step 5: cleanup");
    server.kill().ok();
    server.wait().ok();
}
