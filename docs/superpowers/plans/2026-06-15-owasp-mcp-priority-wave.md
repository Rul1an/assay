# OWASP MCP Priority Wave Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the MCP01/MCP04/MCP09 Partial coverage rows into bounded, evidence-backed experiments and then productizable Assay/Plimsoll capabilities, starting with MCP09 inventory coverage.

**Architecture:** Build evidence carriers and fixtures before gates. Each experiment separates observation from approval, reports scanner coverage honestly, and never upgrades incomplete evidence to clean. Plimsoll/report consumers should read source artifacts as truth and produce bounded findings, not broad OWASP compliance claims.

**Tech Stack:** Rust (`assay-mcp-server` / CLI), JSON fixtures, Markdown security docs, shell smoke tests, optional Python fixture generators where existing examples already use Python.

---

## Priority Order

1. **M1 — MCP inventory coverage / shadow-server zoo**: establish the environment boundary for MCP09.
2. **M3 — MCP secret sink and credential-boundary corpus**: protect new evidence/report sinks before expanding them.
3. **M2 — Declared-vs-observed MCP server admission**: compare inventory/runtime observations with declared admissions.
4. **M4 — MCP supply-chain drift zoo**: classify package/source/manifest drift without maliciousness claims.
5. **M5 — Cross-risk chain**: compose shadow server, secret exposure, and missing audit evidence.
6. **M6 — OWASP MCP coverage report projection**: render scoped coverage from source artifact digests.

## Shared Claim Rules

- Observed is not approved.
- Admitted is not safe.
- Digest match is not benign behavior.
- Digest drift and unsigned source are not maliciousness findings.
- Not observed is not absent unless scanner coverage is complete.
- Coverage incomplete is warning/inconclusive/pending, never clean.
- Redacted means safe for the sink; it does not mean removed from the source system unless the source contract says so.

## File Structure

- `docs/security/OWASP-MCP-TOP10-MAPPING.md`: public coverage taxonomy and current bounded claims.
- `docs/security/OWASP-MCP-TOP10-TEST-MAP.md`: representative tests and the M1-M6 experiment backlog.
- `docs/superpowers/plans/2026-06-15-owasp-mcp-priority-wave.md`: this implementation plan.
- `crates/assay-mcp-server/src/mcp_inventory.rs`: future M1 producer module for `assay.mcp_server_inventory.v0`.
- `crates/assay-mcp-server/tests/fixtures/mcp_inventory/`: future M1 fixture corpus.
- `crates/assay-mcp-server/tests/mcp_inventory.rs`: future M1 producer tests.
- `docs/reference/mcp-server-inventory.md`: future M1 carrier contract.

## Task 1: Documentation Grounding

**Files:**
- Modify: `docs/security/OWASP-MCP-TOP10-MAPPING.md`
- Modify: `docs/security/OWASP-MCP-TOP10-TEST-MAP.md`
- Create: `docs/superpowers/plans/2026-06-15-owasp-mcp-priority-wave.md`

- [ ] **Step 1: Verify current public mapping**

Run:

```bash
sed -n '1,220p' docs/security/OWASP-MCP-TOP10-MAPPING.md
sed -n '1,280p' docs/security/OWASP-MCP-TOP10-TEST-MAP.md
```

Expected: MCP01, MCP04, and MCP09 are Partial; MCP08 is Strong, not Complete; the M1-M6 wave is listed in priority order.

- [ ] **Step 2: Run markdown sanity checks**

Run:

```bash
git diff --check
rg -n "Complete \\| Evidence bundles, decision logs|MCP08 \\| \\*\\*Complete\\*\\*" docs/security || true
```

Expected: no whitespace errors; no stale MCP08 Complete wording remains in the security mapping.

- [ ] **Step 3: Commit docs**

Run:

```bash
git add docs/security/OWASP-MCP-TOP10-MAPPING.md docs/security/OWASP-MCP-TOP10-TEST-MAP.md docs/superpowers/plans/2026-06-15-owasp-mcp-priority-wave.md
git commit -m "docs(security): plan OWASP MCP priority evidence wave"
```

Expected: one docs commit with no code changes.

## Task 2: M1 Contract Fixture First

**Files:**
- Create: `docs/reference/mcp-server-inventory.md`
- Create: `crates/assay-mcp-server/tests/fixtures/mcp_inventory/inventory_cases.v0.json`
- Create: `crates/assay-mcp-server/tests/mcp_inventory.rs`

- [ ] **Step 1: Write the carrier contract**

Create `docs/reference/mcp-server-inventory.md` with this shape:

```json
{
  "schema": "assay.mcp_server_inventory.v0",
  "scanner_coverage": {
    "config_sources": {
      "claude_desktop": "complete",
      "vscode": "complete",
      "cursor": "not_scanned"
    },
    "process_scan": "unavailable",
    "network_scan": "not_scanned"
  },
  "servers": [
    {
      "server_id": "github-tools",
      "source": "vscode_mcp_config",
      "transport": "stdio",
      "command_digest": "sha256:...",
      "args_digest": "sha256:...",
      "observed_state": "observed"
    }
  ],
  "non_claims": [
    "absence from inventory is not absence from environment unless scanner coverage is complete"
  ]
}
```

The contract must say `servers=[]` plus incomplete coverage is not clean.

- [ ] **Step 2: Add fixture cases**

