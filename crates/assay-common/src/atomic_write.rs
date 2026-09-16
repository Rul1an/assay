use std::fmt::{Display, Formatter};
use std::format;
use std::io;
#[cfg(not(unix))]
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::string::{String, ToString};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(not(unix))]
use std::fs::File;
#[cfg(not(unix))]
use std::fs::OpenOptions;

#[cfg(unix)]
use nix::errno::Errno;
#[cfg(unix)]
use nix::fcntl::{open, openat, OFlag};
#[cfg(unix)]
use nix::sys::stat::Mode;
#[cfg(unix)]
use nix::unistd::{close, fsync, linkat, unlinkat, write as nix_write, LinkatFlags, UnlinkatFlags};
#[cfg(unix)]
use std::os::fd::{AsRawFd, RawFd};

const MAX_TEMP_CREATE_ATTEMPTS: usize = 32;

#[derive(Debug)]
pub enum WriteNewError {
    InvalidName { name: String },
    OpenDir { dir: PathBuf, source: io::Error },
    TempCreate { path: PathBuf, source: io::Error },
    Write { path: PathBuf, source: io::Error },
    SyncFile { path: PathBuf, source: io::Error },
    AlreadyExists { path: PathBuf },
    Publish { path: PathBuf, source: io::Error },
    TempCleanup { path: PathBuf, source: io::Error },
    SyncDir { dir: PathBuf, source: io::Error },
}

impl Display for WriteNewError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName { name } => write!(
                f,
                "refusing file name {name:?}: name must be one normal path component without backslashes"
            ),
            Self::OpenDir { dir, source } => {
                write!(f, "opening output directory {} securely: {source}", dir.display())
            }
            Self::TempCreate { path, source } => {
                write!(f, "creating temporary file for {}: {source}", path.display())
            }
            Self::Write { path, source } => {
                write!(f, "writing temporary file for {}: {source}", path.display())
            }
            Self::SyncFile { path, source } => {
                write!(f, "syncing temporary file for {}: {source}", path.display())
            }
            Self::AlreadyExists { path } => write!(
                f,
                "creating {} without overwriting an existing path",
                path.display()
            ),
            Self::Publish { path, source } => {
                write!(f, "publishing {} atomically: {source}", path.display())
            }
            Self::TempCleanup { path, source } => {
                write!(f, "cleaning up temporary file {}: {source}", path.display())
            }
            Self::SyncDir { dir, source } => {
                write!(f, "syncing directory {}: {source}", dir.display())
            }
        }
    }
}

impl std::error::Error for WriteNewError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::OpenDir { source, .. }
            | Self::TempCreate { source, .. }
            | Self::Write { source, .. }
            | Self::SyncFile { source, .. }
            | Self::Publish { source, .. }
            | Self::TempCleanup { source, .. }
            | Self::SyncDir { source, .. } => Some(source),
            Self::InvalidName { .. } | Self::AlreadyExists { .. } => None,
        }
    }
}

