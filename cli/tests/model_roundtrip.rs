#![allow(clippy::unwrap_used)]

//! Integration test: mgc model push/pull roundtrip qua registry server thật
//! (Test roundtrip: push file → pull về → content khớp; fail-closed không token)

use std::process::{Command, Stdio};

const PORT_CANDIDATES: &[u16] = &[
    4315, 4351, 4135, 4153, 4513, 4531, 3415, 3451, 3145, 3154, 3541, 3514, 1345, 1354, 1435, 1453,
    1534, 1543, 5134, 5143, 5314, 5341, 5413, 5431,
]; // RULE §13: port contains 4·3·1·5
const ADMIN: &str = "adm1-test";

fn mgc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_mg").expect("CARGO_BIN_EXE_mg")
}

// ponytail: server con phải tách stdio — nếu không cargo test chờ pipe đóng vô hạn
struct ServerGuard(std::process::Child);
impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn pick_free_rule_port() -> Option<u16> {
    let mut permission_denied = false;
    for port in PORT_CANDIDATES {
        match std::net::TcpListener::bind(("127.0.0.1", *port)) {
            Ok(_) => return Some(*port),
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                permission_denied = true;
            }
            Err(_) => {}
        }
    }
    if permission_denied {
        eprintln!("skipped: sandbox denies localhost bind for registry roundtrip test");
        None
    } else {
        panic!("no free RULE-compatible registry test port");
    }
}

#[tokio::test]
async fn model_push_pull_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let out = tmp.path().join("out");
    std::fs::create_dir_all(&data).unwrap();

    let weights = b"fake-weights-1234567890";
    let config = b"{\"layers\": 1}";
    std::fs::write(data.join("weights.bin"), weights).unwrap();
    std::fs::write(data.join("config.json"), config).unwrap();

    let store = tmp.path().join("store");
    let Some(port) = pick_free_rule_port() else {
        return;
    };
    let mut server = ServerGuard(
        Command::new(mgc_bin())
            .args([
                "registry",
                "serve",
                "--port",
                &port.to_string(),
                "--host",
                "127.0.0.1",
                "--store-dir",
                store.to_str().unwrap(),
                "--admin-token",
                ADMIN,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn registry server"),
    );

    // Đợi server lên (401/405 fail-closed cũng = server sống)
    let url = format!("http://localhost:{port}");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(status) = server.0.try_wait().unwrap() {
            let stderr = server
                .0
                .stderr
                .take()
                .map(|mut stderr| {
                    let mut buf = String::new();
                    let _ = std::io::Read::read_to_string(&mut stderr, &mut buf);
                    buf
                })
                .unwrap_or_default();
            panic!("registry server exited early with {status}: {stderr}");
        }
        if let Ok(r) = reqwest::Client::new()
            .get(format!("{url}/v2/"))
            .send()
            .await
        {
            if r.status().is_success() || r.status().as_u16() == 401 || r.status().as_u16() == 405 {
                break;
            }
        }
        if std::time::Instant::now() > deadline {
            panic!("registry server did not become ready at {url}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let run = |args: &[&str]| {
        Command::new(mgc_bin())
            .args(args)
            .output()
            .expect("run mgc")
    };

    // Push không token → fail-closed 401
    let no_token = run(&[
        "model",
        "push",
        data.join("weights.bin").to_str().unwrap(),
        "--repo",
        "ai/t1",
        "--tag",
        "v1",
        "--registry",
        &url,
    ]);
    assert!(
        String::from_utf8_lossy(&no_token.stderr).contains("401"),
        "push không token phải 401: {}",
        String::from_utf8_lossy(&no_token.stderr)
    );

    // Push đầy đủ
    let push = run(&[
        "model",
        "push",
        data.join("weights.bin").to_str().unwrap(),
        data.join("config.json").to_str().unwrap(),
        "--repo",
        "ai/t1",
        "--tag",
        "v1",
        "--registry",
        &url,
        "--token",
        ADMIN,
    ]);
    assert!(
        push.status.success(),
        "push thất bại: {}",
        String::from_utf8_lossy(&push.stderr)
    );

    // Pull + verify nội dung
    let pull = run(&[
        "model",
        "pull",
        "ai/t1",
        "--tag",
        "v1",
        "--registry",
        &url,
        "--token",
        ADMIN,
        "--output",
        out.to_str().unwrap(),
    ]);
    assert!(
        pull.status.success(),
        "pull thất bại: {}",
        String::from_utf8_lossy(&pull.stderr)
    );
    assert_eq!(
        std::fs::read(out.join("weights.bin")).unwrap(),
        weights,
        "weights.bin khác sau roundtrip"
    );
    assert_eq!(
        std::fs::read(out.join("config.json")).unwrap(),
        config,
        "config.json khác sau roundtrip"
    );
}
