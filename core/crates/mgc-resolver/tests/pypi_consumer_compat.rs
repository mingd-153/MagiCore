//! Consumer-Python compatibility resolution tests.
//! Test resolve theo phiên bản Python đang dùng.

#![allow(clippy::unwrap_used)]
#![allow(unsafe_code)]

use mgc_resolver::protocols::{PypiProtocol, RegistryProtocol};

struct PythonVersionGuard(Option<std::ffi::OsString>);

impl PythonVersionGuard {
    fn set(version: &str) -> Self {
        let previous = std::env::var_os("MGC_PYTHON_VERSION");
        // This integration-test binary contains one test; restoring in Drop
        // keeps the process environment isolated even if assertions panic.
        // Binary test tích hợp này chỉ có một test; Drop phục hồi env kể cả panic.
        unsafe { std::env::set_var("MGC_PYTHON_VERSION", version) };
        Self(previous)
    }
}

impl Drop for PythonVersionGuard {
    fn drop(&mut self) {
        match &self.0 {
            Some(previous) => unsafe {
                std::env::set_var("MGC_PYTHON_VERSION", previous);
            },
            None => unsafe {
                std::env::remove_var("MGC_PYTHON_VERSION");
            },
        }
    }
}

fn file_entry(filename: &str, url: &str) -> String {
    format!(
        r#"{{"filename":"{filename}","url":"{url}","digests":{{"sha256":"{}"}},"packagetype":"bdist_wheel","requires_python":">=3.9"}}"#,
        "11".repeat(32)
    )
}

static PYTHON_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn pypi_candidate_metadata_failure_does_not_fall_back_to_older_release() {
    let _environment = PYTHON_ENV_LOCK.lock().await;
    let _python_version = PythonVersionGuard::set("3.11");
    let mut server = mockito::Server::new_async().await;
    let base = server.url();
    let releases = format!(
        r#""2.0.0": [{latest}], "1.0.0": [{older}]"#,
        latest = file_entry("demo-2.0.0-py3-none-any.whl", &format!("{base}/latest.whl")),
        older = file_entry("demo-1.0.0-py3-none-any.whl", &format!("{base}/older.whl")),
    );
    server
        .mock("GET", "/pypi/demo/json")
        .with_status(200)
        .with_body(format!(
            r#"{{"info":{{"requires_python":">=3.9"}},"releases":{{{releases}}}}}"#
        ))
        .create_async()
        .await;
    server
        .mock("GET", "/pypi/demo/2.0.0/json")
        .with_status(503)
        .with_body("temporary registry failure")
        .expect(1)
        .create_async()
        .await;
    let older_mock = server
        .mock("GET", "/pypi/demo/1.0.0/json")
        .with_status(200)
        .with_body(r#"{"info":{"requires_python":">=3.9","requires_dist":[]},"releases":{}}"#)
        .expect(0)
        .create_async()
        .await;

    let error = PypiProtocol::new(&base)
        .resolve("demo", ">=1.0")
        .await
        .expect_err("an unreadable newer candidate must not be silently skipped");
    assert!(
        error
            .to_string()
            .contains("cannot determine Python compatibility"),
        "{error}"
    );
    older_mock.assert_async().await;
}

#[tokio::test]
async fn pypi_skips_wheel_with_incompatible_file_requires_python() {
    let _environment = PYTHON_ENV_LOCK.lock().await;
    let _python_version = PythonVersionGuard::set("3.11");
    let mut server = mockito::Server::new_async().await;
    let base = server.url();
    let newer_url = format!("{base}/newer.whl");
    let older_url = format!("{base}/older.whl");
    let releases = format!(
        r#""2.0.0": [{{"filename":"demo-2.0.0-py3-none-any.whl","url":"{newer_url}","digests":{{"sha256":"{newer_digest}"}},"packagetype":"bdist_wheel","requires_python":">=3.12"}}], "1.0.0": [{{"filename":"demo-1.0.0-py3-none-any.whl","url":"{older_url}","digests":{{"sha256":"{older_digest}"}},"packagetype":"bdist_wheel","requires_python":">=3.9"}}]"#,
        newer_digest = "22".repeat(32),
        older_digest = "11".repeat(32),
    );
    server
        .mock("GET", "/pypi/demo/json")
        .with_status(200)
        .with_body(format!(
            r#"{{"info":{{"requires_python":">=3.9"}},"releases":{{{releases}}}}}"#
        ))
        .create_async()
        .await;
    server
        .mock("GET", "/pypi/demo/2.0.0/json")
        .with_status(200)
        .with_body(r#"{"info":{"requires_python":">=3.9","requires_dist":[]},"releases":{}}"#)
        .expect(1)
        .create_async()
        .await;
    server
        .mock("GET", "/pypi/demo/1.0.0/json")
        .with_status(200)
        .with_body(r#"{"info":{"requires_python":">=3.9","requires_dist":[]},"releases":{}}"#)
        .expect(1)
        .create_async()
        .await;

    let resolved = PypiProtocol::new(&base)
        .resolve("demo", ">=1.0")
        .await
        .expect("resolver should continue to the newest release with a compatible artifact");
    assert_eq!(resolved.version, "1.0.0");
    assert!(resolved.artifact_url.ends_with("/older.whl"));
}
