pub(crate) async fn kill_pid(
    pid: u32,
    mode: assay_core::mcp::runtime_features::KillMode,
    grace_ms: u64,
) {
    unsafe {
        libc::kill(
            pid as i32,
            if mode == assay_core::mcp::runtime_features::KillMode::Immediate {
                libc::SIGKILL
            } else {
                libc::SIGTERM
            },
        );
    }
    if mode == assay_core::mcp::runtime_features::KillMode::Graceful {
        tokio::time::sleep(std::time::Duration::from_millis(grace_ms)).await;
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }
}

pub(crate) fn open_path_no_symlink(c_path: &std::ffi::CString) -> std::io::Result<i32> {
    let fd = unsafe {
        libc::open(
            c_path.as_ptr(),
            libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd >= 0 {
        Ok(fd)
    } else {
        Err(std::io::Error::last_os_error())
    }
}

pub(crate) fn fstat_fd(fd: i32) -> std::io::Result<libc::stat> {
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::fstat(fd, &mut stat) };
    if rc < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(stat)
    }
}

pub(crate) fn close_fd(fd: i32) {
    let _ = unsafe { libc::close(fd) };
}
