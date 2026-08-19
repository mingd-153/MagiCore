#[test]
fn test_iot_framework_wizard_defaults() {
    let config = crate::wizard::iot::IotWizard::run();
    assert_eq!(config.core, "iot");
    assert!(!config.frameworks.is_empty(), "IoT default framework should not be empty");
}