pub fn write_new(dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, WriteNewError> {
    validate_name(name)?;

    #[cfg(unix)]
    {
        write_new_unix(dir, name, bytes)?;
    }
    #[cfg(not(unix))]
    {
        write_new_non_unix(dir, name, bytes)?;
    }

    Ok(dir.join(name))
}

#[cfg(unix)]
pub fn write_new_at(dir: &std::fs::File, name: &str, bytes: &[u8]) -> Result<(), WriteNewError> {
    write_new_unix_at_fd(
        dir.as_raw_fd(),
        PathBuf::from("<opened-dir-fd>"),
        name,
        bytes,
    )
}

#[cfg(unix)]
fn write_new_unix(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), WriteNewError> {
    let dir_fd = open(
        dir,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|errno| WriteNewError::OpenDir {
        dir: dir.to_path_buf(),
        source: io_error(errno),
    })?;

    let result = write_new_unix_at_fd(dir_fd, dir.to_path_buf(), name, bytes);
    let close_result = close(dir_fd);
    if let Err(errno) = close_result {
        if result.is_ok() {
            return Err(WriteNewError::SyncDir {
                dir: dir.to_path_buf(),
                source: io_error(errno),
            });
        }
    }
    result
}

#[cfg(unix)]
fn write_new_unix_at_fd(
    dir_fd: RawFd,
    dir_label: PathBuf,
    name: &str,
    bytes: &[u8],
) -> Result<(), WriteNewError> {
    validate_name(name)?;
    let target = PathBuf::from(name);
    let (temp_name, temp_fd) = create_temp_fd(dir_fd, name, &target)?;

    let write_result = (|| {
        write_all_fd(temp_fd, bytes).map_err(|source| WriteNewError::Write {
            path: target.clone(),
            source,
        })?;
        fsync(temp_fd).map_err(|errno| WriteNewError::SyncFile {
            path: target.clone(),
            source: io_error(errno),
        })?;
        Ok::<(), WriteNewError>(())
    })();

    let close_result = close(temp_fd);
    if let Err(error) = write_result {
        let _ = cleanup_temp_at(dir_fd, &temp_name, &target);
        return Err(error);
    }
    if let Err(errno) = close_result {
        let _ = cleanup_temp_at(dir_fd, &temp_name, &target);
        return Err(WriteNewError::SyncFile {
            path: target.clone(),
            source: io_error(errno),
        });
    }

    match linkat(
        Some(dir_fd),
        temp_name.as_str(),
        Some(dir_fd),
        name,
        LinkatFlags::NoSymlinkFollow,
    ) {
        Ok(()) => {}
        Err(Errno::EEXIST) => {
            let _ = cleanup_temp_at(dir_fd, &temp_name, &target);
            return Err(WriteNewError::AlreadyExists { path: target });
        }
        Err(errno) => {
            let _ = cleanup_temp_at(dir_fd, &temp_name, &target);
            return Err(WriteNewError::Publish {
                path: target,
                source: io_error(errno),
            });
        }
    }

    cleanup_temp_at(dir_fd, &temp_name, &target)?;
    fsync(dir_fd).map_err(|errno| WriteNewError::SyncDir {
        dir: dir_label,
        source: io_error(errno),
    })?;
    Ok(())
}

#[cfg(unix)]
fn create_temp_fd(
    dir_fd: RawFd,
    name: &str,
    target: &Path,
) -> Result<(String, RawFd), WriteNewError> {
    for attempt in 0..MAX_TEMP_CREATE_ATTEMPTS {
        let temp_name = temporary_name(name, attempt);
        match openat(
            dir_fd,
            temp_name.as_str(),
            OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_WRONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::from_bits_truncate(0o600),
        ) {
            Ok(fd) => return Ok((temp_name, fd)),
            Err(Errno::EEXIST) => continue,
            Err(errno) => {
                return Err(WriteNewError::TempCreate {
                    path: target.to_path_buf(),
                    source: io_error(errno),
                });
            }
        }
    }

    Err(WriteNewError::TempCreate {
        path: target.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "exhausted temporary file name retries",
        ),
    })
}

#[cfg(unix)]
fn write_all_fd(fd: RawFd, mut remaining: &[u8]) -> Result<(), io::Error> {
    while !remaining.is_empty() {
        match nix_write(fd, remaining) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "short write while persisting atomic output",
                ));
            }
            Ok(written) => {
                remaining = &remaining[written..];
            }
            Err(Errno::EINTR) => continue,
            Err(errno) => return Err(io_error(errno)),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn cleanup_temp_at(dir_fd: RawFd, temp_name: &str, target: &Path) -> Result<(), WriteNewError> {
    unlinkat(Some(dir_fd), temp_name, UnlinkatFlags::NoRemoveDir).map_err(|errno| {
        WriteNewError::TempCleanup {
            path: target.to_path_buf(),
            source: io_error(errno),
        }
    })
}

#[cfg(unix)]
fn io_error(errno: Errno) -> io::Error {
    io::Error::from_raw_os_error(errno as i32)
}

