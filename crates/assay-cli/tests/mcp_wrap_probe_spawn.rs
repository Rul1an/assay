//! A client may probe a stdio server on a disposable process before opening the session, as the
//! MCP TypeScript client's version negotiation does, and may end that process with a signal. Such
//! a run must not consume this wrap's one-run output paths, and must not leave anything that reads
//! as a session.
#![cfg(unix)]

use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::process::ExitStatusExt,
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Output, Stdio},
    time::Duration,
};
use tempfile::TempDir;

const SERVER: &str = "import sys,json\nfor line in sys.stdin:\n r=json.loads(line)\n if 'id' in r: print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':{'resultType':'complete'}}),flush=True)\n";
/// Records each frame that reaches it, with whether the context was already written.
const RECORDING_SERVER: &str = "import sys,json,os\nfor line in sys.stdin:\n r=json.loads(line)\n open('seen','a').write(r['method']+' '+str(os.path.exists('context') and os.path.getsize('context')>0)+'\\n')\n print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':{'resultType':'complete'}}),flush=True)\n";
/// Takes a path this wrap will want, once the probe has shown the run is live.
const PLANTING_SERVER: &str = "import sys,json\nfor line in sys.stdin:\n r=json.loads(line)\n open('seen','a').write(r['method']+'\\n')\n if r['method']=='server/discover': open('decisions','x').write('foreign')\n print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':{'resultType':'complete'}}),flush=True)\n";
const DISCOVER: &str =
    "{\"jsonrpc\":\"2.0\",\"id\":\"probe-1\",\"method\":\"server/discover\",\"params\":{}}\n";
const LIST: &str = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\",\"params\":{}}\n";
const CALL: &str = "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"echo\",\"arguments\":{}}}\n";
const OUTPUTS: [&str; 4] = ["decisions", "receipts", "context", "stop"];

fn command(dir: &Path, server: &str, extra: &[&str]) -> Command {
    fs::write(
        dir.join("policy.yaml"),
        "version: '2.0'\nname: probe-spawn-test\ntools:\n  allow: [echo]\n",
    )
    .unwrap();
    fs::write(dir.join("server.py"), server).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
    command
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args([
            "mcp",
            "wrap",
            "--policy",
            "policy.yaml",
            "--event-source",
            "assay://probe-spawn-test",
            "--decision-log",
            "decisions",
            "--decision-log-new",
            "--request-receipts-out",
            "receipts",
            "--session-context-out",
            "context",
            "--session-stop-out",
            "stop",
        ])
        .args(extra)
        .args(["--", "python3", "-I", "server.py"]);
    command
}

/// Spawn a wrap that cannot outlive the test: a blocked read on its stdout fails rather than hangs.
fn spawn_bounded(mut command: Command) -> Child {
    let child = command.spawn().unwrap();
    let pid = nix::unistd::Pid::from_raw(child.id() as i32);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(30));
        let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL);
    });
    child
}

