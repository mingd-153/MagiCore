use std::fs;
use std::io;
use std::path::Path;

pub fn validate_cas_root(cas_path: &Path) -> io::Result<()> {
    if cas_path.is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "CAS root path is a symlink",
        ));
    }
    Ok(())
}

pub fn ensure_cas_dirs(cas_path: &Path) -> io::Result<()> {
    for i in 0..256u16 {
        let shard = format!("{:02x}", i);
        let dir = cas_path.join(&shard);
        if !dir.exists() {
            fs::create_dir_all(&dir)?;
        }
    }
    Ok(())
}

pub fn set_cas_root_permissions(cas_path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(cas_path)?.permissions();
        perms.set_mode(0o700);
        fs::set_permissions(cas_path, perms)?;
    }
    Ok(())
}
