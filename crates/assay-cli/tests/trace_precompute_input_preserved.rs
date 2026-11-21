//! `assay trace precompute-embeddings` and `assay trace precompute-judge` never truncate the trace
//! they are reading.
//!
//! Both commands open the trace and then open their output for writing. The output was opened by
//! path alone, so an `--output` that was the trace itself under another name was truncated before a
//! line of it was read: the command ended with 0 and the trace was empty. The same happened when
//! enriching in place found its scratch name, `<trace>.tmp.jsonl`, already linked to the trace.
//!
//! The output is now opened through the writer that refuses to truncate one of its inputs, the rule
//! `assay explain` follows. These tests pin the refusal for every name the trace can go by, and that
//! the routes which worked still work: enriching in place, a new target, an existing target, a pipe
//! and a device as the output, and a named pipe as the trace.
//!
//! Out of scope here, and unchanged: the fixed scratch name itself, what a failed run leaves behind,
//! and what enriching in place does to the trace's own metadata.
//!
//! From 7.0 both commands read a native episode stream only (ADR-052). An older trace record is
//! refused with the converter's name, before either file is touched, and a trace that
//! `assay trace migrate` converted is enriched like any other.

#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;
#[path = "../../../tests/support/named_pipe.rs"]
mod named_pipe;

use bounded_process::{run_bounded, ProcessLimits};
use named_pipe::{Fed, NamedPipe};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

/// One native episode, spaced unlike anything serde writes, so a trace that was rewritten differs
/// from it in bytes even when nothing was added.
const TRACE: &str = "{\"type\": \"episode_start\", \"episode_id\": \"hello_1\", \"timestamp\": 0, \
                     \"input\": {\"prompt\": \"hello_prompt\"}, \"meta\": {}}\n\
                     {\"type\": \"episode_end\", \"episode_id\": \"hello_1\", \"timestamp\": 1, \
                     \"final_output\": \"Hello Assay\"}\n";

/// The same answer in each form that 6.x read and 7.0 leaves to `assay trace migrate`: an
/// `assay.trace` record, a record without a `type`, and the positional record.
const OLD_RECORDS: [(&str, &str); 3] = [
    (
        "assay.trace record",
        "{\"schema_version\": 1, \"type\": \"assay.trace\", \"request_id\": \"hello_1\", \
         \"prompt\": \"hello_prompt\", \"response\": \"Hello Assay\", \"meta\": {}}\n",
    ),
    (
        "untagged record",
        "{\"request_id\": \"hello_1\", \"prompt\": \"hello_prompt\", \"response\": \"Hello Assay\"}\n",
    ),
    (
        "positional record",
        "[1, \"assay.trace\", \"hello_1\", \"hello_prompt\", \"Hello Assay\", {}]\n",
    ),
];

#[derive(Clone, Copy, Debug)]
enum Precompute {
    Embeddings,
    Judge,
}

impl Precompute {
    fn subcommand(self) -> &'static str {
        match self {
            Precompute::Embeddings => "precompute-embeddings",
            Precompute::Judge => "precompute-judge",
        }
    }

    /// The offline provider of each command.
    fn provider(self) -> [&'static str; 2] {
        match self {
            Precompute::Embeddings => ["--embedder", "fake"],
            Precompute::Judge => ["--judge", "fake"],
        }
    }

    /// A config under which a readable trace succeeds offline. The fake embedder answers, so the
    /// embeddings config has a test that needs it. The fake judge returns no verdict, so the judge
    /// config has nothing to judge and every record passes through.
    fn config(self) -> &'static str {
        match self {
            Precompute::Embeddings => {
                "version: 1\nsuite: precompute_input\nmodel: trace\ntests:\n  - id: hello_1\n    \
                 input:\n      prompt: hello_prompt\n    expected:\n      type: \
                 semantic_similarity_to\n      semantic_similarity_to: \"Hello Assay\"\n      \
                 min_score: 0.5\n"
            }
            Precompute::Judge => {
                "version: 1\nsuite: precompute_input\nmodel: trace\ntests:\n  - id: hello_1\n    \
                 input:\n      prompt: hello_prompt\n    expected:\n      type: must_contain\n      \
                 must_contain: [\"Hello\"]\n"
            }
        }
    }

    /// Whether a run that succeeds under `config` adds to the episode.
    fn enriches(self) -> bool {
        matches!(self, Precompute::Embeddings)
    }
}

