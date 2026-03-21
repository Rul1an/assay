# Wave C2 Checklist

- [x] New pack name is `owasp-agentic-control-evidence-baseline`
- [x] Open pack and built-in mirror are exact YAML twins
- [x] Built-in registration added in `crates/assay-evidence/src/lint/packs/mod.rs`
- [x] Shipped rule set is exactly `A1-002`, `A3-001`, `A5-001`
- [x] No `conditional`, `engine_min_version`, or unsupported checks are shipped
- [x] README contains explicit non-goals
- [x] README includes the process-exec non-goal guardrail
- [x] Tests cover exact equivalence, wording, and pass/fail behavior
- [x] Reviewer gate enforces no-overclaim and no-drift constraints
