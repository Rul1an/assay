use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::string::{String, ToString};

#[derive(Debug)]
pub enum WriteNewError {
    InvalidName { name: String },
    TempCreate { dir: PathBuf, source: io::Error },
    TempPermissions { path: PathBuf, source: io::Error },
    Write { path: PathBuf, source: io::Error },
    SyncFile { path: PathBuf, source: io::Error },
    AlreadyExists { path: PathBuf },
    Publish { path: PathBuf, source: io::Error },
    SyncDir { dir: PathBuf, source: io::Error },
}

impl Display for WriteNewError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName { name } => write!(
                f,
                "refusing file name {name:?}: name must be one normal path component"
            ),
            Self::TempCreate { dir, source } => {
                write!(f, "creating temporary file in {}: {source}", dir.display())
            }
            Self::TempPermissions { path, source } => write!(
                f,
                "setting temporary file permissions for {}: {source}",
                path.display()
            ),
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
            Self::SyncDir { dir, source } => {
                write!(f, "syncing directory {}: {source}", dir.display())
            }
        }
    }
}

impl std::error::Error for WriteNewError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::TempCreate { source, .. }
            | Self::TempPermissions { source, .. }
            | Self::Write { source, .. }
            | Self::SyncFile { source, .. }
            | Self::Publish { source, .. }
            | Self::SyncDir { source, .. } => Some(source),
            Self::InvalidName { .. } | Self::AlreadyExists { .. } => None,
        }
    }
}

pub fn write_new(dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, WriteNewError> {
    validate_name(name)?;
    let target = dir.join(name);

    let mut temp =
        tempfile::NamedTempFile::new_in(dir).map_err(|source| WriteNewError::TempCreate {
            dir: dir.to_path_buf(),
            source,
        })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        temp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|source| WriteNewError::TempPermissions {
                path: target.clone(),
                source,
            })?;
    }

    use std::io::Write;
    temp.write_all(bytes)
        .map_err(|source| WriteNewError::Write {
            path: target.clone(),
            source,
        })?;
    temp.as_file()
        .sync_all()
        .map_err(|source| WriteNewError::SyncFile {
            path: target.clone(),
            source,
        })?;

    temp.persist_noclobber(&target).map_err(|error| {
        if error.error.kind() == io::ErrorKind::AlreadyExists {
            WriteNewError::AlreadyExists {
                path: target.clone(),
            }
        } else {
            WriteNewError::Publish {
                path: target.clone(),
                source: error.error,
            }
        }
    })?;

    File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|source| WriteNewError::SyncDir {
            dir: dir.to_path_buf(),
            source,
        })?;

    Ok(target)
}

fn validate_name(name: &str) -> Result<(), WriteNewError> {
    if name.is_empty() {
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
    use super::{write_new, WriteNewError};
    use std::fs;
    #[cfg(not(unix))]
    use std::path::Path;
    use std::vec;
    use std::vec::Vec;

    #[cfg(unix)]
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn writes_bytes_with_strict_permissions_and_no_temp_residue() {
        let dir = tempfile::tempdir().expect("temp dir");
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

    #[test]
    fn existing_regular_file_is_rejected_without_modifying_original() {
        let dir = tempfile::tempdir().expect("temp dir");
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
        let dir = tempfile::tempdir().expect("temp dir");
        let outside = tempfile::tempdir().expect("outside dir");
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
        let dir = tempfile::tempdir().expect("temp dir");

        for invalid in ["", "..", "nested/out.json"] {
            let error = write_new(dir.path(), invalid, b"payload").expect_err("must reject");
            assert!(
                matches!(error, WriteNewError::InvalidName { .. }),
                "expected InvalidName for {invalid:?}, got {error:?}"
            );
        }

        assert_eq!(fs::read_dir(dir.path()).expect("read_dir").count(), 0);
    }

    #[test]
    fn missing_or_non_directory_parent_is_rejected_without_creating_output() {
        let dir = tempfile::tempdir().expect("temp dir");
        let missing = dir.path().join("missing-parent");
        let error = write_new(&missing, "out.json", b"payload").expect_err("missing must fail");
        assert!(matches!(error, WriteNewError::TempCreate { .. }));
        assert!(!missing.join("out.json").exists());

        let file_as_dir = dir.path().join("not-a-dir");
        fs::write(&file_as_dir, b"marker").expect("seed marker file");
        let error =
            write_new(&file_as_dir, "out.json", b"payload").expect_err("non-dir parent fails");
        assert!(matches!(error, WriteNewError::TempCreate { .. }));
        assert!(!file_as_dir.join("out.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn temp_creation_failure_leaves_no_partial_target() {
        let dir = tempfile::tempdir().expect("temp dir");
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
        let dir = tempfile::tempdir().expect("temp dir");
        let out_path = dir.path().join("out.json");
        let bogus = Path::new("nested").join("parent");
        let error = write_new(dir.path().join(bogus).as_path(), "out.json", b"payload")
            .expect_err("must fail");
        assert!(matches!(error, WriteNewError::TempCreate { .. }));
        assert!(!out_path.exists());
    }
}