/// A throwaway directory with `eval.yaml` and the trace `t.jsonl`.
fn suite(command: Precompute) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("eval.yaml"), command.config()).expect("eval.yaml");
    fs::write(dir.path().join("t.jsonl"), TRACE).expect("t.jsonl");
    dir
}

/// The command on `trace`, in `dir`, offline.
fn assay(command: Precompute, dir: &Path, trace: &Path, output: Option<&str>) -> Command {
    let mut assay = Command::new(env!("CARGO_BIN_EXE_assay"));
    assay
        .current_dir(dir)
        .env("ASSAY_EXIT_CODES", "v2")
        .env("ASSAY_VCR_MODE", "off")
        .env("NO_COLOR", "1")
        .env_remove("OPENAI_API_KEY")
        .args(["trace", command.subcommand()])
        .args(command.provider())
        .args(["--config", "eval.yaml", "--trace"])
        .arg(trace);
    if let Some(output) = output {
        assay.args(["--output", output]);
    }
    assay
}

fn run(command: Precompute, dir: &Path, output: Option<&str>) -> Output {
    assay(command, dir, Path::new("t.jsonl"), output)
        .output()
        .expect("run assay binary")
}

fn describe(out: &Output) -> String {
    format!(
        "exit={:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The run refused to open its output because the output is the trace, and the trace is byte for
/// byte what it was.
fn assert_refused_and_the_trace_kept(command: Precompute, out: &Output, dir: &Path) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.code() == Some(2)
            && stderr.contains("failed to create output trace file")
            && stderr.contains("refusing to overwrite an input file"),
        "{command:?}: an output that is the trace must be refused\n{}",
        describe(out)
    );
    assert_eq!(
        fs::read_to_string(dir.join("t.jsonl")).expect("trace"),
        TRACE,
        "{command:?}: the trace must be byte for byte what it was"
    );
}

/// The text holds exactly the one episode, whole, its start enriched when the command enriches.
fn assert_holds_the_episode(command: Precompute, text: &str, what: &str) {
    let events: Vec<serde_json::Value> = text
        .lines()
        .filter(|line| line.starts_with('{'))
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("{command:?}: {what} holds a broken line: {e}"))
        })
        .collect();
    assert_eq!(
        events.len(),
        2,
        "{command:?}: {what} must hold the episode's two events, found {text:?}"
    );
    assert_eq!(events[0]["type"], "episode_start", "{command:?}: {what}");
    assert_eq!(events[0]["episode_id"], "hello_1", "{command:?}: {what}");
    assert_eq!(
        events[0]["input"]["prompt"], "hello_prompt",
        "{command:?}: {what}"
    );
    assert_eq!(events[1]["type"], "episode_end", "{command:?}: {what}");
    assert_eq!(
        events[1]["final_output"], "Hello Assay",
        "{command:?}: {what}"
    );
    assert_enriched_as_the_command_enriches(command, &events[0], what);
}

/// An episode start carries the embedding when the command enriches, and only then.
fn assert_enriched_as_the_command_enriches(
    command: Precompute,
    start: &serde_json::Value,
    what: &str,
) {
    let embedded = start
        .pointer("/meta/assay/embeddings/response")
        .and_then(|vector| vector.as_array())
        .map(Vec::len);
    let expected = command.enriches().then_some(1536);
    assert_eq!(embedded, expected, "{command:?}: {what}");
}

