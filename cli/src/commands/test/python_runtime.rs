use super::{join_python_paths, python_path_env_value};
use std::ffi::OsString;
use std::path::PathBuf;

#[test]
fn managed_python_paths_precede_but_preserve_existing_paths() {
    let managed = PathBuf::from("mgc-site");
    let existing =
        std::env::join_paths([PathBuf::from("user-site"), PathBuf::from("system-site")]).unwrap();
    let joined = join_python_paths(vec![managed.clone()], Some(existing)).unwrap();
    let paths = std::env::split_paths(&joined).collect::<Vec<_>>();

    assert_eq!(paths[0], managed);
    assert_eq!(paths[1], PathBuf::from("user-site"));
    assert_eq!(paths[2], PathBuf::from("system-site"));
}

#[test]
fn managed_python_paths_work_without_an_existing_pythonpath() {
    let managed = PathBuf::from("mgc-site");
    let joined = join_python_paths(vec![managed.clone()], None).unwrap();
    assert_eq!(
        std::env::split_paths(&joined).collect::<Vec<_>>(),
        vec![managed]
    );
}

#[test]
fn pythonpath_is_kept_as_os_string_without_utf8_conversion() {
    let existing = OsString::from("user-site");
    let joined = join_python_paths(vec![PathBuf::from("mgc-site")], Some(existing)).unwrap();
    assert_eq!(std::env::split_paths(&joined).count(), 2);
}

#[test]
fn unicode_python_path_is_passed_without_lossy_conversion() {
    let path = OsString::from("/tmp/mgc-python-site");
    assert_eq!(python_path_env_value(path).unwrap(), "/tmp/mgc-python-site");
}

#[cfg(unix)]
#[test]
fn non_unicode_python_path_fails_clearly_instead_of_being_corrupted() {
    use std::os::unix::ffi::OsStringExt;
    assert!(python_path_env_value(OsString::from_vec(vec![0xff])).is_err());
}
