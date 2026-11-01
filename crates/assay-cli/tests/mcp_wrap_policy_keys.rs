//! A policy key `assay mcp wrap` does not act on must say so. Each case runs the command and reads
//! its stderr, beside what reached the server.

use assert_cmd::Command;
use std::{fs, path::Path, time::Duration};
use tempfile::TempDir;

const SERVER: &str = "import sys,json\nfor line in sys.stdin:\n open('seen','a',newline='').write(line)\n r=json.loads(line)\n if 'id' in r: print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':{'content':[]}}),flush=True)\n";
const CALL: &str =
    "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"deploy\",\"arguments\":{}}}\n";
const BASE: &str = "version: '2.0'\nname: keys\ntools:\n  allow: [deploy]\n";

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

struct Wrapped {
    stdout: String,
    stderr: String,
    seen: String,
}

fn wrap(policy: &str) -> Wrapped {
    let dir = TempDir::new().unwrap();
    let root: &Path = dir.path();
    fs::write(root.join("policy.yaml"), policy).unwrap();
    fs::write(root.join("server.py"), SERVER).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(root)
        .timeout(Duration::from_secs(20))
        .args(["mcp", "wrap", "--policy", "policy.yaml"])
        .args(["--", python(), "-I", "server.py"])
        .write_stdin(CALL)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    Wrapped {
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
        seen: fs::read_to_string(root.join("seen")).unwrap_or_default(),
    }
}

fn validate(policy: &str) -> (i32, String) {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("policy.yaml"), policy).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir.path())
        .timeout(Duration::from_secs(20))
        .args(["policy", "validate", "--input", "policy.yaml"])
        .output()
        .unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

const INERT: &[(&str, &str, &str)] = &[
    (
        "signatures",
        "signatures:\n  check_descriptions: true\n",
        "nothing reads it",
    ),
    (
        "discovery",
        "discovery:\n  enabled: true\n",
        "assay discover reads it",
    ),
    (
        "runtime_monitor",
        "runtime_monitor:\n  enabled: true\n",
        "assay monitor reads it",
    ),
    (
        "kill_switch",
        "kill_switch:\n  enabled: true\n",
        "assay monitor reads it",
    ),
];

#[test]
fn a_key_wrap_does_not_act_on_is_named_and_changes_nothing() {
    let control = wrap(BASE);
    assert_eq!(control.seen, CALL);
    for (key, yaml, reader) in INERT {
        let run = wrap(&format!("{BASE}{yaml}"));
        let note = run
            .stderr
            .lines()
            .find(|line| line.contains(&format!("{key:?}")))
            .unwrap_or_else(|| panic!("{key} not named: {}", run.stderr));
        assert!(note.contains(reader), "{note}");
        assert_eq!(run.seen, control.seen, "{key} changed what was forwarded");
        assert_eq!(run.stdout, control.stdout, "{key} changed the reply");
    }
}

#[test]
fn keys_that_are_not_policy_keys_are_named() {
    for (yaml, named) in [
        ("sequences: [a, b]\n", "\"sequences\""),
        ("rules: []\n", "\"rules\""),
        ("tool_pin:\n  deploy: {}\n", "\"tool_pin\""),
    ] {
        let run = wrap(&format!("{BASE}{yaml}"));
        assert!(run.stderr.contains(named), "{named}: {}", run.stderr);
    }
    let run = wrap("version: '2.0'\nname: keys\ntools:\n  allow: [deploy]\n  denny: [deploy]\n");
    assert!(run.stderr.contains("\"tools.denny\""), "{}", run.stderr);
    let run = wrap(&format!("{BASE}limits:\n  max_calls: 1\n"));
    assert!(
        run.stderr.contains("\"limits.max_calls\""),
        "{}",
        run.stderr
    );
}

#[test]
fn keys_wrap_acts_on_are_not_named() {
    let run = wrap(concat!(
        "version: '2.0'\nname: keys\n",
        "tools:\n  allow: [deploy]\n  deny: [other]\n  approval_required: []\n",
        "schemas:\n  deploy: {type: object}\n",
        "enforcement:\n  unconstrained_tools: warn\n",
        "limits:\n  max_requests_total: 5\n  max_tool_calls_total: 5\n",
        "tool_classes:\n  deploy: [write]\n",
    ));
    assert!(!run.stderr.contains("WARNING"), "{}", run.stderr);
    assert_eq!(run.seen, CALL);
}

#[test]
fn policy_validate_names_them_and_still_succeeds() {
    for (key, yaml, _) in INERT {
        let (code, stderr) = validate(&format!("{BASE}{yaml}"));
        assert_eq!(code, 0, "{stderr}");
        assert!(stderr.contains(&format!("{key:?}")), "{key}: {stderr}");
        assert!(stderr.contains("Policy OK"), "{stderr}");
    }
    let (code, stderr) = validate(BASE);
    assert_eq!(code, 0, "{stderr}");
    assert!(!stderr.contains("WARNING"), "{stderr}");
}

/// What `assay init` writes must not carry a key that does nothing.
#[test]
fn every_init_mode_writes_a_policy_wrap_acts_on() {
    for mode in [
        &[][..],
        &["--preset", "default"][..],
        &["--preset", "hardened"][..],
        &["--preset", "dev"][..],
        &["--hello-trace"][..],
    ] {
        let dir = TempDir::new().unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_assay"))
            .current_dir(dir.path())
            .timeout(Duration::from_secs(20))
            .arg("init")
            .args(mode)
            .output()
            .unwrap();
        assert!(output.status.success(), "{mode:?}: {output:?}");
        let mut policies = 0;
        for entry in fs::read_dir(dir.path()).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if name.contains("policy") {
                policies += 1;
                let policy = fs::read_to_string(&path).unwrap();
                assert!(!policy.contains("signatures"), "{mode:?} {name}: {policy}");
                let (code, stderr) = validate(&policy);
                assert_eq!(code, 0, "{mode:?} {name}: {stderr}");
                assert!(!stderr.contains("WARNING"), "{mode:?} {name}: {stderr}");
            }
        }
        assert!(policies > 0, "{mode:?} wrote no policy file");
    }
}
