//! E2E: pack → serve (mgc-registry) → publish → install.
//! Ported từ cli/tests/e2e_pipeline.rs sang crate tests/e2e chuẩn canonical.
// (E2E pipeline driving the real binaries end-to-end against a local registry.)

#![allow(clippy::unwrap_used)]

use std::fs;

use mgc_e2e::{free_port, mgc, RegistryServer, TEST_ADMIN_TOKEN};

#[test]
fn e2e_publish_then_install() {
    let Some(port) = free_port() else { return };
    let base = tempfile::tempdir().expect("work dir");
    let publisher = base.path().join("publisher");
    let consumer = base.path().join("consumer");
    fs::create_dir_all(&publisher).unwrap();
    fs::create_dir_all(&consumer).unwrap();

    // 1. publisher project (package.json + index.js)
    fs::write(
        publisher.join("package.json"),
        r#"{"name":"@e2e-test/demo","version":"1.0.0","type":"module","main":"index.js"}"#,
    )
    .unwrap();
    fs::write(publisher.join("index.js"), "export const demo = 42;\n").unwrap();
    fs::write(
        publisher.join("mgc.toml"),
        format!(
            r#"name = "e2e-demo"
version = "1.0.0"
ecosystem = "web"

[[registries]]
name = "e2e"
url = "http://127.0.0.1:{port}"
"#
        ),
    )
    .unwrap();

    // consumer project trống, trỏ registry local
    fs::write(
        consumer.join("package.json"),
        r#"{"name":"consumer","version":"0.0.1","type":"module"}"#,
    )
    .unwrap();
    fs::write(
        consumer.join("mgc.toml"),
        format!(
            r#"name = "e2e-consumer"
ecosystem = "web"

[[registries]]
name = "e2e"
url = "http://127.0.0.1:{port}"
"#
        ),
    )
    .unwrap();

    // 2. server
    eprintln!("[e2e] step 2: spawn server port={port}");
    let mut server = RegistryServer::spawn(&base.path().join("registry-store"), port);
    server.wait_ready();
    let url = server.url();

    // 3. publish
    eprintln!("[e2e] step 3: publish");
    let out = mgc(
        &publisher,
        &[
            "publish",
            "--registry",
            &url,
            "--no-git-checks",
            "--ignore-scripts",
            "--token",
            TEST_ADMIN_TOKEN,
        ],
        &[],
    );
    assert!(
        out.contains("Published") && out.contains("1.0.0") || out.contains("0.1.0"),
        "publish output missing confirmation: {out}"
    );

    // 4. install từ consumer (registry local qua env override — không đụng mạng thật)
    eprintln!("[e2e] step 4: install");
    let out = mgc(
        &consumer,
        &["add", "@e2e-test/demo@1.0.0"],
        &[
            ("MAGICORE_WEB_REGISTRY_URL", &url),
            ("MAGICORE_WEB_REGISTRY_TOKEN", TEST_ADMIN_TOKEN),
            ("MAGICORE_WEB_ALLOW_INSECURE_LOCALHOST", "1"),
        ],
    );
    assert!(
        out.to_lowercase().contains("installed"),
        "unexpected install output: {out}"
    );
    let installed = consumer
        .join("node_modules")
        .join("@e2e-test")
        .join("demo")
        .join("index.js");
    assert!(
        installed.exists(),
        "package not materialized in node_modules"
    );

    eprintln!("[e2e] step 5: cleanup");
    server.shutdown();
}