fn run_with(dir: &Path, server: &str, input: &str, extra: &[&str]) -> Output {
    let mut child = spawn_bounded(command(dir, server, extra));
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn run(dir: &Path, server: &str, input: &str) -> Output {
    run_with(dir, server, input, &[])
}

/// A wrap the test talks to one frame at a time.
struct Live {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Live {
    fn start(dir: &Path, server: &str) -> Self {
        let mut child = spawn_bounded(command(dir, server, &[]));
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn exchange(&mut self, frame: &str) -> String {
        self.stdin.write_all(frame.as_bytes()).unwrap();
        self.stdin.flush().unwrap();
        let mut reply = String::new();
        self.stdout.read_line(&mut reply).unwrap();
        reply
    }

    fn signal(mut self, signal: nix::sys::signal::Signal) -> i32 {
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(self.child.id() as i32), signal).unwrap();
        let status = self.child.wait().unwrap();
        status.signal().expect("ended by the signal")
    }
}

fn present(dir: &Path) -> Vec<&'static str> {
    OUTPUTS
        .into_iter()
        .filter(|name| dir.join(name).exists())
        .collect()
}

fn assert_session_gets_the_paths(dir: &Path) {
    let session = run(dir, SERVER, &format!("{DISCOVER}{LIST}{CALL}"));
    assert!(session.status.success(), "{session:?}");
    assert_eq!(present(dir), OUTPUTS);
    let decision: Value =
        serde_json::from_str(fs::read_to_string(dir.join("decisions")).unwrap().trim()).unwrap();
    assert_eq!(decision["data"]["tool"], "echo");
    for name in ["receipts", "stop", "context"] {
        assert!(!fs::read(dir.join(name)).unwrap().is_empty(), "{name}");
    }
}

#[test]
fn discover_only_run_leaves_no_outputs_and_the_session_gets_the_paths() {
    let d = TempDir::new().unwrap();
    let probe = run(d.path(), SERVER, DISCOVER);
    assert!(probe.status.success(), "{probe:?}");
    assert!(String::from_utf8_lossy(&probe.stdout).contains("probe-1"));
    assert!(
        String::from_utf8_lossy(&probe.stderr).contains("probe-only run"),
        "{probe:?}"
    );
    assert_eq!(present(d.path()), Vec::<&str>::new());
    assert_session_gets_the_paths(d.path());
}

/// A client may reap the probe with a signal at once, so the run gets no chance to clean up.
#[test]
fn a_signal_after_the_probe_leaves_no_outputs_and_keeps_its_exit_status() {
    use nix::sys::signal::Signal::{SIGKILL, SIGTERM};
    for signal in [SIGTERM, SIGKILL] {
        let d = TempDir::new().unwrap();
        let mut probe = Live::start(d.path(), SERVER);
        assert!(probe.exchange(DISCOVER).contains("probe-1"));
        assert_eq!(present(d.path()), Vec::<&str>::new(), "{signal}");
        assert_eq!(probe.signal(signal), signal as i32);
        assert_eq!(present(d.path()), Vec::<&str>::new(), "{signal}");
        assert_session_gets_the_paths(d.path());
    }
}

/// Assay installs no signal handler: a session keeps what it wrote and its closure stays unknown.
#[test]
fn a_signal_during_a_session_leaves_its_outputs_and_no_stop_record() {
    use nix::sys::signal::Signal::SIGTERM;
    let d = TempDir::new().unwrap();
    let mut session = Live::start(d.path(), SERVER);
    assert!(session.exchange(CALL).contains("\"id\": 2"));
    assert_eq!(present(d.path()), OUTPUTS);
    assert_eq!(session.signal(SIGTERM), SIGTERM as i32);
    assert_eq!(present(d.path()), OUTPUTS);
    assert!(fs::read(d.path().join("stop")).unwrap().is_empty());
    assert!(!fs::read(d.path().join("context")).unwrap().is_empty());
    let refused = run(d.path(), SERVER, LIST);
    assert!(!refused.status.success(), "{refused:?}");
}

/// A run with anything beyond discover is a session, including one that made no decision.
#[test]
fn any_other_traffic_creates_every_output() {
    for input in [
        String::new(),
        LIST.to_string(),
        format!("{DISCOVER}{LIST}"),
        format!("{LIST}{DISCOVER}"),
        format!("{DISCOVER}not json\n"),
        // No id: a notification cannot be the request a probe sends and awaits.
        "{\"jsonrpc\":\"2.0\",\"method\":\"server/discover\",\"params\":{}}\n".to_string(),
    ] {
        let d = TempDir::new().unwrap();
        let output = run(d.path(), SERVER, &input);
        assert!(output.status.success(), "{input}: {output:?}");
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("probe-only run"),
            "{input}"
        );
        assert_eq!(present(d.path()), OUTPUTS, "{input}");
        assert!(
            !fs::read(d.path().join("stop")).unwrap().is_empty(),
            "{input}"
        );
    }
}

