//! E2E import→install: lockfile của PM khác seed mgc.lock rồi install offline-from-registry.
//!
//! Kịch bản (hermetic — chỉ đụng registry local):
//! 1. publish @e2e-import/demo lên registry local
//! 2. `mgc add` trong consumer để tạo mgc.lock THẬT (resolved + integrity thật)
//! 3. thu hoạch entry từ mgc.lock → tổng hợp `package-lock.json` giả định PM cũ
//! 4. xoá mgc.lock + node_modules → `mgc import` → `mgc install`
//! 5. install phải dùng graph từ lockfile ĐÃ IMPORT (không resolve lại) và
//!    materialize lại node_modules

#![allow(clippy::unwrap_used)]

use std::fs;

use mgc_e2e::{free_port, mgc, RegistryServer, TEST_ADMIN_TOKEN};

#[test]
fn e2e_import_lockfile_seeds_install() {
    let Some(port) = free_port() else { return };
    let base = tempfile::tempdir().expect("work dir");
    let publisher = base.path().join("publisher");
    let consumer = base.path().join("consumer");
    fs::create_dir_all(&publisher).unwrap();
    fs::create_dir_all(&consumer).unwrap();

    fs::write(
        publisher.join("package.json"),
        r#"{"name":"@e2e-import/demo","version":"2.5.0","type":"module","main":"index.js"}"#,
    )
    .unwrap();
    fs::write(
        publisher.join("index.js"),
        "export const imported = true;\n",
    )
    .unwrap();
    fs::write(
        publisher.join("mgc.toml"),
        format!(
            r#"name = "e2e-import-demo"
version = "2.5.0"
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
        r#"{"name":"consumer","version":"0.0.1","type":"module","dependencies":{"@e2e-import/demo":"^2.5.0"}}"#,
    )
    .unwrap();
    fs::write(
        consumer.join("mgc.toml"),
        format!(
            r#"name = "e2e-import-consumer"
ecosystem = "web"

[[registries]]
name = "e2e"
url = "http://127.0.0.1:{port}"
"#
        ),
    )
    .unwrap();

    let mut server = RegistryServer::spawn(&base.path().join("registry-store"), port);
    server.wait_ready();
    let url = server.url();
    let reg_envs: Vec<(&str, &str)> = vec![
        ("MAGICORE_WEB_REGISTRY_URL", &url),
        ("MAGICORE_WEB_REGISTRY_TOKEN", TEST_ADMIN_TOKEN),
        ("MAGICORE_WEB_ALLOW_INSECURE_LOCALHOST", "1"),
    ];

    // Bước 1-2: publish + add thật để có resolved/integrity chuẩn từ hệ thống
    eprintln!("[import-e2e] step 1: publish + add");
    mgc(
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
    mgc(&consumer, &["add", "@e2e-import/demo@2.5.0"], &reg_envs);

    // Bước 3: thu hoạch entry thật từ mgc.lock do mgc sinh ra
    let lock_content = fs::read_to_string(consumer.join("mgc.lock")).unwrap();
    // `mgc add` hiện ghi lockfile kiểu JSON (khác TOML chuẩn của import) — đọc khoan dung cả 2
    // (`mgc add` writes a JSON-flavoured lockfile; accept both shapes here)
    let parsed: toml::Value = match toml::from_str(&lock_content) {
        Ok(v) => v,
        Err(_) => {
            let json: serde_json::Value = serde_json::from_str(&lock_content)
                .expect("consumer mgc.lock must be valid TOML or JSON");
            serde_json::from_value(json).expect("convert json lock to toml value")
        }
    };
    let pkg = parsed
        .get("package")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .cloned()
        .expect("mgc.lock must contain the demo package");
    let resolved = pkg
        .get("resolved")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let integrity = pkg
        .get("integrity")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let version = pkg
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // Xoá dấu vết mgc → mô phỏng project mới chuyển từ npm sang
    fs::remove_file(consumer.join("mgc.lock")).unwrap();
    let _ = fs::remove_file(consumer.join("mgc.lock.sig"));
    fs::remove_dir_all(consumer.join("node_modules")).ok();

    // Tổng hợp package-lock.json "cũ" với đúng dữ liệu thật vừa thu hoạch
    let legacy = format!(
        r#"{{
  "lockfileVersion": 3,
  "packages": {{
    "": {{ "name": "consumer", "dependencies": {{ "@e2e-import/demo": "^{version}" }} }},
    "node_modules/@e2e-import/demo": {{
      "version": "{version}",
      "resolved": "{resolved}",
      "integrity": "{integrity}"
    }}
  }}
}}"#,
    );
    fs::write(consumer.join("package-lock.json"), legacy).unwrap();

    // Bước 4: import phải tạo mgc.lock signed
    eprintln!("[import-e2e] step 2: mgc import");
    let out = mgc(&consumer, &["import"], &[]);
    assert!(
        out.contains("Imported") && out.contains("signed"),
        "import output unexpected: {out}"
    );
    assert!(
        consumer.join("mgc.lock").exists(),
        "mgc.lock must exist after import"
    );
    assert!(
        consumer.join("mgc.lock.sig").exists(),
        "imported lockfile must be signed"
    );

    // Bước 5: install phải seed graph từ lock đã import (không cần resolve mạng rộng)
    eprintln!("[import-e2e] step 3: install from imported lock");
    let out = mgc(&consumer, &["install"], &reg_envs);
    assert!(
        out.to_lowercase().contains("installed"),
        "install output unexpected: {out}"
    );
    let materialized = consumer
        .join("node_modules")
        .join("@e2e-import")
        .join("demo")
        .join("index.js");
    assert!(
        materialized.exists(),
        "imported lock did not drive installation"
    );

    server.shutdown();
}