fn assert_file_holds_the_episode(command: Precompute, path: &Path) {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    // A file holds the episode and nothing else. What an inherited pipe carries may be more, which
    // is why the count above looks at events only; here every line counts, so what an older and
    // longer file left behind is seen.
    assert_eq!(
        text.lines().count(),
        2,
        "{command:?}: {} must hold the episode and nothing after it",
        path.display()
    );
    assert_holds_the_episode(command, &text, &path.display().to_string());
}

// --- An output that is the trace, under any name, is refused -----------------------------------

fn an_output_that_spells_the_trace_differently_is_refused(command: Precompute) {
    let dir = suite(command);
    let out = run(command, dir.path(), Some("./t.jsonl"));
    assert_refused_and_the_trace_kept(command, &out, dir.path());
}

fn an_output_that_gives_the_trace_as_an_absolute_path_is_refused(command: Precompute) {
    let dir = suite(command);
    let absolute = dir.path().join("t.jsonl");
    let out = run(
        command,
        dir.path(),
        Some(absolute.to_str().expect("utf-8 path")),
    );
    assert_refused_and_the_trace_kept(command, &out, dir.path());
}

fn an_output_hard_linked_to_the_trace_is_refused(command: Precompute) {
    let dir = suite(command);
    fs::hard_link(dir.path().join("t.jsonl"), dir.path().join("hard.jsonl")).expect("hard link");
    let out = run(command, dir.path(), Some("hard.jsonl"));
    assert_refused_and_the_trace_kept(command, &out, dir.path());
    assert_eq!(
        fs::read_to_string(dir.path().join("hard.jsonl")).expect("hard link"),
        TRACE,
        "{command:?}"
    );
}

#[cfg(unix)]
fn an_output_symlinked_to_the_trace_is_refused(command: Precompute) {
    let dir = suite(command);
    std::os::unix::fs::symlink("t.jsonl", dir.path().join("link.jsonl")).expect("symlink");
    let out = run(command, dir.path(), Some("link.jsonl"));
    assert_refused_and_the_trace_kept(command, &out, dir.path());
    assert!(
        dir.path().join("link.jsonl").is_symlink(),
        "{command:?}: the link must still be a link"
    );
}

// --- Enriching in place writes through `<trace>.tmp.jsonl`. That name can already be the trace ---

fn a_scratch_name_hard_linked_to_the_trace_is_refused(command: Precompute) {
    let dir = suite(command);
    fs::hard_link(dir.path().join("t.jsonl"), dir.path().join("t.tmp.jsonl")).expect("hard link");
    let out = run(command, dir.path(), None);
    assert_refused_and_the_trace_kept(command, &out, dir.path());
    assert_eq!(
        fs::read_to_string(dir.path().join("t.tmp.jsonl")).expect("scratch name"),
        TRACE,
        "{command:?}"
    );
}

#[cfg(unix)]
fn a_scratch_name_symlinked_to_the_trace_is_refused(command: Precompute) {
    let dir = suite(command);
    std::os::unix::fs::symlink("t.jsonl", dir.path().join("t.tmp.jsonl")).expect("symlink");
    let out = run(command, dir.path(), None);
    assert_refused_and_the_trace_kept(command, &out, dir.path());
    assert!(
        !dir.path().join("t.jsonl").is_symlink(),
        "{command:?}: the trace must still be a file, not a link"
    );
    assert!(
        dir.path().join("t.tmp.jsonl").is_symlink(),
        "{command:?}: the scratch name must be left as it was"
    );
}

// --- The routes that worked still work ----------------------------------------------------------

fn enriching_in_place_without_an_output_still_works(command: Precompute) {
    let dir = suite(command);
    let out = run(command, dir.path(), None);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{command:?}\n{}",
        describe(&out)
    );
    assert_file_holds_the_episode(command, &dir.path().join("t.jsonl"));
    assert!(
        !dir.path().join("t.tmp.jsonl").exists(),
        "{command:?}: the scratch file is renamed over the trace"
    );
}

