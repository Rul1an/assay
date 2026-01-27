# ADR-007: Evidence DX Tooling (Lint, Diff, Explore)

## Status
**Adopted** (Jan 2026)

## Context
As `assay-evidence` matures (ADR-006), developers and CI systems need ergonomic tools to interact with evidence bundles. Raw JSON/NDJSON is machine-readable but not human-friendly or CI-native. We need a standard set of tools for **Linting** (policy checks), **Diffing** (regression detection), and **Exploring** (debugging) evidence.

## Decision
We will implement three core DX primitives in `assay-cli` and `assay-evidence`:

### 1. `assay evidence lint`: SARIF & JSON Output
To integrate with modern CI (GitHub Code Scanning, GitLab), we will treat Evidence Linting as a first-class citizen.

*   **Format**: Support `--format sarif` (SARIF 2.1.0) and `--format json`.
*   **SARIF Policy**:
    *   **Single Run**: One `run` object per SARIF file (GitHub limitation/best practice).
    *   **Stable Rule IDs**: `ASSAY-E001` (Error), `ASSAY-W001` (Warning).
    *   **Fingerprinting**: Use `partialFingerprints` with privacy-safe hashes (e.g., `sha256(rule_id + run_id + event_seq + startLine)`) to allow baseline matching without exposing sensitive payload data.
*   **Exit Codes**:
    *   `0`: Success (or violations below threshold).
    *   `1`: Violations found (above threshold).
    *   `2`: Resource limit exceeded.
    *   `3`: I/O or Verification error (Input untrusted).

### 2. `assay evidence diff`: Verified-Only Comparison
To detect regressions between runs (e.g., "Why did this run take 30s longer?"), we introduce semantic diffing.

*   **Security Invariant**: **Verify First**. Both `baseline` and `candidate` bundles MUST pass `verify_bundle_with_limits()` before diffing begins. We never diff untrusted input.
*   **Baseline Management**: Support explicit baseline files (`--baseline base.tar.gz`) and "Baseline Pointers" JSON (`--baseline-from pointer.json`) for CI stability.
*   **Semantic Diff**: Compare extracted sets (Hosts, File Accesses, Event Counts) rather than raw bytes. Use scrubbed subjects to ensure privacy.

### 3. `assay evidence explore`: Secure TUI
To allow rapid local debugging without unzipping, we introduce a TUI explorer based on `ratatui`.

*   **Security (Terminal Injection)**:
    *   **Input Gate**: Verify bundle before render.
    *   **Sanitization**: Strict stripping of ANSI escape codes (`\x1b`), control characters, and OSC sequences from all displayed text.
    *   **Resource Bounds**: Cap max events loaded (e.g., 200k), max subject length, and max payload preview size to prevent TUI DoS.
*   **ReadOnly**: The TUI is strictly a viewer; it does not modify evidence.

## Consequences
*   **CI Native**: Users can see assay violations directly in GitHub PRs via SARIF.
*   **Safe Comparison**: Diffing is protected against malicious payloads by the verification gate.
*   **Secure Debugging**: TUI provides a safe way to inspect untrusted bundles without risk of terminal hijacking.
