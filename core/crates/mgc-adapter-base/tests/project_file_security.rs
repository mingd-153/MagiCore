#![allow(clippy::unwrap_used)]

use mgc_adapter_base::project_file::{
    atomic_write_project_regular, atomic_write_regular, read_project_regular_text,
    read_regular_text,
};

#[cfg(unix)]
#[test]
fn read_regular_text_rejects_manifest_symlink() {
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let external_manifest = outside.path().join("manifest.toml");
    std::fs::write(&external_manifest, "secret = true\n").unwrap();
    symlink(&external_manifest, project.path().join("pyproject.toml")).unwrap();

    let result = read_regular_text(&project.path().join("pyproject.toml"), "pyproject.toml");

    assert!(
        result.is_err(),
        "must not read a project manifest through a symlink"
    );
    assert_eq!(
        std::fs::read_to_string(external_manifest).unwrap(),
        "secret = true\n"
    );
}

#[cfg(unix)]
#[test]
fn read_regular_text_rejects_fifo_without_blocking() {
    use std::sync::mpsc;
    use std::time::Duration;

    let project = tempfile::tempdir().unwrap();
    let fifo = project.path().join("pom.xml");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("POSIX test runner must provide mkfifo");
    assert!(status.success(), "create FIFO fixture");

    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(read_regular_text(&fifo, "pom.xml"));
    });

    let result = receiver
        .recv_timeout(Duration::from_millis(250))
        .expect("reading a non-regular manifest must not block on a FIFO");
    assert!(result.is_err(), "FIFO is not a regular project manifest");
}

#[cfg(unix)]
#[test]
fn atomic_write_regular_rejects_manifest_symlink_without_touching_target() {
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let external_manifest = outside.path().join("manifest.toml");
    std::fs::write(&external_manifest, "secret = true\n").unwrap();
    let project_manifest = project.path().join("pyproject.toml");
    symlink(&external_manifest, &project_manifest).unwrap();

    let result = atomic_write_regular(&project_manifest, b"secret = false\n");

    assert!(
        result.is_err(),
        "must not replace or follow a symlinked manifest"
    );
    assert_eq!(
        std::fs::read_to_string(external_manifest).unwrap(),
        "secret = true\n"
    );
    assert!(
        std::fs::symlink_metadata(project_manifest)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn read_regular_text_rejects_parent_directory_symlink() {
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let outside_dir = outside.path().join("gradle");
    std::fs::create_dir(&outside_dir).unwrap();
    let external_manifest = outside_dir.join("libs.versions.toml");
    std::fs::write(&external_manifest, "[versions]\nsecret = \"1\"\n").unwrap();
    symlink(&outside_dir, project.path().join("gradle")).unwrap();

    let result = read_project_regular_text(
        project.path(),
        std::path::Path::new("gradle/libs.versions.toml"),
        "libs.versions.toml",
    );

    assert!(
        result.is_err(),
        "must refuse traversal through a symlinked parent"
    );
    assert_eq!(
        std::fs::read_to_string(external_manifest).unwrap(),
        "[versions]\nsecret = \"1\"\n"
    );
}

#[cfg(unix)]
#[test]
fn atomic_write_regular_rejects_parent_directory_symlink() {
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let outside_dir = outside.path().join("gradle");
    std::fs::create_dir(&outside_dir).unwrap();
    let external_manifest = outside_dir.join("libs.versions.toml");
    std::fs::write(&external_manifest, "[versions]\nsecret = \"1\"\n").unwrap();
    symlink(&outside_dir, project.path().join("gradle")).unwrap();

    let result = atomic_write_project_regular(
        project.path(),
        std::path::Path::new("gradle/libs.versions.toml"),
        b"[versions]\nsecret = \"2\"\n",
    );

    assert!(
        result.is_err(),
        "must not replace a file through a symlinked parent"
    );
    assert_eq!(
        std::fs::read_to_string(external_manifest).unwrap(),
        "[versions]\nsecret = \"1\"\n"
    );
}

#[test]
fn project_file_helpers_accept_regular_nested_manifest() {
    let project = tempfile::tempdir().unwrap();
    let gradle = project.path().join("gradle");
    std::fs::create_dir(&gradle).unwrap();
    let path = std::path::Path::new("gradle/libs.versions.toml");
    std::fs::write(
        gradle.join("libs.versions.toml"),
        "[versions]\napp = \"1\"\n",
    )
    .unwrap();

    let before = read_project_regular_text(project.path(), path, "catalog").unwrap();
    atomic_write_project_regular(project.path(), path, b"[versions]\napp = \"2\"\n").unwrap();
    let after = read_project_regular_text(project.path(), path, "catalog").unwrap();

    assert!(before.contains("app = \"1\""));
    assert!(after.contains("app = \"2\""));
}

#[test]
fn project_file_helpers_reject_parent_traversal() {
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let external = outside.path().join("manifest.toml");
    std::fs::write(&external, "owned = true\n").unwrap();

    let read = read_project_regular_text(
        project.path(),
        std::path::Path::new("../outside/manifest.toml"),
        "manifest",
    );
    let write = atomic_write_project_regular(
        project.path(),
        std::path::Path::new("../outside/manifest.toml"),
        b"owned = false\n",
    );

    assert!(read.is_err());
    assert!(write.is_err());
    assert_eq!(std::fs::read_to_string(external).unwrap(), "owned = true\n");
}

#[cfg(unix)]
#[test]
fn project_file_helpers_allow_a_symlink_selected_project_root() {
    use std::os::unix::fs::symlink;

    let parent = tempfile::tempdir().unwrap();
    let project = parent.path().join("real-project");
    std::fs::create_dir(&project).unwrap();
    let gradle = project.join("gradle");
    std::fs::create_dir(&gradle).unwrap();
    std::fs::write(
        gradle.join("libs.versions.toml"),
        "[versions]\napp = \"1\"\n",
    )
    .unwrap();
    let selected_root = parent.path().join("project-link");
    symlink(&project, &selected_root).unwrap();

    let content = read_project_regular_text(
        &selected_root,
        std::path::Path::new("gradle/libs.versions.toml"),
        "catalog",
    )
    .unwrap();

    assert!(content.contains("app = \"1\""));
}