#[cfg(not(unix))]
fn write_new_non_unix(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), WriteNewError> {
    let target = dir.join(name);
    let (temp_name, temp_path, mut file) = create_temp_file_non_unix(dir, name, &target)?;
    file.write_all(bytes)
        .map_err(|source| WriteNewError::Write {
            path: target.clone(),
            source,
        })?;
    file.sync_all().map_err(|source| WriteNewError::SyncFile {
        path: target.clone(),
        source,
    })?;
    drop(file);

    match std::fs::hard_link(&temp_path, &target) {
        Ok(()) => {}
        Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
            let _ = std::fs::remove_file(&temp_path);
            return Err(WriteNewError::AlreadyExists { path: target });
        }
        Err(source) => {
            let _ = std::fs::remove_file(&temp_path);
            return Err(WriteNewError::Publish {
                path: target,
                source,
            });
        }
    }

    std::fs::remove_file(&temp_path).map_err(|source| WriteNewError::TempCleanup {
        path: PathBuf::from(temp_name),
        source,
    })?;
    // Windows has no portable directory fsync in std, so we stop at synced file + hard-link publish.
    Ok(())
}

#[cfg(not(unix))]
fn create_temp_file_non_unix(
    dir: &Path,
    name: &str,
    target: &Path,
) -> Result<(String, PathBuf, File), WriteNewError> {
    for attempt in 0..MAX_TEMP_CREATE_ATTEMPTS {
        let temp_name = temporary_name(name, attempt);
        let temp_path = dir.join(&temp_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => return Ok((temp_name, temp_path, file)),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(WriteNewError::TempCreate {
                    path: target.to_path_buf(),
                    source,
                });
            }
        }
    }

    Err(WriteNewError::TempCreate {
        path: target.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "exhausted temporary file name retries",
        ),
    })
}

fn temporary_name(name: &str, attempt: usize) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!(".{name}.tmp.{:x}.{nanos:x}.{attempt}", std::process::id())
}