/// The outputs exist, and the context is written, before the first session frame is forwarded.
#[test]
fn outputs_are_created_before_the_first_other_frame_is_forwarded() {
    let d = TempDir::new().unwrap();
    let mut live = Live::start(d.path(), RECORDING_SERVER);
    assert!(live.exchange(DISCOVER).contains("probe-1"));
    assert!(live.exchange(LIST).contains("\"id\": 1"));
    let Live {
        mut child, stdin, ..
    } = live;
    drop(stdin);
    assert!(child.wait().unwrap().success());
    assert_eq!(
        fs::read_to_string(d.path().join("seen")).unwrap(),
        "server/discover False\ntools/list True\n"
    );
}

/// A server can take a path during the window. The run refuses, overwrites nothing, and stops
/// before the frame that needed the output.
#[test]
fn a_path_taken_after_spawn_stops_the_run_before_forwarding() {
    let d = TempDir::new().unwrap();
    let mut live = Live::start(d.path(), PLANTING_SERVER);
    assert!(live.exchange(DISCOVER).contains("probe-1"));
    let reply = live.exchange(CALL);
    assert_eq!(reply, "", "no reply for a frame that was not forwarded");
    let status = live.child.wait().unwrap();
    assert!(!status.success(), "{status:?}");
    assert_eq!(
        fs::read_to_string(d.path().join("decisions")).unwrap(),
        "foreign"
    );
    assert_eq!(
        fs::read_to_string(d.path().join("seen")).unwrap(),
        "server/discover\n"
    );
}

/// Nothing is removed once the server runs, not even after a clean probe.
#[test]
fn a_file_the_run_did_not_create_survives_a_probe() {
    let d = TempDir::new().unwrap();
    let plant = format!("open('decisions','x').write('foreign')\n{SERVER}");
    let probe = run(d.path(), &plant, DISCOVER);
    assert!(probe.status.success(), "{probe:?}");
    assert_eq!(
        fs::read_to_string(d.path().join("decisions")).unwrap(),
        "foreign"
    );
    assert_eq!(present(d.path()), ["decisions"]);
}

/// A refused path stops the start before spawn, and the start consumes none of the other paths.
#[test]
fn a_refused_path_stops_before_spawn_and_leaves_the_others_free() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("receipts"), "earlier").unwrap();
    let touch = format!("open('spawned','w')\n{SERVER}");
    let refused = run(d.path(), &touch, "");
    assert!(!refused.status.success(), "{refused:?}");
    assert!(!d.path().join("spawned").exists());
    assert_eq!(present(d.path()), ["receipts"]);
    assert_eq!(
        fs::read_to_string(d.path().join("receipts")).unwrap(),
        "earlier"
    );
}

/// A server built on an SDK that exits on any request before `initialize` ends the probe itself.
#[test]
fn a_server_that_exits_on_the_probe_leaves_no_outputs() {
    let d = TempDir::new().unwrap();
    let probe = run(
        d.path(),
        "import sys\nsys.stdin.readline()\nsys.exit(1)\n",
        DISCOVER,
    );
    assert_eq!(probe.status.code(), Some(1), "{probe:?}");
    assert_eq!(present(d.path()), Vec::<&str>::new());
    assert_session_gets_the_paths(d.path());
}

/// A probe is not a session, so nothing reports on it.
#[test]
fn a_probe_writes_no_coverage_or_state_window() {
    let d = TempDir::new().unwrap();
    let probe = run_with(
        d.path(),
        SERVER,
        DISCOVER,
        &[
            "--coverage-out",
            "coverage.json",
            "--state-window-out",
            "state.json",
        ],
    );
    assert!(probe.status.success(), "{probe:?}");
    assert_eq!(present(d.path()), Vec::<&str>::new());
    assert!(!d.path().join("coverage.json").exists());
    assert!(!d.path().join("state.json").exists());
}
