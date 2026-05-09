/// Create a scoped temporary directory for sandbox isolation.
pub(super) fn create_scoped_tmp() -> anyhow::Result<std::path::PathBuf> {
    let pid = std::process::id();

    #[cfg(unix)]
    let uid = unsafe { libc::getuid() };
    #[cfg(not(unix))]
    let uid = std::env::var("USER")
        .map(|u| u.chars().take(8).collect::<String>())
        .unwrap_or_else(|_| "sandbox".to_string());

    let base = std::env::var("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));

    let tmp_dir = base.join(format!("assay-{}-{}", uid, pid));

    std::fs::create_dir_all(&tmp_dir)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o700);
        std::fs::set_permissions(&tmp_dir, perms)?;
    }

    Ok(tmp_dir)
}
