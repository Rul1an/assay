//! The early answer to "does this output path already lead to an input".
//!
//! `open_preserving_inputs` refuses an output that is one of the still-open inputs at the moment
//! it opens it. A caller that computes before it opens has by then paid for the computation.
//! `precheck_preserving_inputs` gives the same refusal before that, and asking must cost nothing:
//! it opens nothing, creates nothing and truncates nothing, so a path that does not exist, a pipe
//! and another file are afterwards exactly what they were.

#![cfg(feature = "std")]

use assay_common::file_output::{open_preserving_inputs, precheck_preserving_inputs};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

const EVIDENCE: &[u8] = b"original evidence";

/// An input with content, still open, and the path it was opened by.
fn input_in(dir: &Path) -> (PathBuf, File) {
    let path = dir.join("input");
    fs::write(&path, EVIDENCE).unwrap();
    let file = File::open(&path).unwrap();
    (path, file)
}

#[test]
fn a_path_that_does_not_exist_is_passed_and_is_not_created() {
    let dir = tempfile::tempdir().unwrap();
    let (_, input) = input_in(dir.path());
    let target = dir.path().join("not-there");

    precheck_preserving_inputs(&target, &[&input]).unwrap();

    assert!(!target.exists(), "the check must create nothing");
}

#[test]
fn another_file_is_passed_and_keeps_every_byte() {
    let dir = tempfile::tempdir().unwrap();
    let (_, input) = input_in(dir.path());
    let target = dir.path().join("output");
    fs::write(&target, b"an earlier projection").unwrap();

    precheck_preserving_inputs(&target, &[&input]).unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"an earlier projection");
}

