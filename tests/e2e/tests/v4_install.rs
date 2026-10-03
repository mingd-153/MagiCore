//! E2E: v4 lock round-trip — publish → install (v3) → migrate to v4 →
//! wipe node_modules → install replays the v4 graph byte-identically.
//! (E2E vòng v4: publish → install (v3) → migrate sang v4 → xóa
//! node_modules → install replay graph v4 y hệt byte.)

#![allow(clippy::unwrap_used)]

use std::fs;

use mgc_e2e::{RegistryServer, TEST_ADMIN_TOKEN, free_port, mgc};

const WEB_ENV_KEYS: [&str; 3] = [
    "MAGICORE_WEB_REGISTRY_URL",
    "MAGICORE_WEB_REGISTRY_TOKEN",
    "MAGICORE_WEB_ALLOW_INSECURE_LOCALHOST",
];

fn web_env(url: &str) -> Vec<(&str, &str)> {
    vec![
        (WEB_ENV_KEYS[0], url),
        (WEB_ENV_KEYS[1], TEST_ADMIN_TOKEN),
        (WEB_ENV_KEYS[2], "1"),
    ]
}

#[test]
fn e2e_v4_lock_replays_install_byte_identical() {
    let Some(port) = free_port() else { return };
    let base = tempfile::tempdir().expect("work dir");
    let publisher = base.path().join("publisher");
    let consumer = base.path().join("consumer");
    fs::create_dir_all(&publisher).unwrap();
    fs::create_dir_all(&consumer).unwrap();

    fs::write(
        publisher.join("package.json"),
        r#"{"name":"@e2e-test/v4demo","version":"1.0.0","type":"module","main":"index.js"}"#,
    )
    .unwrap();
    fs::write(publisher.join("index.js"), "export const v4demo = 7;\n").unwrap();
    fs::write(
        publisher.join("mgc.toml"),
        "name = \"e2e-v4demo\"\nversion = \"1.0.0\"\necosystem = \"web\"\n",
    )
    .unwrap();

    fs::write(
        consumer.join("package.json"),
        r#"{"name":"consumer","version":"0.0.1","type":"module"}"#,
    )
    .unwrap();
    fs::write(
        consumer.join("mgc.toml"),
        "name = \"e2e-v4consumer\"\necosystem = \"web\"\n",
    )
    .unwrap();

    eprintln!("[e2e-v4] step 1: spawn server");
    let mut server = RegistryServer::spawn(&base.path().join("registry-store"), port);
    server.wait_ready();
    let url = server.url();

    eprintln!("[e2e-v4] step 2: publish");
    let publish_env: Vec<(&str, &str)> = vec![];
    let _ = mgc(
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
        &publish_env,
    );

    eprintln!("[e2e-v4] step 3: install (v3 lock)");
    let env = web_env(&url);
    mgc(&consumer, &["add", "@e2e-test/v4demo@1.0.0"], &env);
    let installed = consumer
        .join("node_modules")
        .join("@e2e-test")
        .join("v4demo")
        .join("index.js");
    assert!(installed.exists(), "package not materialized");
    let before = fs::read(&installed).unwrap();
    let lock_v3 = fs::read_to_string(consumer.join("mgc.lock")).unwrap();
    assert!(
        lock_v3.contains("version = \"3\""),
        "precondition: add must write a v3 lock"
    );

    eprintln!("[e2e-v4] step 4: migrate to v4");
    mgc(&consumer, &["migrate", "lock", "--to", "v4"], &env);
    let lock_v4 = fs::read_to_string(consumer.join("mgc.lock")).unwrap();
    assert!(
        lock_v4.contains("version = \"4\""),
        "migrate must emit a v4 lock"
    );

    eprintln!("[e2e-v4] step 5: wipe + reinstall from v4");
    fs::remove_dir_all(consumer.join("node_modules")).unwrap();
    let out = mgc(&consumer, &["install"], &env);
    assert!(
        out.contains("Using mgc.lock"),
        "install must replay the lock (v4 on disk): {out}"
    );
    let after = fs::read(&installed).unwrap();
    assert_eq!(before, after, "v4 replay must materialize identical bytes");

    eprintln!("[e2e-v4] step 6: cleanup");
    server.shutdown();
}
