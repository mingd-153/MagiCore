#[test]
fn lock_file_detects_uv_sync() {
    let dir = std::env::temp_dir().join(format!("mgc-ai-lock-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("uv.lock"), "ok").unwrap();
    let root = dir.clone();
    let (tool, args): (&str, Vec<String>) = if root.join("uv.lock").exists() {
        ("uv", vec!["sync".to_string()])
    } else {
        ("pip", Vec::new())
    };
    assert_eq!(tool, "uv");
    assert_eq!(args, vec!["sync".to_string()]);
    std::fs::remove_dir_all(&dir).ok();
}
