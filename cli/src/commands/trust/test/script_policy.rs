//! Trust policy mutation lock regressions — hồi quy khóa khi sửa trust policy.

use std::time::Duration;

fn use_short_lock_timeout(root: &std::path::Path) {
    std::fs::write(
        root.join("mgc.toml"),
        "name = \"test-project\"\nversion = \"0.1.0\"\necosystem = \"web\"\n[lock]\nacquire_timeout_ms = 1\n",
    )
    .expect("short lock timeout must be configured for the test");
}

#[test]
fn policy_write_refuses_to_race_an_active_dependency_mutation() {
    let temp = tempfile::tempdir().expect("temporary project must be created");
    use_short_lock_timeout(temp.path());
    let _held =
        mgc_lockfile::project_lock::ProjectWriteLock::acquire(temp.path(), Duration::from_secs(1))
            .expect("dependency mutation lock must be acquired");

    let result = super::set_policy(temp.path(), "sample-package", super::ScriptPolicy::Approved);

    assert!(
        result.is_err(),
        "trust approval must wait for an active install instead of racing its script gate"
    );
}

#[test]
fn policy_write_applies_after_the_dependency_mutation_releases_its_lock() {
    let temp = tempfile::tempdir().expect("temporary project must be created");
    super::set_policy(
        temp.path(),
        "sample-package@1.2.3",
        super::ScriptPolicy::Denied,
    )
    .expect("policy write must succeed when the project is idle");

    let db_path = temp.path().join(".magicore/cache/web/store.db");
    let db = mgc_store::Database::open(&db_path).expect("trust database must open");
    assert_eq!(
        db.get_trust_policy("sample-package@1.2.3")
            .expect("saved policy must be readable")
            .as_deref(),
        Some("denied")
    );
}

#[test]
fn prune_refuses_to_race_an_active_dependency_mutation() {
    let temp = tempfile::tempdir().expect("temporary project must be created");
    use_short_lock_timeout(temp.path());
    let _held =
        mgc_lockfile::project_lock::ProjectWriteLock::acquire(temp.path(), Duration::from_secs(1))
            .expect("dependency mutation lock must be acquired");

    let result = super::prune_stale_policies(temp.path());

    assert!(
        result.is_err(),
        "trust pruning must not race the installed package database"
    );
}

#[test]
fn prune_updates_the_trust_database_when_the_project_is_idle() {
    let temp = tempfile::tempdir().expect("temporary project must be created");
    super::set_policy(temp.path(), "stale-package", super::ScriptPolicy::Denied)
        .expect("initial policy write must succeed");

    let pruned = super::prune_stale_policies(temp.path())
        .expect("stale policies must be pruned when the project is idle");

    assert_eq!(pruned, 1);
}

#[test]
#[cfg(unix)]
fn policy_write_refuses_a_symlinked_cache_directory() {
    let temp = tempfile::tempdir().expect("temporary project must be created");
    let outside = temp.path().join("outside");
    std::fs::create_dir(&outside).expect("outside directory must be created");
    let cache = temp.path().join(".magicore/cache");
    std::fs::create_dir_all(cache.parent().expect("cache parent must exist"))
        .expect("magicore directory must be created");
    std::os::unix::fs::symlink(&outside, &cache).expect("cache symlink must be created");

    let result = super::set_policy(temp.path(), "sample-package", super::ScriptPolicy::Denied);

    assert!(result.is_err(), "trust DB must not follow a cache symlink");
    assert!(
        std::fs::read_dir(&outside)
            .expect("outside directory must remain readable")
            .next()
            .is_none(),
        "no trust database may be created through the symlink"
    );
}