fn an_output_spelt_exactly_as_the_trace_still_enriches_in_place(command: Precompute) {
    let dir = suite(command);
    let out = run(command, dir.path(), Some("t.jsonl"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "{command:?}\n{}",
        describe(&out)
    );
    assert_file_holds_the_episode(command, &dir.path().join("t.jsonl"));
}

fn a_new_target_still_gets_the_trace_and_the_source_is_untouched(command: Precompute) {
    let dir = suite(command);
    let out = run(command, dir.path(), Some("out.jsonl"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "{command:?}\n{}",
        describe(&out)
    );
    assert_file_holds_the_episode(command, &dir.path().join("out.jsonl"));
    assert_eq!(
        fs::read_to_string(dir.path().join("t.jsonl")).expect("trace"),
        TRACE,
        "{command:?}"
    );
}

/// An existing target is still written over whole, and is still the same file.
fn an_existing_target_is_still_rewritten_whole(command: Precompute) {
    let dir = suite(command);
    let target = dir.path().join("out.jsonl");
    // Longer than anything either command writes, so a target that kept its old length would keep
    // a tail of this. The enriched record alone is tens of kilobytes.
    let older = "an older and much longer output line\n".repeat(8000);
    fs::write(&target, &older).expect("target");
    #[cfg(unix)]
    let before = {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).expect("chmod");
        let metadata = fs::metadata(&target).expect("metadata");
        (metadata.ino(), metadata.mode() & 0o777)
    };

    let out = run(command, dir.path(), Some("out.jsonl"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "{command:?}\n{}",
        describe(&out)
    );
    assert_file_holds_the_episode(command, &target);
    let written = fs::metadata(&target).expect("metadata").len();
    assert!(
        written < older.len() as u64,
        "{command:?}: the older content must be the longer one, or this shows nothing: \
         {written} bytes written over {}",
        older.len()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::metadata(&target).expect("metadata");
        assert_eq!(
            (metadata.ino(), metadata.mode() & 0o777),
            before,
            "{command:?}: an existing target is the same file, with the mode it had"
        );
    }
}

/// A pipe this process was handed, named by its descriptor: the lines still go into it.
#[cfg(unix)]
fn an_inherited_pipe_still_receives_the_trace(command: Precompute) {
    let dir = suite(command);
    // Standard output is a pipe here, and `/dev/fd/1` names it.
    let out = run(command, dir.path(), Some("/dev/fd/1"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "{command:?}\n{}",
        describe(&out)
    );
    assert_holds_the_episode(command, &String::from_utf8_lossy(&out.stdout), "the pipe");
    assert_eq!(
        fs::read_to_string(dir.path().join("t.jsonl")).expect("trace"),
        TRACE,
        "{command:?}"
    );
}

#[cfg(unix)]
fn a_device_is_still_written_to_and_stays_a_device(command: Precompute) {
    use std::os::unix::fs::FileTypeExt;
    let dir = suite(command);
    let out = run(command, dir.path(), Some("/dev/null"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "{command:?}\n{}",
        describe(&out)
    );
    assert!(
        fs::metadata("/dev/null")
            .expect("/dev/null")
            .file_type()
            .is_char_device(),
        "{command:?}: /dev/null must still be a device"
    );
}

// --- An older trace is refused with the converter's name; what the converter writes is enriched ---

/// Each older form is refused before anything is written: the trace and an existing target keep
/// their bytes, a new target is not created, and enriching in place leaves no scratch file.
fn an_older_trace_record_is_refused_and_both_files_are_kept(command: Precompute) {
    for (form, old) in OLD_RECORDS {
        let dir = suite(command);
        fs::write(dir.path().join("t.jsonl"), old).expect("t.jsonl");
        fs::write(dir.path().join("out.jsonl"), "previous output\n").expect("out.jsonl");
        for (output, what) in [
            (Some("out.jsonl"), "an existing target"),
            (Some("new.jsonl"), "a new target"),
            (None, "in place"),
        ] {
            let out = run(command, dir.path(), output);
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(
                out.status.code() == Some(2)
                    && stderr.contains("line 1: legacy replay row")
                    && stderr.contains("assay trace migrate"),
                "{command:?}: an {form} must be refused with the converter's name ({what})\n{}",
                describe(&out)
            );
            assert_eq!(
                fs::read_to_string(dir.path().join("t.jsonl")).expect("trace"),
                old,
                "{command:?}: {form}: the trace must be byte for byte what it was ({what})"
            );
            assert_eq!(
                fs::read_to_string(dir.path().join("out.jsonl")).expect("target"),
                "previous output\n",
                "{command:?}: {form}: an existing target must be kept ({what})"
            );
            assert!(
                !dir.path().join("new.jsonl").exists() && !dir.path().join("t.tmp.jsonl").exists(),
                "{command:?}: {form}: a refused run must not create a file ({what})"
            );
        }
    }
}

/// The route the refusal names works: `assay trace migrate` converts the older record, and the
/// command enriches the converted episode and leaves the converted trace as it was.
fn a_trace_converted_by_trace_migrate_is_enriched(command: Precompute) {
    let dir = suite(command);
    let (_, old) = OLD_RECORDS[0];
    fs::write(dir.path().join("old.jsonl"), old).expect("old.jsonl");
    let migrate = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir.path())
        .env("ASSAY_EXIT_CODES", "v2")
        .env("NO_COLOR", "1")
        .args([
            "trace",
            "migrate",
            "--input",
            "old.jsonl",
            "--output",
            "converted.jsonl",
        ])
        .output()
        .expect("run assay trace migrate");
    assert_eq!(
        migrate.status.code(),
        Some(0),
        "{command:?}: the converter must accept the older record\n{}",
        describe(&migrate)
    );
    let converted = fs::read(dir.path().join("converted.jsonl")).expect("converted trace");

    let out = assay(
        command,
        dir.path(),
        Path::new("converted.jsonl"),
        Some("out.jsonl"),
    )
    .output()
    .expect("run assay binary");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{command:?}: the converted trace must be read\n{}",
        describe(&out)
    );
    let text = fs::read_to_string(dir.path().join("out.jsonl")).expect("out.jsonl");
    let start: serde_json::Value = text
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("an event"))
        .find(|event| event["type"] == "episode_start")
        .expect("the converted episode's start");
    assert_enriched_as_the_command_enriches(command, &start, "the converted trace");
    if !command.enriches() {
        assert_eq!(
            text.as_bytes(),
            converted,
            "{command:?}: nothing to add, so the output is the converted trace"
        );
    }
    assert_eq!(
        fs::read(dir.path().join("converted.jsonl")).expect("converted trace"),
        converted,
        "{command:?}: the converted trace must be left as it was"
    );
}

// --- A trace read from a named pipe -------------------------------------------------------------

/// How long a command reading from a named pipe gets, and how long the pipe waits for it. A run
/// that works takes well under a second; the rest is room for a slow runner.
const PIPE_LIMITS: ProcessLimits =
    ProcessLimits::new(Duration::from_secs(30), 4 * 1024 * 1024, 1024 * 1024);

/// Offers the trace on a named pipe and runs `reader` on that pipe, bounded on both sides.
///
/// `reader` is given the pipe's path and returns the command to run. The error says which side did
/// not do its part: the reader was stopped at its limit, or it ended without taking the trace.
fn read_from_a_named_pipe(
    dir: &Path,
    limits: ProcessLimits,
    reader: impl FnOnce(&Path) -> Command,
) -> Result<Output, String> {
    let pipe = NamedPipe::offer(dir, TRACE.as_bytes(), limits.timeout)?;
    let ran = run_bounded(reader(pipe.path()), b"", limits, "reader of the named pipe");
    // The reader has ended, by itself or stopped at its limit. Withdrawing the offer now is what
    // keeps a reader that never opened the pipe from holding the test.
    let fed = pipe.finish();
    let output = ran?;
    match fed {
        Fed::Written => Ok(output),
        Fed::NoReader => Err(format!(
            "the reader ended without opening the named pipe\n{}",
            describe(&output)
        )),
        Fed::Failed(why) => Err(format!(
            "the named pipe could not be served: {why}\n{}",
            describe(&output)
        )),
    }
}

/// A trace that is not a file on disk has no file identity to compare. Here it is a named pipe, and
/// the target is an ordinary file of its own: the whole output is, byte for byte, what the same
/// trace gives when it is a file.
fn a_trace_read_from_a_named_pipe_still_reaches_a_separate_target(command: Precompute) {
    let dir = suite(command);
    let from_file = run(command, dir.path(), Some("from-file.jsonl"));
    assert_eq!(
        from_file.status.code(),
        Some(0),
        "{command:?}\n{}",
        describe(&from_file)
    );
    assert_file_holds_the_episode(command, &dir.path().join("from-file.jsonl"));

    let out = read_from_a_named_pipe(dir.path(), PIPE_LIMITS, |pipe| {
        assay(command, dir.path(), pipe, Some("from-pipe.jsonl"))
    })
    .unwrap_or_else(|why| panic!("{command:?}: {why}"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "{command:?}\n{}",
        describe(&out)
    );
    let expected = fs::read(dir.path().join("from-file.jsonl")).expect("from-file.jsonl");
    let written = fs::read(dir.path().join("from-pipe.jsonl")).expect("from-pipe.jsonl");
    assert!(
        written == expected,
        "{command:?}: a trace read from a named pipe must give the output the same trace gives \
         as a file: {} bytes against {}",
        written.len(),
        expected.len()
    );
}

/// A reader that ends without a word.
#[cfg(unix)]
fn a_reader_that_ends_at_once() -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", "exit 0"]);
    command
}

#[cfg(windows)]
fn a_reader_that_ends_at_once() -> Command {
    let mut command = Command::new("cmd");
    command.args(["/C", "exit", "/B", "0"]);
    command
}

/// A reader that never ends.
#[cfg(unix)]
fn a_reader_that_never_ends() -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", "while :; do sleep 1; done"]);
    command
}

