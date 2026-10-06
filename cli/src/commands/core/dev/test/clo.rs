use super::*;

#[test]
fn cloud_dev_and_deploy_fail_closed_without_spawning_provider_clis() {
    for kind in ["terraform", "cdk", "pulumi"] {
        assert!(
            native_dev_command(kind).is_err(),
            "{kind} dev must fail closed"
        );
        assert!(
            native_deploy_command(kind).is_err(),
            "{kind} deploy must fail closed"
        );
    }
}
