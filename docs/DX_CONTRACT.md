# Developer Experience (DX) Contract & Release Gates

All Verdict integration features (Importers, CI adapters) must pass this contract before merging. This ensures a "Zero-Footgun" experience for users.

## 1. The Core Principles
1.  **Zero-Footgun**: Users cannot corrupt state by following standard docs.
2.  **Trace = Truth**: The Trace file is the authoritative source; the DB is a disposable index.
3.  **CI-First**: Defaults assume an ephemeral, stateless environment.
4.  **No Magic**: Implicit actions (like auto-ingest) must always be logged.

## 2. PR Checklist for Contributors

### Interaction Design
- [ ] **Idempotency**: Can I run this command 2x against the same DB without error?
    - *Implementation*: Use `ON CONFLICT DO UPDATE` or `DO NOTHING`.
- [ ] **Ephemeral Support**: Does this work with `--db :memory:` or a temp file?
- [ ] **Feedback**: Does the command print exactly *one* useful log line per major action (e.g., ingest)?
    - *Bad*: Silent execution.
    - *Bad*: Spamming 1000 lines of debug logs.

### Determinism
- [ ] **Output Stability**: Given the same input, is the output byte-stable (sorted keys, sorted events)?
- [ ] **Time Handling**: Are timestamps parsed strictly or normalized?

## 3. Mandatory Test Matrix
Every new integration must include a regression script (e.g., `tests/e2e/my_feature.sh`) covering:

| Scenario | Requirement | Test |
| :--- | :--- | :--- |
| **Re-Run (Idempotency)** | Run command 2x. | Expect: Exit 0, No "Unique Constraint" errors. |
| **Ephemeral DB** | Run with `--db :memory:`. | Expect: Success (Auto-ingest works). |
| **Missing ID** | Trace has diff ID than Config. | Expect: Fallback to Prompt Match (Success) or specific Error. |
| **Content Mismatch** | Trace prompt != Config prompt. | Expect: Failure (`E_TRACE_EPISODE_MISSING` / Mismatch). |

## 4. Documentation Standard
- [ ] **Usage**: Docs show a maximum of 2 commands for the standard flow (Import -> CI).
- [ ] **Defaults**: Documentation explicitly recommends ephemeral DBs (e.g., `rm -f db` or `:memory:`).

## 5. Troubleshooting Guide (Standard Errors)
- `E_TRACE_EPISODE_MISSING`: "We couldn't match your config to the trace. Check `test_id` or `prompt`."

