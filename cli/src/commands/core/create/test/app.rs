#[test]
fn test_app_framework_wizard_defaults() {
    let config = crate::wizard::app::AppWizard::run();
    assert_eq!(config.core, "app");
    assert!(
        !config.frameworks.is_empty(),
        "App default framework should not be empty"
    );
}