#[cfg(windows)]
fn a_reader_that_never_ends() -> Command {
    let mut command = Command::new("ping");
    command.args(["-t", "127.0.0.1"]);
    command
}

/// Nothing holds the pipe any more: no reader on Unix, no pipe by that name on Windows.
#[cfg(unix)]
fn assert_nobody_holds(pipe: &Path) {
    use std::os::unix::fs::OpenOptionsExt;
    let error = fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(pipe)
        .expect_err("a FIFO with no reader refuses a writer that will not wait");
    assert_eq!(error.raw_os_error(), Some(libc::ENXIO), "{error}");
}

#[cfg(windows)]
fn assert_nobody_holds(pipe: &Path) {
    let error = fs::File::open(pipe).expect_err("the pipe must be gone once its server end is");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound, "{error}");
}

/// A reader that ends, successfully, without opening the pipe. Nobody will ever take the trace,
/// and that has to be said at once. Waiting on a writer that waits for a reader held the test for
/// ever.
#[test]
fn a_reader_that_never_opens_the_named_pipe_is_reported_at_once() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut offered = None::<PathBuf>;
    let started = Instant::now();
    let error = read_from_a_named_pipe(dir.path(), PIPE_LIMITS, |pipe| {
        offered = Some(pipe.to_path_buf());
        a_reader_that_ends_at_once()
    })
    .expect_err("nobody read the pipe");
    let took = started.elapsed();

    assert!(
        error.contains("ended without opening the named pipe"),
        "{error}"
    );
    // Far inside the limit the pipe was given: it is the withdrawal that ended the wait.
    assert!(
        took < PIPE_LIMITS.timeout / 3,
        "took {took:?} of a {:?} limit",
        PIPE_LIMITS.timeout
    );
    assert_nobody_holds(&offered.expect("the reader was given the pipe"));
}

