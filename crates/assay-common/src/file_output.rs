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

/// Refuse, before anything is spent, an output path that already leads to a still-open input.
///
/// An early answer, not the check that binds. [`open_preserving_inputs`] decides, on the handle
/// that is then written; this exists so that a run which will be refused is refused before it
/// has paid for its result. The refusal is the same error.
///
/// Asking costs nothing. The path is examined, never opened: nothing is created, nothing is
/// truncated, a pipe is not waited on and does not gain a reader, and a path that leads nowhere
/// is passed. It refuses only what it can show, that the path leads to the file an input is. A
/// path it cannot examine is passed and left to the open.
///
/// Only Unix can examine a path without opening it. Elsewhere every path is passed, and the open
/// refuses as it always has.
pub fn precheck_preserving_inputs(path: &Path, inputs: &[&File]) -> io::Result<()> {
    if leads_to_an_input(path, inputs) {
        return Err(refusal());
    }
    Ok(())
}

/// Whether the path leads, now, to the file one of the inputs is.
///
/// A file's identity on Unix is its device and inode, which is what `same_file` compares when
/// the output is opened. `stat` gives both for a path without opening what the path leads to.
/// Opening it is not a harmless question: a FIFO opened to read waits for a writer, or lets a
/// waiting writer through and then leaves it writing to nobody, and a device may act on being
/// opened.
#[cfg(unix)]
fn leads_to_an_input(path: &Path, inputs: &[&File]) -> bool {
    use std::os::unix::fs::MetadataExt;

    // Nothing there, or nothing that can be examined: this check says nothing and the open decides.
    let Ok(target) = std::fs::metadata(path) else {
        return false;
    };
    inputs.iter().any(|input| {
        input
            .metadata()
            .is_ok_and(|input| (input.dev(), input.ino()) == (target.dev(), target.ino()))
    })
}

/// Off Unix a path's identity is only had by opening it, and opening a pipe by its path uses the
/// pipe up. So nothing is examined here, and the open refuses as it always has.
#[cfg(not(unix))]
fn leads_to_an_input(_path: &Path, _inputs: &[&File]) -> bool {
    false
}

/// The one refusal, whether it comes early or at the open.
fn refusal() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "refusing to overwrite an input file",
    )
}

/// Open an output without truncating any of the still-open inputs.
/// Callers must supply the handles they read, not reopened paths. Regular outputs
/// are truncated after the identity check; publication is not atomic.
/// An input that is no file, such as a pipe on Windows, has no identity and is not compared.
pub fn open_preserving_inputs(path: &Path, inputs: &[&File]) -> io::Result<File> {
    let mut handles = Vec::with_capacity(inputs.len());
    for input in inputs {
        if let Some(handle) = identity(input)? {
            handles.push(handle);
        }
    }
    open_preserving_handles(path, &handles.iter().collect::<Vec<_>>())
}

/// The identity an open file is compared by.
#[cfg(not(windows))]
fn identity(file: &File) -> io::Result<Option<Handle>> {
    Handle::from_file(file.try_clone()?).map(Some)
}

/// The identity an open file is compared by, or `None` for a handle that has none.
///
/// Windows defines the identity query for files: its reference says the handle "should not be a
/// pipe handle", and a console or `NUL` is a device. The handle is therefore asked for its type
/// first and the query is never put to a pipe or a character device. Every other handle is still
/// asked, and a failure there is still an error.
#[cfg(windows)]
fn identity(file: &File) -> io::Result<Option<Handle>> {
    let kind = winapi_util::file::typ(file)?;
    if kind.is_pipe() || kind.is_char() {
        return Ok(None);
    }
    Handle::from_file(file.try_clone()?).map(Some)
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
    let Some(output) = identity(&file)? else {
        // No file: it cannot be one of the inputs, and there is nothing of it to truncate.
        return Ok(file);
    };
    for input in inputs {
        if &output == *input {
            return Err(refusal());
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

    /// An open pipe as a `File`, with its writing end kept so the pipe stays open.
    fn pipe_as_file() -> (File, io::PipeWriter) {
        let (reader, writer) = io::pipe().unwrap();
        #[cfg(unix)]
        let file = File::from(std::os::fd::OwnedFd::from(reader));
        #[cfg(windows)]
        let file = File::from(std::os::windows::io::OwnedHandle::from(reader));
        (file, writer)
    }

    /// A pipe has no file identity, and Windows does not define the query that asks for one on a
    /// pipe handle. The rule used to put that query to every input, so its answer for a pipe
    /// decided a write to a file that had nothing to do with the pipe.
    #[test]
    fn an_input_that_is_a_pipe_does_not_stop_a_write_to_a_separate_file() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("output");
        fs::write(&output, b"long old projection").unwrap();
        let (pipe, _writer) = pipe_as_file();
        write_preserving_inputs(&output, b"new", &[&pipe]).unwrap();
        assert_eq!(fs::read(&output).unwrap(), b"new");
    }

    /// The comparison is kept for the inputs that are files: a pipe beside one changes nothing.
    #[test]
    fn a_file_input_is_still_protected_when_another_input_is_a_pipe() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("input");
        fs::write(&source, b"original evidence").unwrap();
        let input = File::open(&source).unwrap();
        let (pipe, _writer) = pipe_as_file();
        for inputs in [[&pipe, &input], [&input, &pipe]] {
            let error = write_preserving_inputs(&source, b"projection", &inputs).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
            assert_eq!(fs::read(&source).unwrap(), b"original evidence");
        }
    }

    /// The device that discards what is written to it. It is no file either, and it has to stay
    /// usable as an output: nothing of it can be an input, and there is nothing of it to truncate.
    #[test]
    fn an_output_that_is_a_device_is_written_and_the_input_is_kept() {
        #[cfg(unix)]
        let device = Path::new("/dev/null");
        #[cfg(windows)]
        let device = Path::new("NUL");
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("input");
        fs::write(&source, b"original evidence").unwrap();
        let input = File::open(&source).unwrap();
        write_preserving_inputs(device, b"projection", &[&input]).unwrap();
        assert_eq!(fs::read(&source).unwrap(), b"original evidence");
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