/// Windows has no way to examine a path without opening it, and opening a pipe by its path uses
/// the pipe up. The early check therefore says nothing there, and the open is what refuses.
#[cfg(windows)]
#[test]
fn on_windows_the_early_check_says_nothing_and_the_open_still_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let (source, input) = input_in(dir.path());

    precheck_preserving_inputs(&source, &[&input]).unwrap();

    let refused = open_preserving_inputs(&source, &[&input]).unwrap_err();
    assert_eq!(refused.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(fs::read(&source).unwrap(), EVIDENCE);
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::io::{self, Read, Write};
    use std::os::unix::fs::{symlink, OpenOptionsExt, PermissionsExt};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    /// The answer, or a failure when the check waits on the path it was asked about.
    fn asked_within_a_bound(target: &Path, input: &File) -> io::Result<()> {
        let (answer, answered) = mpsc::channel();
        let target = target.to_path_buf();
        let input = input.try_clone().unwrap();
        thread::spawn(move || {
            let _ = answer.send(precheck_preserving_inputs(&target, &[&input]));
        });
        answered
            .recv_timeout(Duration::from_secs(5))
            .expect("the check must not wait on the path it is asked about")
    }

    fn fifo_in(dir: &Path) -> PathBuf {
        let path = dir.join("pipe");
        nix::unistd::mkfifo(&path, nix::sys::stat::Mode::from_bits_truncate(0o600)).unwrap();
        path
    }

    /// Refused early, with the error the open gives for the same path, and the input intact.
    fn assert_refused_as_the_open_refuses(target: &Path, input: &File, source: &Path) {
        let early = precheck_preserving_inputs(target, &[input])
            .expect_err("a path that leads to the input must be refused");
        assert_eq!(
            early.kind(),
            io::ErrorKind::InvalidInput,
            "{}",
            target.display()
        );
        let late = open_preserving_inputs(target, &[input]).expect_err("the open refuses it too");
        assert_eq!(
            early.to_string(),
            late.to_string(),
            "one refusal, worded once"
        );
        assert_eq!(fs::read(source).unwrap(), EVIDENCE);
    }

    #[test]
    fn the_input_is_refused_under_every_name_it_goes_by() {
        let dir = tempfile::tempdir().unwrap();
        let (source, input) = input_in(dir.path());
        let spelt = dir.path().join("sub").join("..").join("input");
        fs::create_dir(dir.path().join("sub")).unwrap();
        let hard = dir.path().join("hard");
        fs::hard_link(&source, &hard).unwrap();
        let link = dir.path().join("link");
        symlink(&source, &link).unwrap();
        let link_to_link = dir.path().join("link-to-link");
        symlink(&link, &link_to_link).unwrap();

        for target in [&source, &spelt, &hard, &link, &link_to_link] {
            assert_refused_as_the_open_refuses(target, &input, &source);
        }
    }

    /// Looking at a path needs no permission on the file itself, so a file that can no longer be
    /// opened for reading is still recognised. A check that opened the path would see nothing.
    #[test]
    fn an_input_whose_path_can_no_longer_be_read_is_still_recognised() {
        let dir = tempfile::tempdir().unwrap();
        let (source, input) = input_in(dir.path());
        fs::set_permissions(&source, fs::Permissions::from_mode(0o000)).unwrap();
        if File::open(&source).is_ok() {
            // Running with the privilege to read anything: the case cannot be set up.
            return;
        }

        let refused = precheck_preserving_inputs(&source, &[&input]).unwrap_err();

        assert_eq!(refused.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn a_link_to_nothing_is_passed_and_nothing_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let (_, input) = input_in(dir.path());
        let nothing = dir.path().join("nothing");
        let link = dir.path().join("dangling");
        symlink(&nothing, &link).unwrap();

        precheck_preserving_inputs(&link, &[&input]).unwrap();

        assert!(
            !nothing.exists(),
            "the check must not create the link's target"
        );
    }

    /// An input that is a pipe is not a file a path could lead to.
    #[test]
    fn an_input_that_is_a_pipe_refuses_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (reader, _writer) = io::pipe().unwrap();
        let pipe = File::from(std::os::fd::OwnedFd::from(reader));
        let target = dir.path().join("output");
        fs::write(&target, b"an earlier projection").unwrap();

        asked_within_a_bound(&target, &pipe).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"an earlier projection");
    }

    /// Opening a pipe to read it waits for a writer. The check must not: it is asked about the
    /// path, and nobody may be writing yet.
    #[test]
    fn a_pipe_nobody_writes_to_is_passed_without_waiting() {
        let dir = tempfile::tempdir().unwrap();
        let (_, input) = input_in(dir.path());
        let pipe = fifo_in(dir.path());

        asked_within_a_bound(&pipe, &input).unwrap();

        // And it left no reader behind: a writer that will not wait is still turned away.
        let no_reader = fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&pipe)
            .unwrap_err();
        assert_eq!(no_reader.raw_os_error(), Some(libc::ENXIO), "{no_reader}");
    }

    /// A writer waiting on the pipe is waiting for a reader. A check that opened the pipe, even
    /// for a moment, would be that reader: the writer would get through and then write to nobody.
    #[test]
    fn a_writer_waiting_on_a_pipe_is_left_waiting() {
        let dir = tempfile::tempdir().unwrap();
        let (_, input) = input_in(dir.path());
        let pipe = fifo_in(dir.path());
        let (about_to_open, is_about_to_open) = mpsc::channel();
        let (got_through, has_got_through) = mpsc::channel();
        let writer = thread::spawn({
            let pipe = pipe.clone();
            move || {
                about_to_open.send(()).unwrap();
                let mut pipe = fs::OpenOptions::new().write(true).open(pipe).unwrap();
                got_through.send(()).unwrap();
                pipe.write_all(b"x").unwrap();
            }
        });
        is_about_to_open.recv().unwrap();
        // Time for the writer to be inside its open. Too little only makes this test see less.
        thread::sleep(Duration::from_millis(100));

        asked_within_a_bound(&pipe, &input).unwrap();

        assert!(
            has_got_through
                .recv_timeout(Duration::from_millis(300))
                .is_err(),
            "the check became the pipe's reader and let a waiting writer through"
        );
        // Let the writer finish: be the reader it was waiting for.
        let mut reader = File::open(&pipe).unwrap();
        has_got_through
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte).unwrap();
        writer.join().unwrap();
    }

    /// The early answer and the open are one rule asked twice: whatever the open would refuse the
    /// early check refuses, and nothing else. Asked in that order, since the open creates and
    /// truncates what it accepts.
    #[test]
    fn the_early_answer_is_the_answer_the_open_gives() {
        let dir = tempfile::tempdir().unwrap();
        let (source, input) = input_in(dir.path());
        let other = dir.path().join("other");
        fs::write(&other, b"an earlier projection").unwrap();
        let hard = dir.path().join("hard");
        fs::hard_link(&source, &hard).unwrap();
        let link = dir.path().join("link");
        symlink(&source, &link).unwrap();
        let link_to_other = dir.path().join("link-to-other");
        symlink(&other, &link_to_other).unwrap();
        let missing = dir.path().join("missing");
        let device = File::open("/dev/null").unwrap();

        let cases: [(&str, &Path, &File); 8] = [
            ("the input's own path", &source, &input),
            ("a hard link to the input", &hard, &input),
            ("a symlink to the input", &link, &input),
            ("another file", &other, &input),
            ("a symlink to another file", &link_to_other, &input),
            ("a path that does not exist", &missing, &input),
            ("a device, the input a file", Path::new("/dev/null"), &input),
            (
                "the device the input is, by another path",
                Path::new("/dev/../dev/null"),
                &device,
            ),
        ];
        for (what, target, read) in cases {
            let early = precheck_preserving_inputs(target, &[read]).is_err();
            let late = open_preserving_inputs(target, &[read]).is_err();
            assert_eq!(
                early, late,
                "{what}: early refusal {early}, the open's {late}"
            );
        }
        assert_eq!(fs::read(&source).unwrap(), EVIDENCE);
    }
}
