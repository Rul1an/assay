pub(super) struct ScopedTmpDir {
    path: std::path::PathBuf,
}

impl ScopedTmpDir {
    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for ScopedTmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Create a scoped temporary directory for sandbox isolation.
pub(super) fn create_scoped_tmp() -> anyhow::Result<ScopedTmpDir> {
    #[cfg(unix)]
    let uid = unsafe { libc::getuid() };
    #[cfg(not(unix))]
    let uid = std::env::var("USER")
        .map(|u| u.chars().take(8).collect::<String>())
        .unwrap_or_else(|_| "sandbox".to_string());

    let base = std::env::var("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));

    std::fs::create_dir_all(&base)?;
    for _ in 0..16 {
        let nonce = rand::random::<u128>();
        let tmp_dir = base.join(format!("assay-{uid}-{nonce:032x}"));
        match std::fs::create_dir(&tmp_dir) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let perms = std::fs::Permissions::from_mode(0o700);
                    std::fs::set_permissions(&tmp_dir, perms)?;
                }
                return Ok(ScopedTmpDir { path: tmp_dir });
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.into()),
        }
    }

    anyhow::bail!("failed to create unique assay sandbox tmp dir")
}
