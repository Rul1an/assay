## Why

This PR lands the P0 “must-have” DX fixes from the DX Implementation Plan. It removes early adopter footguns by:
*   making CI bootstrap canonical and consistent,
*   standardizing exit codes + reason codes for reliable automation,
*   ensuring SARIF is always uploadable to GitHub Code Scanning.

## What changed

### 🚀 E1 — Blessed Init & CI On-Ramp
*   **Canonical workflow template:** `assay init --ci` now generates a workflow using `Rul1an/assay/assay-action@v2`.
*   **Supply Chain Security:** Template includes advice to pin to a **full commit SHA** for maximum security (e.g., `uses: ...@<sha>`).
*   **Secure defaults:** Template includes job-scoped minimal permissions:
    ```yaml
    permissions:
      contents: read
      security-events: write # Required for SARIF upload
    ```
    *Note: On Fork PRs, `security-events: write` may drop to read-only; the job summary remains the fallback feedback mechanism.*
*   **Docs aligned:** CI integration + user guide updated to recommend the blessed `init --ci` flow.

### 🚥 E3 — Exit Codes & Reason Code Registry
*   **Architecture:**
    *   `assay-cli`: Defines the canonical `ReasonCode` validation domain and maps to exit codes.
    *   `assay-core`: Provides test result models (future home of ReasonCode).
*   **Strict V2 mapping (default):**
    *   `0` = Success
    *   `1` = Test failure (e.g., `TestFailed` / `PolicyViolation`)
    *   `2` = User/config error (e.g., `InvalidConfig` / `TraceNotFound`)
    *   `3` = Infra/provider error (e.g., Timeout / RateLimit / Judge unavailable)
*   **Compatibility:**
    *   `--exit-codes v1|v2` or `ASSAY_EXIT_CODES=v1|v2`.
    *   V1 mode preserves legacy behavior (e.g., TraceNotFound=3) but is strongly discouraged for new automation.

### 📢 E2 — PR Feedback UX (SARIF + JUnit)
*   **SARIF hard invariant:** every SARIF result has ≥1 location.
    *   Fallback: When no physical source exists, we map to a **synthetic location**: `.assay/eval.yaml:1:1`.
    *   *Result:* Blocking errors reliably appear in **GitHub Code Scanning alerts** and **PR check annotations** instead of silently failing ingestion.
*   **JUnit output:** Action now defaults to producing `.assay/reports/junit.xml` for standard CI test widgets.

## Verification

| Contract test | Invariant |
| :--- | :--- |
| `contract_init_ci` | Workflow uses `@v2`, correct permissions (`security-events: write`), and SHA pinning advice. |
| `contract_exit_codes` | V1/V2 mapping logic holds; CLI/Env switches work as expected. |
| `contract_sarif` | SARIF output guaranteed to have valid `locations` (real or synthetic). |

*All contract tests are now part of the standard `cargo test` suite and run on every CI gate.*

## Notes for reviewers
*   **Recovery PR:** Focuses on stabilizing the contract (Init, Exit Codes, SARIF) for future P1 features.
*   **Safe Rollback:** No localized database schema changes; reverting simply restores V1 exit code semantics.

---

## Release Notes
*   **New "Blessed" CI Setup:** `assay init --ci` is the secure standard. We recommend pinning the generated action to a commit SHA.
*   **Standardized Exit Codes:** Assay now strictly follows V2 semantics (0=Ok, 1=Fail, 2=Config, 3=Infra).
*   **Reliable PR Feedback:** SARIF reports now guarantee upload success, ensuring even "sourceless" config errors are visible in PR checks.

## Risk & Compatibility
*   **Compatibility:** V2 Exit Codes are now default.
    *   *Risk:* Pipelines expecting `TraceNotFound` to return 3 will now see 2.
    *   *Mitigation:* Use `ASSAY_EXIT_CODES=v1` to restore legacy behavior.
*   **Deprecation:** V1 exit codes are considered legacy. Users should migrate automation to key off `reason_code` (JSON) or standard V2 exit codes.
*   **Fork Support:** SARIF upload requires write permissions. External contributors will rely on the Action Job Summary.
