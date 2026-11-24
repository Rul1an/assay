use assert_cmd::Command;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const CONFIG: &str = r#"configVersion: 1
suite: trace_cache_identity
model: gpt-4o-mini
tests:
- id: t1
  input:
    prompt: calculate 1 + 2
  expected:
    type: must_contain
    must_contain: ["3"]
"#;

fn trace(operation: &str) -> String {
    format!(
        concat!(
            r#"{{"type":"episode_start","episode_id":"e1","timestamp":1,"input":{{"prompt":"calculate 1 + 2"}}}}"#,
            "\n",
            r#"{{"type":"tool_call","episode_id":"e1","step_id":"s1","timestamp":2,"tool_name":"Calculate","call_index":0,"args":{{"operation":"{}"}}}}"#,
            "\n",
            r#"{{"type":"episode_end","episode_id":"e1","timestamp":3,"outcome":"pass","final_output":"3"}}"#,
            "\n"
        ),
        operation
    )
}

/// Runs one evaluation in `dir`, keeping its `eval.db` between calls.
fn evaluate(dir: &Path, args: &[&str], operation: &str) -> (bool, bool) {
    fs::write(dir.join("eval.yaml"), CONFIG).expect("eval.yaml");
    fs::write(dir.join("trace.jsonl"), trace(operation)).expect("trace.jsonl");
    let output = Command::cargo_bin("assay")
        .expect("assay binary")
        .current_dir(dir)
        .env("ASSAY_VCR_MODE", "off")
        .args(args)
        .args([
            "--config",
            "eval.yaml",
            "--trace-file",
            "trace.jsonl",
            "--db",
            "eval.db",
        ])
        .output()
        .expect("assay");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cache_hit = String::from_utf8_lossy(&output.stderr).contains("[CACHE HIT]");
    let store = assay_core::storage::Store::open(&dir.join("eval.db")).unwrap();
    let run_id = store
        .get_latest_run_id("trace_cache_identity")
        .unwrap()
        .unwrap();
    let rows = store.fetch_results_for_run(run_id).unwrap();
    assert_eq!(rows.len(), 1);
    (
        cache_hit,
        rows[0].skip_reason.as_deref() == Some("fingerprint_match"),
    )
}

#[test]
fn unchanged_calls_reuse_results_and_changed_calls_are_evaluated() {
    for command in ["run", "ci"] {
        for incremental in [false, true] {
            let mut args = vec![command];
            if incremental {
                args.push("--incremental");
            }
            let dir = TempDir::new().unwrap();
            let evaluate_call = |operation| evaluate(dir.path(), &args, operation);
            assert_eq!(evaluate_call("add"), (false, false));
            assert_eq!(evaluate_call("add"), (!incremental, incremental));
            assert_eq!(evaluate_call("subtract"), (false, false));
        }
    }
}
