# Assay v1.5.0 Handoff & Architecture Guide

**Version:** v1.5.0
**Date:** 2026-01-07
**Role:** Architecture / Maintainer Guide
**Subject:** Autofix Engine & Agentic Hardening

## 1. Quick Start (30 Seconds)
How to explain this release to users in 4 commands:

```bash
$ assay init --preset default  # Generate secure config
$ assay validate               # Find issues
$ assay fix --dry-run          # Preview fixes
$ assay fix --yes              # Apply fixes
```

## 2. Executive Summary
Assay v1.5.0 transforms the tool from a passive validator to an active **Self-Correcting System**.
-   **Problem**: Developers ignore security linters that just complain.
-   **Solution**: `assay fix` solves the problem interactively.
-   **Core Promise**: "Safe by default." We use Atomic I/O to guarantee data integrity and Embedded Packs to guarantee availability.

## 3. Architecture & Terminology
*> **Naming Note**: The internal module `crates/assay-core/src/agentic` handles the autofix logic. In v1.6+, this will be renamed to `autofix` or `repair` to avoid confusion with the "Agentic AI systems" that Assay protects.*

### Core Libraries
-   **`assay-core`**: The brain. Pure Rust.
    -   `fix/mod.rs`: **The Patch Engine**. Handles JSON Patch application.
        -   **Key Mechanic**: Atomic File Writes (`tempfile` -> rename) ensure zero corruption on crash.
    -   `agentic/mod.rs`: **The Strategist**. Maps `Diagnostic` -> `SuggestedPatch`. Deterministic and stateless.
-   **`assay-cli`**: The interface.
    -   `cli/commands/fix.rs`: The orchestration loop. Runs `validate` -> `build_suggestions` -> `filter` -> `apply`.
    -   `packs/`: Embedded YAML assets. Zero runtime dependencies.

## 4. Security Capabilities (The "Why")

### Policy Presets (`assay init --preset`)
These packs are designed to map to common threat vectors (OWASP for LLMs).

#### `default` (Balanced)
The standard for most Agent deployments.
-   **Blocks**: `exec`, `shell`, `spawn`, `bash`, `cmd`, `powershell` (RCE Prevention).
-   **Restricts**: File operations limited to `/app/**` and `/data/**` (Path Traversal Prevention).
-   **Warns**: Tools with descriptions > 500 chars (Prompt Injection Heuristic).

#### `hardened` (High Security)
Financial/Healthcare grade.
-   **Allowlist Only**: Implicit deny for *any* tool not explicitly listed.
-   **Strict Regex**: Arguments must match precise patterns (e.g. `^/app/data/uploads/[a-z0-9]+\.pdf$`).

#### `dev` (Permissive)
-   **Warns Only**: Logs violations but allows execution.
-   **Use Case**: Local prototyping where friction must be zero.

## 5. Operational Safety (The "How")

### Atomic Integrity
We *never* modify a file in place directly.
1.  Read Content.
2.  Apply Patch in Memory (Verify success).
3.  Write to `.assay_fix_tmp`.
4.  Sync to Disk.
5.  OS Rename (Atomic Replace).
6.  *Windows Safe*: Handles file locking semantics correctly.

### Rollback Strategy
v1.5.0 relies on **Git** for rollbacks.
*   **Recommendation**: Users should run `assay fix` only on a clean git index.
*   **Safety Net**: `assay fix --dry-run` shows a Unified Diff before touching disk.

## 6. CI/CD Integration

### Exit Codes
Standard linter contract:
-   `0`: **Clean** (No issues found).
-   `1`: **Issues Found** (Policy violations). Fixable via `assay fix`.
-   `2`: **Error** (Config missing, Schema invalid, IO error). Manual intervention required.

### Output Formats

#### SARIF (`--format sarif`)
Native GitHub Security tab integration (`code-scanning`).

```yaml
# .github/workflows/assay.yml
- run: assay validate --format sarif > results.sarif
- uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: results.sarif
```

#### JSON (`--format json`)
Strict schema for Agentic parsing and automated self-healing loops.

## 7. Test Philosophy & Maintenance
We do not just "run tests"; we verify contracts.

-   **Path Logic**: Verified against RFC 6901 (JSON Pointer) compliance (escape order `~1` before `~0`).
-   **Strict Traversal**: Verified that `remove`/`replace` ops fail *cleanly* (no partial mutation) if a path doesn't exist.
-   **Determinism**: `build_suggestions` is tested to produce identical patches for identical errors, ensuring stable autofix loops.

### Maintenance Targets
1.  **`agentic/mod.rs`**: Add new `Diagnostic` -> `Patch` mappings here when adding new validation rules.
2.  **`fix/mod.rs`**: Touch with extreme caution. This is the I/O kernel.