/// A reader that never ends is stopped at its limit, and the offer goes with it.
#[test]
fn a_reader_that_never_ends_is_stopped_at_its_limit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let limits = ProcessLimits::new(Duration::from_secs(2), 1024 * 1024, 1024 * 1024);
    let mut offered = None::<PathBuf>;
    let started = Instant::now();
    let error = read_from_a_named_pipe(dir.path(), limits, |pipe| {
        offered = Some(pipe.to_path_buf());
        a_reader_that_never_ends()
    })
    .expect_err("the reader never ended");
    let took = started.elapsed();

    assert!(error.contains("deadline"), "{error}");
    assert!(
        took < limits.timeout + Duration::from_secs(10),
        "took {took:?} of a {:?} limit",
        limits.timeout
    );
    assert_nobody_holds(&offered.expect("the reader was given the pipe"));
}

/// A reader that opens the pipe, takes nothing more and never ends: it is stopped at its limit and
/// lets go of the pipe.
#[cfg(unix)]
#[test]
fn a_reader_that_opens_the_named_pipe_and_never_ends_is_stopped_at_its_limit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let limits = ProcessLimits::new(Duration::from_secs(2), 1024 * 1024, 1024 * 1024);
    let mut offered = None::<PathBuf>;
    let started = Instant::now();
    let error = read_from_a_named_pipe(dir.path(), limits, |pipe| {
        offered = Some(pipe.to_path_buf());
        let mut command = Command::new("sh");
        command
            .args(["-c", r#"exec 3<"$0"; while :; do sleep 1; done"#])
            .arg(pipe);
        command
    })
    .expect_err("the reader never ended");
    let took = started.elapsed();

    assert!(error.contains("deadline"), "{error}");
    assert!(
        took < limits.timeout + Duration::from_secs(10),
        "took {took:?} of a {:?} limit",
        limits.timeout
    );
    assert_nobody_holds(&offered.expect("the reader was given the pipe"));
}

macro_rules! for_both_commands {
    ($($case:ident),* $(,)?) => {
        mod embeddings {
            $( #[test] fn $case() { super::$case(super::Precompute::Embeddings); } )*
        }
        mod judge {
            $( #[test] fn $case() { super::$case(super::Precompute::Judge); } )*
        }
    };
}

for_both_commands!(
    an_output_that_spells_the_trace_differently_is_refused,
    an_output_that_gives_the_trace_as_an_absolute_path_is_refused,
    an_output_hard_linked_to_the_trace_is_refused,
    a_scratch_name_hard_linked_to_the_trace_is_refused,
    enriching_in_place_without_an_output_still_works,
    an_output_spelt_exactly_as_the_trace_still_enriches_in_place,
    a_new_target_still_gets_the_trace_and_the_source_is_untouched,
    an_existing_target_is_still_rewritten_whole,
    a_trace_read_from_a_named_pipe_still_reaches_a_separate_target,
    an_older_trace_record_is_refused_and_both_files_are_kept,
    a_trace_converted_by_trace_migrate_is_enriched,
);

#[cfg(unix)]
mod unix {
    macro_rules! for_both_commands {
        ($($case:ident),* $(,)?) => {
            mod embeddings {
                $( #[test] fn $case() { super::super::$case(super::super::Precompute::Embeddings); } )*
            }
            mod judge {
                $( #[test] fn $case() { super::super::$case(super::super::Precompute::Judge); } )*
            }
        };
    }

    for_both_commands!(
        an_output_symlinked_to_the_trace_is_refused,
        a_scratch_name_symlinked_to_the_trace_is_refused,
        an_inherited_pipe_still_receives_the_trace,
        a_device_is_still_written_to_and_stays_a_device,
    );
}