Create `crates/assay-mcp-server/tests/fixtures/mcp_inventory/inventory_cases.v0.json` with cases:

```json
[
  {"case": "approved_stdio_server_in_config", "expected": "clean"},
  {"case": "unapproved_stdio_server_in_config", "expected": "shadow_mcp_server_observed"},
  {"case": "approved_server_command_drift", "expected": "mcp_server_command_drift"},
  {"case": "approved_server_args_drift", "expected": "mcp_server_args_drift"},
  {"case": "duplicate_server_id_different_command", "expected": "duplicate_mcp_server_identity"},
  {"case": "http_mcp_endpoint_not_in_allowlist", "expected": "shadow_mcp_server_observed"},
  {"case": "server_only_visible_in_process_scan", "expected": "observed_with_partial_source_context"},
  {"case": "config_source_not_scanned", "expected": "mcp_inventory_coverage_incomplete"},
  {"case": "no_servers_found_but_coverage_incomplete", "expected": "mcp_inventory_coverage_incomplete"}
]
```

- [ ] **Step 3: Write a fixture parser test**

Create `crates/assay-mcp-server/tests/mcp_inventory.rs` with a test that loads the JSON, asserts every case has `case` and `expected`, and asserts the two coverage-incomplete cases are present.

Run:

```bash
cargo test -p assay-mcp-server --test mcp_inventory
```

Expected: PASS.

## Task 3: M1 Producer Prototype

**Files:**
- Create: `crates/assay-mcp-server/src/mcp_inventory.rs`
- Modify: `crates/assay-mcp-server/src/lib.rs`
- Modify: `crates/assay-mcp-server/tests/mcp_inventory.rs`

- [ ] **Step 1: Define inventory data model**

Implement structs for `InventoryRecord`, `ScannerCoverage`, and `InventoryServer`. Hash command and args with the same `sha256:` prefix style used by existing manifest artifacts.

- [ ] **Step 2: Implement config-source scanner adapters**

Start with static JSON fixture input, not host filesystem scanning. The scanner function should accept parsed config entries and produce `InventoryServer` rows. Real host paths are a later slice.

- [ ] **Step 3: Add coverage honesty tests**

Add tests for:

- no servers + incomplete coverage → warning/inconclusive;
- unapproved server observed → finding;
- approved server unchanged → clean;
- duplicate id with different command digest → finding.

Run:

```bash
cargo test -p assay-mcp-server --test mcp_inventory
cargo fmt --check
cargo clippy -p assay-mcp-server --all-targets -- -D warnings
```

Expected: PASS.

## Task 4: M3 Secret Sink Corpus

**Files:**
- Create: `crates/assay-mcp-server/tests/fixtures/mcp_secret_sink/`
- Create: `crates/assay-mcp-server/tests/mcp_secret_sink.rs`
- Modify only if needed: sink renderers that fail the corpus.

- [ ] **Step 1: Add hostile values fixture**

Include token-like strings, private-key block, AWS-key-like pair, email address, ANSI escape, Unicode bidi override, and an oversized field.

- [ ] **Step 2: Assert public sinks are clean**

Test markdown, SARIF, stdout-safe projection, OTel/log projection, and evidence query report if those sinks are present. If a sink is not present in Assay, mark it absent in the result rather than claiming coverage.

- [ ] **Step 3: Preserve credential alias boundary**

Assert credential aliases may render, but credential values and raw token-like payloads do not.

## Task 5: M2 / M4 Admission and Supply-Chain Fixtures

**Files:**
- Create: `docs/reference/mcp-server-admission.md`
- Create: `crates/assay-mcp-server/tests/fixtures/mcp_server_admission/`
- Create: `crates/assay-mcp-server/tests/mcp_server_admission.rs`

- [ ] **Step 1: Define `assay.mcp_server_admission.v0`**

Use declared server id, source kind, package/version when applicable, source digest, manifest digest, approval timestamp, and non-claims.

- [ ] **Step 2: Add drift zoo**

Cover same package/new digest, same server id/new manifest, tool schema change, tool description change, local binary replacement, container tag drift, unknown registry metadata, and unsigned source.

- [ ] **Step 3: Assert bounded findings**

Digest drift becomes pending/review evidence; unknown source is inconclusive/pending; no case emits maliciousness.

## Task 6: M5 / M6 Review Projection

**Files:**
- Create: `docs/reference/owasp-mcp-coverage-report.md`
- Future Plimsoll repo changes for `plimsoll.owasp_mcp_coverage_report.v0`

- [ ] **Step 1: Define source-digest based report contract**

Every coverage row must include source artifact digests and incomplete source list.

- [ ] **Step 2: Add cross-risk chain fixture**

Fixture: unapproved server + secret-like metadata + missing enforcement decision. Expected: multiple bounded findings and no single maliciousness claim.

- [ ] **Step 3: Add no-false-complete invariant**

If any required source is missing, the report row cannot be `complete`.

## Self-Review

- Spec coverage: M1-M6 are represented, with MCP09 first, MCP01 second, MCP04 third.
- Placeholder scan: no task uses TBD/TODO/fill-in language; later tasks are intentionally scoped as future slices with concrete files and acceptance.
- Type consistency: inventory uses `assay.mcp_server_inventory.v0`; admission uses `assay.mcp_server_admission.v0`; coverage report uses `plimsoll.owasp_mcp_coverage_report.v0`.
