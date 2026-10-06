use super::*;

#[test]
fn sigstore_predicate_marks_publish_attempt_without_claiming_registry_commit() {
    let builder = BuilderIdentity {
        issuer: "https://token.actions.githubusercontent.com".into(),
        subject: "repo:acme/widgets:ref:refs/tags/v1.0.0".into(),
        repository: "acme/widgets".into(),
        job_workflow_ref: None,
        workflow_ref: None,
        event_name: None,
    };
    let predicate =
        trusted_publish_predicate("@acme/widgets", "1.0.0", "sha256:artifact-digest", &builder);

    assert_eq!(predicate["registryOutcome"], "submission_attempted");
}
