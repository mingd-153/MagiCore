#[test]
fn test_cloud_framework_wizard_defaults() {
    let config = crate::wizard::cloud::CloudWizard::run();
    assert_eq!(config.core, "clo");
    assert!(
        !config.frameworks.is_empty(),
        "Cloud default framework should not be empty"
    );
}
