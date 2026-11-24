use assert_cmd::Command;
use std::fs;
use std::path::Path;
use std::process::Output;
use tempfile::TempDir;

const CONFIG: &str = r#"configVersion: 1
suite: unreadable_event
model: gpt-4o-mini
tests:
- id: t1
  input:
    prompt: calculate 1 + 2
  expected:
    type: must_contain
    must_contain: ["3"]
"#;

const START: &str = r#"{"type":"episode_start","episode_id":"e1","timestamp":1,"input":{"prompt":"calculate 1 + 2"}}"#;
const CALL: &str = r#"{"type":"tool_call","episode_id":"e1","step_id":"s1","timestamp":2,"tool_name":"Calculate","call_index":0,"args":{"operation":"add"}}"#;
const END: &str =
    r#"{"type":"episode_end","episode_id":"e1","timestamp":3,"outcome":"pass","final_output":"3"}"#;

fn commands() -> [(&'static str, Vec<&'static str>); 3] {
    [
        ("validate", vec!["validate"]),
        ("run", vec!["run", "--no-cache", "--db", "eval.db"]),
        ("ci", vec!["ci", "--db", ":memory:"]),
    ]
}

fn evaluate(dir: &Path, args: &[&str], trace: &str) -> Output {
    fs::write(dir.join("eval.yaml"), CONFIG).expect("eval.yaml");
    fs::write(dir.join("trace.jsonl"), trace).expect("trace.jsonl");
    Command::cargo_bin("assay")
        .expect("assay binary")
        .current_dir(dir)
        .env("ASSAY_VCR_MODE", "off")
        .args(args)
        .args(["--config", "eval.yaml", "--trace-file", "trace.jsonl"])
        .output()
        .expect("assay")
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn trace(lines: &[&str]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

#[test]
fn well_formed_traces_load_with_or_without_a_call() {
    for (name, args) in commands() {
        let cases = [
            ("with call", trace(&[START, CALL, END]), 0),
            ("without call", trace(&[START, END]), 0),
        ];
        for (case, body, code) in cases {
            let dir = TempDir::new().expect("tempdir");
            let output = evaluate(dir.path(), &args, &body);
            assert_eq!(
                output.status.code(),
                Some(code),
                "{name} {case}: {}",
                text(&output)
            );
        }
    }
}

#[test]
fn an_unreadable_or_unplaced_call_stops_evaluation_with_its_line() {
    let call = |from: &str, to: &str| {
        assert_eq!(CALL.matches(from).count(), 1, "{from}");
        CALL.replacen(from, to, 1)
    };
    let cases = [
        (
            "float timestamp",
            call(r#""timestamp":2,"#, r#""timestamp":2.5,"#),
        ),
        (
            "string timestamp",
            call(r#""timestamp":2,"#, r#""timestamp":"2","#),
        ),
        (
            "unknown type",
            call(r#""type":"tool_call""#, r#""type":"tool_kall""#),
        ),
        ("missing tool_name", call(r#""tool_name":"Calculate","#, "")),
        (
            "episode not open",
            call(r#""episode_id":"e1""#, r#""episode_id":"e9""#),
        ),
    ];
    for (name, args) in commands() {
        for (case, line) in &cases {
            let dir = TempDir::new().expect("tempdir");
            let output = evaluate(dir.path(), &args, &trace(&[START, line, END]));
            let all = text(&output);
            assert_eq!(output.status.code(), Some(2), "{name} {case}: {all}");
            assert!(all.contains("line 2"), "{name} {case}: {all}");
        }
    }
}

#[test]
fn a_reopened_episode_or_a_shared_prompt_stops_evaluation() {
    let step = r#"{"type":"step","episode_id":"e1","step_id":"s9","timestamp":3,"kind":"model","content":"3"}"#;
    let start_e2 = START.replace(r#""episode_id":"e1""#, r#""episode_id":"e2""#);
    let end_e2 = END.replace(r#""episode_id":"e1""#, r#""episode_id":"e2""#);
    let cases = [
        (
            "reopened episode",
            trace(&[START, CALL, START, END]),
            "line 3",
        ),
        (
            "unfinished episode sharing a prompt",
            trace(&[START, CALL, step, &start_e2, &end_e2]),
            "calculate 1 + 2",
        ),
    ];
    for (name, args) in commands() {
        for (case, body, detail) in &cases {
            let dir = TempDir::new().expect("tempdir");
            let output = evaluate(dir.path(), &args, body);
            let all = text(&output);
            assert_eq!(output.status.code(), Some(2), "{name} {case}: {all}");
            assert!(all.contains(detail), "{name} {case}: {all}");
        }
    }
}

// An episode still open at end of file was skipped without a word when it had no output, even
// when another episode held its prompt. Two episodes with one prompt are refused, open or not.
#[test]
fn an_open_episode_without_output_sharing_a_prompt_stops_evaluation() {
    let start_e2 = START.replace(r#""episode_id":"e1""#, r#""episode_id":"e2""#);
    let end_e2 = END.replace(r#""episode_id":"e1""#, r#""episode_id":"e2""#);
    let body = trace(&[START, CALL, &start_e2, &end_e2]);
    for (name, args) in commands() {
        let dir = TempDir::new().expect("tempdir");
        let output = evaluate(dir.path(), &args, &body);
        let all = text(&output);
        assert_eq!(output.status.code(), Some(2), "{name}: {all}");
        assert!(all.contains("calculate 1 + 2"), "{name}: {all}");
    }
}
