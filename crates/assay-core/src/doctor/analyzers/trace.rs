use crate::errors::diagnostic::{codes, Diagnostic};
use std::io::BufRead;
use std::path::Path;

pub fn analyze_trace_schema(path: &Path, diags: &mut Vec<Diagnostic>) {
    if let Ok(f) = std::fs::File::open(path) {
        let rdr = std::io::BufReader::new(f);
        let mut found_function_calls = 0;
        let mut found_tool_calls = 0;
        let mut line_count = 0;

        for l in rdr.lines().take(500).flatten() {
            line_count += 1;
            // Heuristics:
            if l.contains("\"function_call\"") {
                found_function_calls += 1;
            }
            if l.contains("\"tool_calls\"") || l.contains("\"tool\"") {
                found_tool_calls += 1;
            }
        }

        if found_function_calls > 0 && found_tool_calls == 0 {
            // SOTA: Provide JSON Patch fix if possible (conversion is complex, maybe just hint for now)
            // Or suggest running convert tool?
            diags.push(
                Diagnostic::new(
                    codes::E_TRACE_INVALID,
                    "Trace uses legacy OpenAI 'function_call' format.",
                )
                .with_severity("warn")
                .with_source("doctor.trace_schema")
                .with_context(serde_json::json!({
                    "function_call_count": found_function_calls,
                    "scanned_lines": line_count,
                    "recommendation": "Use 'tool_calls' standard (MCP compatible)."
                }))
                .with_fix_step("Normalize trace keys: replace 'function_call' with 'tool_calls'."),
            );
        }
    }
}
