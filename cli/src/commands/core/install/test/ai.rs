#[test]
fn lock_file_detects_uv_sync() {
    let dir = std::env::temp_dir().join(format!("mgc-ai-lock-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("uv.lock"), "ok").unwrap();
    let (tool, args) = super::ai_install_command(&dir).unwrap();
    assert_eq!(tool, "uv");
    assert_eq!(args, vec!["sync".to_string()]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn pyproject_bootstrap_uses_uv_sync_without_lock() {
    let dir = std::env::temp_dir().join(format!("mgc-ai-pyproject-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("pyproject.toml"), "[project]\nname = \"demo\"\n").unwrap();
    let (tool, args) = super::ai_install_command(&dir).unwrap();
    assert_eq!(tool, "uv");
    assert_eq!(args, vec!["sync".to_string()]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn requirements_txt_bootstrap_uses_pip_install() {
    let dir = std::env::temp_dir().join(format!("mgc-ai-requirements-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("requirements.txt"), "numpy\n").unwrap();
    let (tool, args) = super::ai_install_command(&dir).unwrap();
    // The bootstrap picks whichever pip alias exists on THIS machine
    // (pip, else pip3) — asserting a fixed name made the test
    // environment-dependent (failed on hosts without a `pip` alias).
    // (Bootstrap chọn alias pip nào tồn tại trên MÁY NÀY (pip, nếu
    // không thì pip3) — assert tên cứng khiến test phụ thuộc môi
    // trường (fail trên máy không có alias `pip`).)
    let pip_alias = if std::process::Command::new("pip")
        .arg("--version")
        .output()
        .is_ok()
    {
        "pip"
    } else {
        "pip3"
    };
    assert_eq!(tool, pip_alias);
    assert_eq!(
        args,
        vec![
            "install".to_string(),
            "-r".to_string(),
            "requirements.txt".to_string()
        ]
    );
    std::fs::remove_dir_all(&dir).ok();
}
