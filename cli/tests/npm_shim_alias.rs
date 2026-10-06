//! Verify the inert npm version-probe alias carried by the mgc executable.
//! Kiểm tra alias probe phiên bản npm vô hại được mang bởi executable mgc.

use std::process::{Command, Output};

fn invoke_npm_alias(args: &[&str]) -> Output {
    let binary = std::env::var_os("CARGO_BIN_EXE_mgc")
        .map(std::path::PathBuf::from)
        .expect("Cargo must provide the mgc executable");
    let root = tempfile::tempdir().expect("create npm alias fixture");
    #[cfg(windows)]
    let alias = root.path().join("npm.exe");
    #[cfg(not(windows))]
    let alias = root.path().join("npm");
    std::fs::copy(binary, &alias).expect("copy mgc as npm alias");

    Command::new(alias)
        .args(args)
        .output()
        .expect("run npm alias")
}

#[test]
fn npm_alias_allows_only_the_exact_version_probe() {
    let version = invoke_npm_alias(&["--version"]);
    assert!(version.status.success());
    assert_eq!(String::from_utf8_lossy(&version.stdout).trim(), "0.0.0");

    for args in [&["install"][..], &["--version", "install"][..]] {
        let blocked = invoke_npm_alias(args);
        assert_eq!(blocked.status.code(), Some(126));
        assert!(
            String::from_utf8_lossy(&blocked.stderr)
                .contains("MagiCore blocked forbidden package manager: npm")
        );
    }
}
