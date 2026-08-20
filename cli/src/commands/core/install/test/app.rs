use super::*;

#[test]
fn install_command_per_language() {
    let fl = install_command(mg_app_adapter::AppLanguage::Flutter);
    assert_eq!(fl.tool, "flutter");
    assert_eq!(fl.args, vec!["pub", "get"]);
    let kt = install_command(mg_app_adapter::AppLanguage::Kotlin);
    assert_eq!(kt.tool, "gradle");
    assert_eq!(kt.args, vec!["dependencies"]);
    let sw = install_command(mg_app_adapter::AppLanguage::Swift);
    assert_eq!(sw.tool, "swift");
    assert_eq!(sw.args, vec!["package", "resolve"]);
    let rn = install_command(mg_app_adapter::AppLanguage::ReactNative);
    assert_eq!(rn.tool, "npm");
    assert_eq!(rn.args, vec!["install"]);
}

#[test]
fn dev_command_per_language() {
    assert_eq!(
        dev_command(mg_app_adapter::AppLanguage::Flutter).args,
        vec!["run"]
    );
    assert_eq!(
        dev_command(mg_app_adapter::AppLanguage::Kotlin).tool,
        "gradle"
    );
    assert_eq!(
        dev_command(mg_app_adapter::AppLanguage::Swift).args,
        vec!["run"]
    );
    assert_eq!(
        dev_command(mg_app_adapter::AppLanguage::ReactNative).args,
        vec!["run", "android"]
    );
}

#[test]
fn xcode_project_prefers_workspace() {
    let dir = std::env::temp_dir().join(format!("mg-app-xc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("App.xcodeproj"), "").unwrap();
    std::fs::write(dir.join("App.xcworkspace"), "").unwrap();
    assert_eq!(find_xcode_project(&dir).unwrap(), "App.xcworkspace");
    std::fs::remove_file(dir.join("App.xcworkspace")).unwrap();
    assert_eq!(find_xcode_project(&dir).unwrap(), "App.xcodeproj");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
pub fn dev_scheme_reads_mg_toml() {
    let dir = std::env::temp_dir().join(format!("mg-app-scheme-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert!(dev_scheme(&dir).is_none());
    std::fs::write(
        dir.join("mg.toml"),
        "[app]\nlanguage = \"objc\"\ndev_scheme = \"App\"\n",
    )
    .unwrap();
    assert_eq!(dev_scheme(&dir).unwrap(), "App");
    std::fs::write(dir.join("mg.toml"), "[app]\nlanguage = \"objc\"\n").unwrap();
    assert!(dev_scheme(&dir).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
