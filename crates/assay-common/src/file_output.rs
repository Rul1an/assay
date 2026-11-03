//! File output that must not overwrite any of its still-open input files.

use same_file::Handle;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::vec::Vec;

/// Preserve input files while retaining normal overwrite semantics for other outputs.
/// Callers must retain the files they actually read until this call completes.
/// The output is checked and written through the same handle, without reopening its path.
/// This does not provide atomic output publication or protect against other writers.
pub fn write_preserving_inputs(path: &Path, bytes: &[u8], inputs: &[&File]) -> io::Result<()> {
    open_preserving_inputs(path, inputs)?.write_all(bytes)
}

/// Open an output without truncating any of the still-open inputs.
/// Callers must supply the handles they read, not reopened paths. Regular outputs
/// are truncated after the identity check; publication is not atomic.
pub fn open_preserving_inputs(path: &Path, inputs: &[&File]) -> io::Result<File> {
    let handles = inputs
        .iter()
        .map(|input| Handle::from_file(input.try_clone()?))
        .collect::<io::Result<Vec<_>>>()?;
    open_preserving_handles(path, &handles.iter().collect::<Vec<_>>())
}

/// Preserve retained input identities, including `Handle::stdin()` for redirected
/// stdin. Keep the original handles alive through this call. Using the stdin
/// constructor also permits Windows pipes/console handles without file identity.
/// Output publication is not atomic and concurrent writers are not excluded.
pub fn write_preserving_handles(path: &Path, bytes: &[u8], inputs: &[&Handle]) -> io::Result<()> {
    open_preserving_handles(path, inputs)?.write_all(bytes)
}

fn open_preserving_handles(path: &Path, inputs: &[&Handle]) -> io::Result<File> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    let output = Handle::from_file(file.try_clone()?)?;
    for input in inputs {
        if &output == *input {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "refusing to overwrite an input file",
            ));
        }
    }
    // Like fs::write, truncate regular files but keep stream/device outputs usable.
    if file.metadata()?.is_file() {
        file.set_len(0)?;
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn protects_the_open_input_even_after_its_path_changes() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("input");
        let moved = dir.path().join("moved");
        fs::write(&original, b"original evidence").unwrap();
        let input = File::open(&original).unwrap();
        fs::rename(&original, &moved).unwrap();
        fs::write(&original, b"replacement").unwrap();
        let error = write_preserving_inputs(&moved, b"projection", &[&input]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(fs::read(&moved).unwrap(), b"original evidence");
        assert_eq!(fs::read(&original).unwrap(), b"replacement");
    }

    #[cfg(unix)]
    #[test]
    fn permits_symlink_to_a_distinct_output_and_truncates_old_tail() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("input");
        let output = dir.path().join("output");
        let link = dir.path().join("link");
        fs::write(&source, b"evidence").unwrap();
        fs::write(&output, b"long old projection").unwrap();
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        fs::set_permissions(&output, fs::Permissions::from_mode(0o640)).unwrap();
        let original_inode = fs::metadata(&output).unwrap().ino();
        std::os::unix::fs::symlink(&output, &link).unwrap();
        let input = File::open(&source).unwrap();
        write_preserving_inputs(&link, b"new", &[&input]).unwrap();
        assert_eq!(fs::read(&output).unwrap(), b"new");
        assert_eq!(fs::read(&source).unwrap(), b"evidence");
        assert!(link.is_symlink());
        assert_eq!(fs::metadata(&output).unwrap().ino(), original_inode);
        assert_eq!(
            fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
}
