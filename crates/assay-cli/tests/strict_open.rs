#![cfg(unix)]

#[cfg(target_os = "linux")]
use assay_common::strict_open;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use tempfile::NamedTempFile;

#[test]
#[cfg(target_os = "linux")]
fn test_strict_open_blocks_symlinks() {
    // 1. Setup: Create target file and symlink
    let target = NamedTempFile::new().expect("Failed to create target");
    let target_path = target.path().to_owned();
    let link_path = target_path.parent().unwrap().join("symlink_to_target");

    // Ensure clean state
    if link_path.exists() {
        std::fs::remove_file(&link_path).unwrap();
    }

    std::os::unix::fs::symlink(&target_path, &link_path).expect("Failed to create symlink");

    // 2. Attempt strict open on the symlink
    let c_link = CString::new(link_path.as_os_str().as_bytes()).unwrap();
    let res = strict_open::openat2_strict(&c_link);

    // 3. Cleanup immediately to avoid polluting /tmp (though NamedTempFile cleans target)
    let _ = std::fs::remove_file(&link_path);

    // 4. Verification
    match res {
        Ok(fd) => {
            unsafe { libc::close(fd) };
            panic!("Strict open SUCCEEDED on a symlink! (Security Failure)");
        }
        Err(e) => {
            // Check error code
            let errno = e.raw_os_error().unwrap_or(0);

            if errno == libc::ELOOP {
                println!("SUCCESS: Strict open blocked symlink with ELOOP.");
            } else if errno == libc::ENOSYS || errno == libc::EOPNOTSUPP {
                println!("SKIPPING: openat2 not supported on this kernel.");
            } else {
                panic!(
                    "Strict open failed with unexpected error: {} (Expected ELOOP)",
                    e
                );
            }
        }
    }
}

#[test]
#[cfg(target_os = "linux")]
fn test_strict_open_allows_regular_file() {
    let target = NamedTempFile::new().expect("Failed to create target");
    let c_path = CString::new(target.path().as_os_str().as_bytes()).unwrap();

    let res = strict_open::openat2_strict(&c_path);

    match res {
        Ok(fd) => {
            println!("SUCCESS: Strict open allowed regular file.");
            unsafe { libc::close(fd) };
        }
        Err(e) => {
            let errno = e.raw_os_error().unwrap_or(0);
            if errno == libc::ENOSYS || errno == libc::EOPNOTSUPP {
                println!("SKIPPING: openat2 not supported.");
            } else {
                panic!("Strict open FAILED on regular file: {}", e);
            }
        }
    }
}
