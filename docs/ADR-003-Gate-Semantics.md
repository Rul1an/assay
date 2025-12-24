# ADR-003: Gate Semantics and Strict Mode

## Context
A CI gate must provide clear signals: "Block" vs "Inform". Teams have different risk appetites.
Assay introduces `Pass`, `Fail`, `Warn`, and `Flaky` statuses.
- `Fail`: Always blocks (Exit 1).
- `Pass`: Always passes.
- `Warn` / `Flaky`: Ambiguous.

## Decision
We implement a configurable strictness model using a `--strict` flag.

### 1. Status Definitions
| Status | Meaning | Default Behavior | Strict Behavior (`--strict`) |
| :--- | :--- | :--- | :--- |
| **Pass** | All assertions met. | Exit 0 | Exit 0 |
| **Fail** | Assertion failed. | **Exit 1** | **Exit 1** |
| **Error** | Runtime/System error. | **Exit 1** | **Exit 1** |
| **Warn** | Quarantined test failed OR unstable metric. | **Exit 0** (Log) | **Exit 1** |
| **Flaky** | Failed initially, passed on retry. | **Exit 0** (Log) | **Exit 1** |

### 2. CI/CD Integration
- **JUnit**:
  - `Pass` / `Warn` / `Flaky` -> `<testcase>` (Pass).
  - `Warn` / `Flaky` include `<system-out>` with warning details for visibility without failing strict parsers.
  - `Fail` -> `<failure>`.
  - `Error` -> `<error>`.
- **SARIF**:
  - `Fail` -> `error` level.
  - `Warn` / `Flaky` -> `warning` level (always visible as code scanning alert).

### 3. Replay Semantics (Override)
To ensure deterministic gates, when Replay Mode (`--trace-file`) is active:
- `rerun_failures` is **forced to 0**.
- `Flaky` status cannot occur (only Pass/Fail/Warn/Error).
- This override happens *before* policy construction.

## Consequences
- Default mode allows "soft gates" (Warn on regression, but don't break build).
- Strict mode allows "hard gates" (Zero tolerance for instability/quarantine).
