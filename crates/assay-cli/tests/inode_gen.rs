#![cfg(unix)]
#[cfg(target_os = "linux")]
use assay_common::get_inode_generation;
#[cfg(target_os = "linux")]
use std::os::unix::prelude::AsRawFd;
#[cfg(target_os = "linux")]
use tempfile::NamedTempFile;

#[test]
#[cfg(target_os = "linux")]
fn test_inode_generation_retrieval() {
    // 1. Create a temp file
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let fd = temp_file.as_raw_fd();

    // 2. Query generation
    // Supported FS (ext4, xfs, etc): Returns Ok(gen).
    // Unsupported (tmpfs, etc): May return ENOTTY or EOPNOTSUPP, or Ok(0).
    //
    // Since we don't know the runners FS type for /tmp, we check for
    // "No Panic" + "Reasonable Result".

    match get_inode_generation(fd) {
        Ok(gen) => {
            println!("Got generation: {}", gen);
            // gen might be 0 on tmpfs or young files, that's fine.
        }
        Err(e) => {
            // If the FS doesn't support it, we expect specific errors.
            // On tmpfs, ioctl might fail with ENOTTY (25) or similar.
            println!("Got error: {:?}", e);
            if let Some(code) = e.raw_os_error() {
                // ENOTTY means ioctl not supported on this file/fs.
                if code == libc::ENOTTY || code == libc::EOPNOTSUPP {
                    println!("FS does not support generation ioctl (expected on tmpfs).");
                } else {
                    panic!("Unexpected error querying generation: {:?}", e);
                }
            }
        }
    }
}