fn validate_name(name: &str) -> Result<(), WriteNewError> {
    if name.is_empty() || name.contains('\\') {
        return Err(WriteNewError::InvalidName {
            name: name.to_string(),
        });
    }

    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(WriteNewError::InvalidName {
            name: name.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::write_new_at;
    use super::{write_new, WriteNewError};
    use std::fs;
    use std::path::Path;
    use std::vec;
    use std::vec::Vec;
    use tempfile::TempDir;

    #[cfg(unix)]
    use std::os::unix::fs::{symlink, PermissionsExt};

    struct TestDir {
        dir: TempDir,
    }

    impl TestDir {
        fn new() -> Self {
            Self {
                dir: tempfile::tempdir().expect("create unique temp dir"),
            }
        }

        fn path(&self) -> &Path {
            self.dir.path()
        }
    }

    #[test]
    fn writes_bytes_with_strict_permissions_and_no_temp_residue() {
        let dir = TestDir::new();
        let payload = b"{\"ok\":true}";
        let out = write_new(dir.path(), "out.json", payload).expect("write succeeds");

        assert_eq!(out, dir.path().join("out.json"));
        assert_eq!(fs::read(&out).expect("read output"), payload);

        #[cfg(unix)]
        {
            let mode = fs::metadata(&out).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "output mode must be 0600");
        }

        let entries = fs::read_dir(dir.path())
            .expect("list output dir")
            .map(|entry| entry.expect("entry").file_name())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec![std::ffi::OsString::from("out.json")]);
    }

    #[cfg(unix)]
    #[test]
    fn write_stays_bound_to_original_directory_fd_after_directory_swap() {
        let root = TestDir::new();
        let opened_dir_path = root.path().join("output");
        let moved_dir_path = root.path().join("output-moved");
        let leaked_dir = TestDir::new();
        fs::create_dir(&opened_dir_path).expect("create output directory");

        let dir_file = std::fs::File::open(&opened_dir_path).expect("open directory fd");
        fs::rename(&opened_dir_path, &moved_dir_path).expect("move original directory");
        symlink(leaked_dir.path(), &opened_dir_path).expect("replace path with symlink");

        write_new_at(&dir_file, "out.json", b"payload").expect("write through opened directory");

        assert_eq!(
            fs::read(moved_dir_path.join("out.json")).expect("read moved output"),
            b"payload"
        );
        assert!(
            !leaked_dir.path().join("out.json").exists(),
            "write must not escape into symlink target"
        );
    }

    #[test]
    fn existing_regular_file_is_rejected_without_modifying_original() {
        let dir = TestDir::new();
        let out = dir.path().join("out.json");
        fs::write(&out, b"original").expect("seed original");

        let error = write_new(dir.path(), "out.json", b"new payload").expect_err("must reject");
        assert!(matches!(error, WriteNewError::AlreadyExists { .. }));
        assert_eq!(fs::read(&out).expect("read original"), b"original");
        assert_eq!(fs::read_dir(dir.path()).expect("read_dir").count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn existing_symlink_is_rejected_without_touching_link_or_target() {
        let dir = TestDir::new();
        let outside = TestDir::new();
        let outside_file = outside.path().join("outside.txt");
        fs::write(&outside_file, b"outside-original").expect("seed outside");

        let link = dir.path().join("out.json");
        symlink(&outside_file, &link).expect("create symlink");

        let error = write_new(dir.path(), "out.json", b"new payload").expect_err("must reject");
        assert!(matches!(error, WriteNewError::AlreadyExists { .. }));
        let link_meta = fs::symlink_metadata(&link).expect("link metadata");
        assert!(link_meta.file_type().is_symlink());
        assert_eq!(
            fs::read(&outside_file).expect("outside content"),
            b"outside-original"
        );
        assert_eq!(fs::read_dir(dir.path()).expect("read_dir").count(), 1);
    }

    #[test]
    fn invalid_names_are_rejected_before_creating_anything() {
        let dir = TestDir::new();

        for invalid in ["", ".", "..", "nested/out.json", "a\\b"] {
            let error = write_new(dir.path(), invalid, b"payload").expect_err("must reject");
            assert!(
                matches!(error, WriteNewError::InvalidName { .. }),
                "expected InvalidName for {invalid:?}, got {error:?}"
            );
        }

        assert_eq!(fs::read_dir(dir.path()).expect("read_dir").count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_directory_path_is_rejected_without_writing_to_link_target() {
        let dir = TestDir::new();
        let real_output = TestDir::new();
        let symlink_path = dir.path().join("output-link");
        symlink(real_output.path(), &symlink_path).expect("create output symlink");

        let error = write_new(&symlink_path, "out.json", b"payload").expect_err("must reject");
        assert!(matches!(error, WriteNewError::OpenDir { .. }));
        assert!(!real_output.path().join("out.json").exists());
    }

    #[test]
    fn missing_or_non_directory_parent_is_rejected_without_creating_output() {
        let dir = TestDir::new();
        let missing = dir.path().join("missing-parent");
        let error = write_new(&missing, "out.json", b"payload").expect_err("missing must fail");

        #[cfg(unix)]
        assert!(matches!(error, WriteNewError::OpenDir { .. }));
        #[cfg(not(unix))]
        assert!(matches!(error, WriteNewError::TempCreate { .. }));
        assert!(!missing.join("out.json").exists());

        let file_as_dir = dir.path().join("not-a-dir");
        fs::write(&file_as_dir, b"marker").expect("seed marker file");
        let error =
            write_new(&file_as_dir, "out.json", b"payload").expect_err("non-dir parent fails");
        #[cfg(unix)]
        assert!(matches!(error, WriteNewError::OpenDir { .. }));
        #[cfg(not(unix))]
        assert!(matches!(error, WriteNewError::TempCreate { .. }));
        assert!(!file_as_dir.join("out.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn temp_creation_failure_leaves_no_partial_target() {
        let dir = TestDir::new();
        let perms_before = fs::metadata(dir.path())
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500))
            .expect("chmod read-only");

        let out_path = dir.path().join("out.json");
        let error = write_new(dir.path(), "out.json", b"payload").expect_err("must fail");
        assert!(matches!(error, WriteNewError::TempCreate { .. }));
        assert!(!out_path.exists());

        fs::set_permissions(dir.path(), fs::Permissions::from_mode(perms_before))
            .expect("restore permissions");
        assert_eq!(fs::read_dir(dir.path()).expect("read_dir").count(), 0);
    }

    #[cfg(not(unix))]
    #[test]
    fn temp_creation_failure_leaves_no_partial_target() {
        let dir = TestDir::new();
        let out_path = dir.path().join("out.json");
        let bogus = Path::new("nested").join("parent");
        let error = write_new(dir.path().join(bogus).as_path(), "out.json", b"payload")
            .expect_err("must fail");
        assert!(matches!(error, WriteNewError::TempCreate { .. }));
        assert!(!out_path.exists());
    }
}
