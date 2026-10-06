//! PyPI identity migration regressions.
//! Kiểm thử hồi quy migration khóa định danh PyPI.

use super::*;

#[tokio::test]
async fn legacy_pypi_aliases_migrate_to_one_canonical_record() {
    let temp = tempfile::tempdir().expect("temporary registry directory");
    let store = RegistryStore::new(temp.path())
        .await
        .expect("registry store");
    sqlx::query("DELETE FROM mgc_runtime_migrations WHERE name = ?")
        .bind("canonical-pypi-project-names-v1")
        .execute(&store.db)
        .await
        .expect("reset one-time migration marker");

    for (name, version, filename, digest) in [
        (
            "Flask_Test.pkg",
            "1.0.0",
            "flask_test_pkg-1.0.0.whl",
            "sha256:old",
        ),
        (
            "flask-test-pkg",
            "2.0.0",
            "flask_test_pkg-2.0.0.whl",
            "sha256:new",
        ),
    ] {
        sqlx::query(
            "INSERT INTO pypi_files (name, version, filename, digest, size) VALUES (?, ?, ?, ?, 1)",
        )
        .bind(name)
        .bind(version)
        .bind(filename)
        .bind(digest)
        .execute(&store.db)
        .await
        .expect("insert legacy PyPI record");
    }

    store
        .migrate_pypi_project_names()
        .await
        .expect("canonicalize existing PyPI records");
    let files = store
        .get_pypi_files("FLASK---TEST___PKG")
        .await
        .expect("read canonical PyPI records");

    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|file| file.name == "flask-test-pkg"));
}

#[tokio::test]
async fn conflicting_pypi_alias_migration_rolls_back_without_deleting_records() {
    let temp = tempfile::tempdir().expect("temporary registry directory");
    let store = RegistryStore::new(temp.path())
        .await
        .expect("registry store");
    sqlx::query("DELETE FROM mgc_runtime_migrations WHERE name = ?")
        .bind("canonical-pypi-project-names-v1")
        .execute(&store.db)
        .await
        .expect("reset one-time migration marker");

    for (name, digest) in [
        ("Flask_Test.pkg", "sha256:old"),
        ("flask-test-pkg", "sha256:new"),
    ] {
        sqlx::query(
            "INSERT INTO pypi_files (name, version, filename, digest, size) VALUES (?, '1.0.0', 'same.whl', ?, 1)",
        )
        .bind(name)
        .bind(digest)
        .execute(&store.db)
        .await
        .expect("insert conflicting legacy PyPI record");
    }

    assert!(store.migrate_pypi_project_names().await.is_err());
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pypi_files")
        .fetch_one(&store.db)
        .await
        .expect("count preserved legacy records");
    assert_eq!(count, 2);
}
