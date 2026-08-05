# Hotspot LOC Under 600 Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring every current handwritten Rust hotspot at or above 600 LOC below 600 LOC, excluding generated files, while preserving behavior, public APIs, wire contracts, CLI output, and security invariants.

**Architecture:** Use one discovery/freeze slice, then split by subsystem waves. Each wave keeps the existing file as a stable facade, moves implementation and tests into private sibling modules, and uses a narrow allowlist gate plus targeted Cargo checks. Mechanical splits happen before any efficiency or hygiene cleanup; measurable cleanup follows only after contract tests prove behavior is unchanged.

**Tech Stack:** Rust 1.96.0, Cargo, Clippy, existing Assay crates, `scripts/ci/review-split-wave.sh`, Bash review gates, GitHub PR-body move maps.

---

## Discovery Snapshot

Run from `/Users/roelschuurkes/assay` on `fcfdb8e1` with a clean detached `HEAD`.

Generated outlier excluded:

- `crates/assay-ebpf/src/vmlinux.rs` at 60746 LOC.

Current handwritten files at or above 600 LOC:

| LOC | File | Kind | Readiness |
|---:|---|---|---|
| 1630 | `crates/assay-mcp-server/src/proxy/enforce.rs` | security/protocol production + inline tests | High value, high risk |
| 1565 | `crates/assay-mcp-server/tests/proxy_enforce_pdp_e2e.rs` | integration tests | High value, medium risk |
| 1398 | `crates/assay-core/src/mcp/tool_decision_truth.rs` | experimental MCP carrier production + inline tests | High value, high risk |
| 1390 | `crates/assay-mcp-server/src/proxy/mod.rs` | proxy runtime production + inline tests | High value, high risk |
| 1311 | `crates/assay-registry/src/supply_chain.rs` | registry verification production + inline tests | High value, high risk |
| 860 | `crates/assay-cli/src/cli/commands/supply_chain_conformance.rs` | CLI emitter production + inline tests | Medium risk |
| 851 | `crates/assay-cli/tests/evidence_test/mcp_execution_records.rs` | integration tests | Medium risk |
| 748 | `crates/assay-mcp-server/src/proxy/annotation_conformance.rs` | MCP conformance carrier + inline tests | Medium risk |
| 737 | `crates/assay-cli/src/cli/commands/mcp.rs` | CLI command orchestration + inline tests | Medium risk |
| 724 | `crates/assay-runner-core/src/kernel/tests.rs` | unit tests | Low risk |
| 701 | `crates/assay-registry/src/rekor.rs` | Rekor offline verification + inline tests | High risk |
| 645 | `crates/assay-runner-core/src/redact.rs` | redaction production + inline tests | High risk |
| 643 | `crates/assay-evidence/src/types.rs` | public evidence contract types + inline tests | High risk |
| 629 | `crates/assay-mcp-server/src/tool_decision.rs` | MCP decision carrier production + inline tests | Medium risk |
| 605 | `crates/assay-cli/src/cli/commands/run_output.rs` | CLI run output + inline tests | Medium risk |
| 600 | `crates/assay-cli/src/cli/commands/evidence/mcp_execution_records.rs` | CLI verifier production | Medium risk |

Target posture:

- Every listed source file ends below 600 LOC, preferably below 450 LOC for production facades and below 120 LOC for test facades.
- Every new module also stays below 600 LOC.
- Existing public import paths remain valid through `pub use` from the original facade.
- No generated or vendored file is reformatted or split.

## Best-Practice Rules For June 2026

- Prefer small, reviewable Rust modules with stable public facades and crate-private implementation modules.
- Keep mechanical moves behavior-free: no dependency changes, no performance tuning, no naming churn, no semantic cleanup in the same diff.
- Use 2024-era Rust hygiene: explicit module ownership, narrow visibility, no hidden panics in production paths, Clippy clean with `-D warnings`.
- Treat protocol, evidence, and security code as contract surfaces: freeze JSON shape, reason-code precedence, redaction guarantees, digest domains, exit codes, and startup failure behavior before moving code.
- Efficiency work is measured, not guessed. First preserve behavior; then remove accidental clones, repeated canonicalization, duplicate JSON allocations, or repeated regex setup only when tests and benchmarks prove no contract drift.
- Keep docs light. Routine wave context belongs in the PR body and a single rolling line in `docs/contributing/REFACTOR-WAVE-STATUS.md`, not new per-wave graveyard docs.

Official references to keep handy during execution:

- Rust API Guidelines: https://rust-lang.github.io/api-guidelines/
- Rust Clippy documentation: https://doc.rust-lang.org/clippy/
- Rust 2024 Edition Guide: https://doc.rust-lang.org/edition-guide/rust-2024/
- Cargo Book: https://doc.rust-lang.org/cargo/

## File Structure

Create once in Task 1:

- `scripts/ci/review-hotspot-loc-under-600.sh`

Modify once per landed wave:

- `docs/contributing/REFACTOR-WAVE-STATUS.md`

Create during Task 2:

- `crates/assay-mcp-server/src/proxy/enforce/policy.rs`
- `crates/assay-mcp-server/src/proxy/enforce/manifest.rs`
- `crates/assay-mcp-server/src/proxy/enforce/decision.rs`
- `crates/assay-mcp-server/src/proxy/enforce/allowance.rs`
- `crates/assay-mcp-server/src/proxy/enforce/credential_scope.rs`
- `crates/assay-mcp-server/src/proxy/enforce/records.rs`
- `crates/assay-mcp-server/src/proxy/enforce/tests.rs`
- `crates/assay-mcp-server/src/proxy/enforce/tests/policy.rs`
- `crates/assay-mcp-server/src/proxy/enforce/tests/pdp.rs`
- `crates/assay-mcp-server/src/proxy/enforce/tests/drift.rs`
- `crates/assay-mcp-server/src/proxy/enforce/tests/records.rs`
- `crates/assay-mcp-server/src/proxy/enforce/tests/fixtures.rs`

Create during Task 3:

- `crates/assay-mcp-server/src/proxy/jsonrpc.rs`
- `crates/assay-mcp-server/src/proxy/observer.rs`
- `crates/assay-mcp-server/src/proxy/artifacts.rs`
- `crates/assay-mcp-server/src/proxy/io.rs`
- `crates/assay-mcp-server/src/proxy/run_loop.rs`
- `crates/assay-mcp-server/src/proxy/establish_flow.rs`
- `crates/assay-mcp-server/src/proxy/tests.rs`
- `crates/assay-mcp-server/src/proxy/tests/observer.rs`
- `crates/assay-mcp-server/src/proxy/tests/establish.rs`

Create during Task 4:

- `crates/assay-mcp-server/src/proxy/annotation_conformance/declared.rs`
- `crates/assay-mcp-server/src/proxy/annotation_conformance/observed.rs`
- `crates/assay-mcp-server/src/proxy/annotation_conformance/record.rs`
- `crates/assay-mcp-server/src/proxy/annotation_conformance/contract_fixture.rs`
- `crates/assay-mcp-server/src/proxy/annotation_conformance/tests.rs`

Create during Task 5:

- `crates/assay-mcp-server/tests/proxy_enforce_pdp_e2e/support.rs`
- `crates/assay-mcp-server/tests/proxy_enforce_pdp_e2e/pdp_denials.rs`
- `crates/assay-mcp-server/tests/proxy_enforce_pdp_e2e/startup.rs`
- `crates/assay-mcp-server/tests/proxy_enforce_pdp_e2e/establish.rs`
- `crates/assay-mcp-server/tests/proxy_enforce_pdp_e2e/carriers.rs`
- `crates/assay-mcp-server/tests/proxy_enforce_pdp_e2e/conformance.rs`

Create during Task 6:

- `crates/assay-core/src/mcp/tool_decision_truth/digest.rs`
- `crates/assay-core/src/mcp/tool_decision_truth/record.rs`
- `crates/assay-core/src/mcp/tool_decision_truth/verdict.rs`
- `crates/assay-core/src/mcp/tool_decision_truth/identity.rs`
- `crates/assay-core/src/mcp/tool_decision_truth/pack.rs`
- `crates/assay-core/src/mcp/tool_decision_truth/tests.rs`
- `crates/assay-core/src/mcp/tool_decision_truth/tests/digest.rs`
- `crates/assay-core/src/mcp/tool_decision_truth/tests/gate.rs`
- `crates/assay-core/src/mcp/tool_decision_truth/tests/pack.rs`

Create during Task 7:

- `crates/assay-registry/src/supply_chain/types.rs`
- `crates/assay-registry/src/supply_chain/dsse.rs`
- `crates/assay-registry/src/supply_chain/sigstore.rs`
- `crates/assay-registry/src/supply_chain/provenance.rs`
- `crates/assay-registry/src/supply_chain/pinning.rs`
- `crates/assay-registry/src/supply_chain/policy.rs`
- `crates/assay-registry/src/supply_chain/tests.rs`

Create during Task 8:

- `crates/assay-registry/src/rekor/checkpoint.rs`
- `crates/assay-registry/src/rekor/tlog.rs`
- `crates/assay-registry/src/rekor/rfc6962.rs`
- `crates/assay-registry/src/rekor/body.rs`
- `crates/assay-registry/src/rekor/verify.rs`
- `crates/assay-registry/src/rekor/tests.rs`

Create during Task 9:

- `crates/assay-cli/src/cli/commands/supply_chain_conformance/descriptor.rs`
- `crates/assay-cli/src/cli/commands/supply_chain_conformance/carrier.rs`
- `crates/assay-cli/src/cli/commands/supply_chain_conformance/io.rs`
- `crates/assay-cli/src/cli/commands/supply_chain_conformance/tests.rs`

Create during Task 10:

- `crates/assay-cli/src/cli/commands/mcp/coverage.rs`
- `crates/assay-cli/src/cli/commands/mcp/temp.rs`
- `crates/assay-cli/src/cli/commands/mcp/tdt.rs`
- `crates/assay-cli/src/cli/commands/mcp/wrap.rs`
- `crates/assay-cli/src/cli/commands/mcp/tests.rs`

Create during Task 11:

- `crates/assay-cli/src/cli/commands/run_output/reason_codes.rs`
- `crates/assay-cli/src/cli/commands/run_output/summary_json.rs`
- `crates/assay-cli/src/cli/commands/run_output/sanitize.rs`
- `crates/assay-cli/src/cli/commands/run_output/tests.rs`
- `crates/assay-cli/src/cli/commands/evidence/mcp_execution_records/args.rs`
- `crates/assay-cli/src/cli/commands/evidence/mcp_execution_records/input.rs`
- `crates/assay-cli/src/cli/commands/evidence/mcp_execution_records/report.rs`
- `crates/assay-cli/src/cli/commands/evidence/mcp_execution_records/binding.rs`
- `crates/assay-cli/src/cli/commands/evidence/mcp_execution_records/checks.rs`
- `crates/assay-cli/src/cli/commands/evidence/mcp_execution_records/print.rs`

Create during Task 12:

- `crates/assay-runner-core/src/redact/mode.rs`
- `crates/assay-runner-core/src/redact/rules.rs`
- `crates/assay-runner-core/src/redact/engine.rs`
- `crates/assay-runner-core/src/redact/argv.rs`
- `crates/assay-runner-core/src/redact/url.rs`
- `crates/assay-runner-core/src/redact/tests.rs`
- `crates/assay-runner-core/src/kernel/tests/events.rs`
- `crates/assay-runner-core/src/kernel/tests/network.rs`
- `crates/assay-runner-core/src/kernel/tests/health.rs`
- `crates/assay-runner-core/src/kernel/tests/apply.rs`

Create during Task 13:

- `crates/assay-evidence/src/types/producer.rs`
- `crates/assay-evidence/src/types/event.rs`
- `crates/assay-evidence/src/types/payload.rs`
- `crates/assay-evidence/src/types/sandbox.rs`
- `crates/assay-evidence/src/types/tests.rs`
- `crates/assay-mcp-server/src/tool_decision/hash.rs`
- `crates/assay-mcp-server/src/tool_decision/classifier.rs`
- `crates/assay-mcp-server/src/tool_decision/surface.rs`
- `crates/assay-mcp-server/src/tool_decision/sanitize.rs`
- `crates/assay-mcp-server/src/tool_decision/tests.rs`

Create during Task 14:

- `crates/assay-cli/tests/evidence_test/mcp_execution_records/support.rs`
- `crates/assay-cli/tests/evidence_test/mcp_execution_records/pairing.rs`
- `crates/assay-cli/tests/evidence_test/mcp_execution_records/fallback.rs`
- `crates/assay-cli/tests/evidence_test/mcp_execution_records/named_projection.rs`

## Task 1: Freeze Baseline And Add Generic LOC Gate

**Files:**

- Create: `scripts/ci/review-hotspot-loc-under-600.sh`
- Modify: `docs/contributing/REFACTOR-WAVE-STATUS.md`

- [ ] **Step 1: Record live baseline**

Run:

```bash
git status --short
git rev-parse --short HEAD
rg --files -g '*.rs' | xargs wc -l | sort -nr | sed -n '1,40p'
```

Expected:

- `git status --short` is empty or unrelated user changes are documented in the PR body.
- `crates/assay-ebpf/src/vmlinux.rs` is the only generated outlier.
- The 16 handwritten files listed in this plan are still the only files at or above 600 LOC.

- [ ] **Step 2: Add the generic LOC gate**

Create `scripts/ci/review-hotspot-loc-under-600.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

threshold="${HOTSPOT_LOC_THRESHOLD:-600}"

violations="$(
  find crates -name '*.rs' -type f \
    ! -path '*/target/*' \
    ! -name 'vmlinux.rs' \
    -print0 \
    | xargs -0 wc -l 2>/dev/null \
    | awk -v threshold="${threshold}" '$2 != "total" && $1 >= threshold { printf "%s %s\n", $1, $2 }' \
    | sort -rn
)"

if [[ -n "${violations}" ]]; then
  echo "FAIL: handwritten Rust files at or above ${threshold} LOC:" >&2
  printf '%s\n' "${violations}" >&2
  exit 1
fi

echo "PASS: no handwritten Rust files at or above ${threshold} LOC"
```

- [ ] **Step 3: Make the gate executable**

Run:

```bash
chmod +x scripts/ci/review-hotspot-loc-under-600.sh
```

Expected: no output.

- [ ] **Step 4: Update rolling status**

Append one row to `docs/contributing/REFACTOR-WAVE-STATUS.md` under the closed/current status table once the first PR lands:

```markdown
| Wave71 | Hotspot LOC under 600 | in progress | Active | Current handwritten >=600 LOC files are being reduced by subsystem waves with `review-hotspot-loc-under-600.sh` as the final closure gate |
```

- [ ] **Step 5: Verify freeze slice**

Run:

```bash
cargo fmt --check
git diff --check
```

Expected: both commands pass.

- [ ] **Step 6: Commit freeze slice**

Run:

```bash
git add scripts/ci/review-hotspot-loc-under-600.sh docs/contributing/REFACTOR-WAVE-STATUS.md
git commit -m "chore: freeze hotspot loc refactor gate"
```

## Task 2: Split MCP Server PDP Enforcement

**Files:**

- Modify: `crates/assay-mcp-server/src/proxy/enforce.rs`
- Create: `crates/assay-mcp-server/src/proxy/enforce/{policy,manifest,decision,allowance,credential_scope,records,tests}.rs`
- Create: `crates/assay-mcp-server/src/proxy/enforce/tests/{policy,pdp,drift,records,fixtures}.rs`

- [ ] **Step 1: Characterize before move**

Run:

```bash
cargo test -p assay-mcp-server proxy::enforce -- --nocapture
cargo test -p assay-mcp-server pdp -- --nocapture
cargo test -p assay-mcp-server golden -- --nocapture
```

Expected: existing enforcement unit tests and golden corpus tests pass.

- [ ] **Step 2: Move policy and manifest loading**

Move these items out of `enforce.rs`:

- `EnforceInputs`, `EnforcePolicy`, `Caller`, `UpstreamCredential`, `Allowance`, `Target` to `enforce/policy.rs`.
- `DeclaredManifest`, `BaselineTool`, `ObservedToolDigest`, `load_declared_manifest` to `enforce/manifest.rs`.
- `load` to `enforce/policy.rs`.

Keep `enforce.rs` as facade:

```rust
mod allowance;
mod credential_scope;
mod decision;
mod manifest;
mod policy;
mod records;

#[cfg(test)]
mod tests;

pub use decision::{decide, Decision};
pub use manifest::{load_declared_manifest, BaselineTool, DeclaredManifest, ObservedToolDigest};
pub use policy::{load, Allowance, Caller, EnforceInputs, EnforcePolicy, Target, UpstreamCredential};
pub use records::decision_record;
```

- [ ] **Step 3: Move PDP helpers**

Move these items:

- `target_digest`, `allowance_matches` to `enforce/allowance.rs`.
- `ScopeCoverage`, `ScopeLattice`, `credential_scope_gate`, `lattice_for`, `scope_covers` to `enforce/credential_scope.rs`.
- `Decision`, `Decision::deny`, `decide`, `drift_state` to `enforce/decision.rs`.
- `decision_record` to `enforce/records.rs`.

Use `pub(crate)` only where sibling modules need access; keep the external API through `enforce.rs`.

- [ ] **Step 4: Split inline tests**

Move inline tests by subject:

- load/validation tests to `enforce/tests/policy.rs`.
- allow/deny precedence tests to `enforce/tests/pdp.rs`.
- manifest drift tests to `enforce/tests/drift.rs`.
- decision record and carrier fixture tests to `enforce/tests/records.rs`.
- shared builders such as `policy_from`, `baseline_with`, `matching_baseline`, `golden_corpus` to `enforce/tests/fixtures.rs`.

`enforce/tests.rs` should only declare submodules and shared imports.

- [ ] **Step 5: Verify LOC target**

Run:

```bash
wc -l crates/assay-mcp-server/src/proxy/enforce.rs crates/assay-mcp-server/src/proxy/enforce/*.rs crates/assay-mcp-server/src/proxy/enforce/tests/*.rs
```

Expected: every listed file is below 600 LOC; `enforce.rs` is below 250 LOC.

- [ ] **Step 6: Verify behavior**

Run:

```bash
cargo fmt --check
cargo test -p assay-mcp-server proxy::enforce
cargo test -p assay-mcp-server pdp_golden
cargo clippy -p assay-mcp-server --all-targets -- -D warnings
scripts/ci/review-split-wave.sh assay-mcp-server '^crates/assay-mcp-server/src/proxy/enforce'
```

Expected: all commands pass.

- [ ] **Step 7: Commit**

Run:

```bash
git add crates/assay-mcp-server/src/proxy/enforce.rs crates/assay-mcp-server/src/proxy/enforce
git commit -m "refactor(mcp-server): split proxy enforcement pdp"
```

## Task 3: Split MCP Server Proxy Runtime

**Files:**

- Modify: `crates/assay-mcp-server/src/proxy/mod.rs`
- Create: `crates/assay-mcp-server/src/proxy/{jsonrpc,observer,artifacts,io,run_loop,establish_flow,tests}.rs`
- Create: `crates/assay-mcp-server/src/proxy/tests/{observer,establish}.rs`

- [ ] **Step 1: Characterize before move**

Run:

```bash
cargo test -p assay-mcp-server proxy::tests -- --nocapture
cargo test -p assay-mcp-server run_establish -- --nocapture
```

Expected: proxy observer and establish tests pass.

- [ ] **Step 2: Move constants and JSON-RPC helpers**

Move `ALLOWED_METHODS`, `PROXY_UNSUPPORTED`, `PROXY_DENIED`, `proxy_error_line`, and `Mode` to `proxy/jsonrpc.rs`.

`proxy/mod.rs` should re-export only the public mode and `run` entrypoint:

```rust
mod artifacts;
mod establish_flow;
mod io;
mod jsonrpc;
mod observer;
mod run_loop;

#[cfg(test)]
mod tests;

pub use jsonrpc::Mode;
pub use run_loop::run;
```

- [ ] **Step 3: Move observer state**

Move `Observer`, `Emission`, `manifest_from`, `health_from`, and observation helper methods to `proxy/observer.rs` and `proxy/artifacts.rs`.

Keep the invariant that the annotation conformance carrier reads the same complete manifest view as the drift gate.

- [ ] **Step 4: Move runtime loop**

Move `run` to `proxy/run_loop.rs`, `forward_line`, `write_json_atomic`, and `append_decision_record` to `proxy/io.rs`, and `run_establish`, `EstablishRunOutcome`, `emit_not_observed`, `degraded_loop` to `proxy/establish_flow.rs`.

- [ ] **Step 5: Split inline tests**

Move observer tests to `proxy/tests/observer.rs` and establish tests to `proxy/tests/establish.rs`. Keep `proxy/tests.rs` as a module facade.

- [ ] **Step 6: Verify LOC target**

Run:

```bash
wc -l crates/assay-mcp-server/src/proxy/mod.rs crates/assay-mcp-server/src/proxy/{jsonrpc,observer,artifacts,io,run_loop,establish_flow,tests}.rs crates/assay-mcp-server/src/proxy/tests/*.rs
```

Expected: every listed file is below 600 LOC; `proxy/mod.rs` is below 160 LOC.

- [ ] **Step 7: Verify behavior**

Run:

```bash
cargo fmt --check
cargo test -p assay-mcp-server proxy::tests
cargo test -p assay-mcp-server run_establish
cargo clippy -p assay-mcp-server --all-targets -- -D warnings
scripts/ci/review-split-wave.sh assay-mcp-server '^crates/assay-mcp-server/src/proxy'
```

Expected: all commands pass.

- [ ] **Step 8: Commit**

Run:

```bash
git add crates/assay-mcp-server/src/proxy
git commit -m "refactor(mcp-server): split proxy runtime facade"
```

## Task 4: Split Annotation Conformance Carrier

**Files:**

- Modify: `crates/assay-mcp-server/src/proxy/annotation_conformance.rs`
- Create: `crates/assay-mcp-server/src/proxy/annotation_conformance/{declared,observed,record,contract_fixture,tests}.rs`

- [ ] **Step 1: Characterize before move**

Run:

```bash
cargo test -p assay-mcp-server annotation_conformance -- --nocapture
cargo test -p assay-mcp-server tool_annotation_conformance_contract_fixture -- --nocapture
```

Expected: all annotation conformance tests pass.

- [ ] **Step 2: Move code by responsibility**

Move:

- `DeclaredToolAnnotations`, `extract_declared_annotations` to `declared.rs`.
- `ObservationBasis`, `ObservedBehavior`, `observed_behavior`, `push_axis`, `conformance_for` to `observed.rs`.
- `build_tool_annotation_conformance_record` to `record.rs`.
- contract fixture builders to `contract_fixture.rs`.
- tests to `tests.rs`.

Keep facade:

```rust
mod declared;
mod observed;
mod record;

#[cfg(test)]
mod contract_fixture;
#[cfg(test)]
mod tests;

pub use declared::{extract_declared_annotations, DeclaredToolAnnotations};
pub use observed::{conformance_for, ObservationBasis};
pub use record::build_tool_annotation_conformance_record;
```

- [ ] **Step 3: Verify**

Run:

```bash
wc -l crates/assay-mcp-server/src/proxy/annotation_conformance.rs crates/assay-mcp-server/src/proxy/annotation_conformance/*.rs
cargo fmt --check
cargo test -p assay-mcp-server annotation_conformance
cargo clippy -p assay-mcp-server --all-targets -- -D warnings
```

Expected: every file is below 600 LOC; facade below 120 LOC.

- [ ] **Step 4: Commit**

Run:

```bash
git add crates/assay-mcp-server/src/proxy/annotation_conformance.rs crates/assay-mcp-server/src/proxy/annotation_conformance
git commit -m "refactor(mcp-server): split annotation conformance carrier"
```

## Task 5: Split Proxy Enforcement E2E Tests

**Files:**

- Modify: `crates/assay-mcp-server/tests/proxy_enforce_pdp_e2e.rs`
- Create: `crates/assay-mcp-server/tests/proxy_enforce_pdp_e2e/{support,pdp_denials,startup,establish,carriers,conformance}.rs`

- [ ] **Step 1: Characterize before move**

Run:

```bash
cargo test -p assay-mcp-server --test proxy_enforce_pdp_e2e -- --nocapture
```

Expected: all 33 current tests pass.

- [ ] **Step 2: Create test facade**

Replace the root test file with only module declarations:

```rust
mod support;
mod pdp_denials;
mod startup;
mod establish;
mod carriers;
mod conformance;
```

Keep all shared helpers in `support.rs`.

- [ ] **Step 3: Move tests by scenario**

Move:

- Deny reason tests from lines 217-414 to `pdp_denials.rs`.
- Startup validation tests from lines 489-588 and budget test to `startup.rs`.
- Establish flow tests from lines 640-808 to `establish.rs`.
- Carrier and decision-output tests from lines 848-1080 to `carriers.rs`.
- Annotation conformance tests from lines 1156-1476 to `conformance.rs`.

- [ ] **Step 4: Verify**

Run:

```bash
wc -l crates/assay-mcp-server/tests/proxy_enforce_pdp_e2e.rs crates/assay-mcp-server/tests/proxy_enforce_pdp_e2e/*.rs
cargo fmt --check
cargo test -p assay-mcp-server --test proxy_enforce_pdp_e2e
cargo clippy -p assay-mcp-server --all-targets -- -D warnings
```

Expected: every file is below 600 LOC; facade below 80 LOC.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/assay-mcp-server/tests/proxy_enforce_pdp_e2e.rs crates/assay-mcp-server/tests/proxy_enforce_pdp_e2e
git commit -m "test(mcp-server): split proxy enforcement e2e scenarios"
```

## Task 6: Split Tool Decision Truth Layer

**Files:**

- Modify: `crates/assay-core/src/mcp/tool_decision_truth.rs`
- Create: `crates/assay-core/src/mcp/tool_decision_truth/{digest,record,verdict,identity,pack,tests}.rs`
- Create: `crates/assay-core/src/mcp/tool_decision_truth/tests/{digest,gate,pack}.rs`

- [ ] **Step 1: Characterize before move**

Run:

```bash
cargo test -p assay-core tool_decision_truth -- --nocapture
cargo test -p assay-core tool_decision_truth_vectors -- --nocapture
```

Expected: all tool-decision truth unit and vector tests pass.

- [ ] **Step 2: Move digest primitives**

Move:

- `HmacSha256`, `normalize_key`, `is_secret_key`, `project_args_for_digest`, `is_valid_key_id`, `args_digest`, `observed_input_digest` to `digest.rs`.
- `build_record`, provenance vocabularies, and carrier schema constants to `record.rs`.
- `DecisionEvidence`, `Applicability`, axis helpers, `decision_verdict`, `run_verdict` to `verdict.rs`.
- `decision_identity_digest`, `carrier_content_digest`, `evidence_ref` to `identity.rs`.
- `pack_recipe_row`, `verify_recipe_row`, pack coherence helpers to `pack.rs`.

Facade:

```rust
mod digest;
mod identity;
mod pack;
mod record;
mod verdict;

#[cfg(test)]
mod tests;

pub use digest::{args_digest, observed_input_digest};
pub use identity::{carrier_content_digest, decision_identity_digest, evidence_ref};
pub use pack::{pack_recipe_row, verify_recipe_row};
pub use record::{build_classified_record, build_record};
pub use verdict::{decision_verdict, run_verdict, DecisionEvidence};
```

- [ ] **Step 3: Split tests**

Move:

- Digest and redaction tests to `tests/digest.rs`.
- Verdict gate tests to `tests/gate.rs`.
- Pack recipe tests to `tests/pack.rs`.

- [ ] **Step 4: Verify**

Run:

```bash
wc -l crates/assay-core/src/mcp/tool_decision_truth.rs crates/assay-core/src/mcp/tool_decision_truth/*.rs crates/assay-core/src/mcp/tool_decision_truth/tests/*.rs
cargo fmt --check
cargo test -p assay-core tool_decision_truth
cargo test -p assay-core tool_decision_truth_vectors
cargo clippy -p assay-core --all-targets -- -D warnings
scripts/ci/review-split-wave.sh assay-core '^crates/assay-core/src/mcp/tool_decision_truth'
```

Expected: every file below 600 LOC; facade below 160 LOC.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/assay-core/src/mcp/tool_decision_truth.rs crates/assay-core/src/mcp/tool_decision_truth
git commit -m "refactor(core): split tool decision truth layer"
```

## Task 7: Split Registry Supply-Chain Verification

**Files:**

- Modify: `crates/assay-registry/src/supply_chain.rs`
- Create: `crates/assay-registry/src/supply_chain/{types,dsse,sigstore,provenance,pinning,policy,tests}.rs`

- [ ] **Step 1: Characterize before move**

Run:

```bash
cargo test -p assay-registry supply_chain -- --nocapture
cargo test -p assay-registry valid_pinned_key_slsa_provenance_is_verified_and_clean -- --nocapture
```

Expected: supply-chain contract tests pass.

- [ ] **Step 2: Move types and pure helpers**

Move public report/input types to `types.rs`: `CheckStatus`, `SlsaLevel`, `Subject`, `IntegrityChecks`, `ProvenanceChecks`, `PinningChecks`, `Checks`, `DeclaredLevel`, `VerifiedLevel`, `Coverage`, `PolicyResult`, `SupplyChainConformance`, `ProvenanceInput`, `UnsupportedProvenance`, `SigstoreBundleInput`, `ContainerRef`, `PinningInput`, `Policy`, and `VerifyInput`.

- [ ] **Step 3: Move verification engines**

Move:

- `build_pae`, `verify_dsse_signature`, `decode_statement`, DSSE structs to `dsse.rs`.
- `parse_sigstore_bundle`, `verify_sigstore_bundle_provenance`, `ParsedSigstoreBundleEvidence` to `sigstore.rs`.
- `verify_provenance`, `ProvenanceOutcome`, in-toto structs to `provenance.rs`.
- `verify_pinning` and pinning helpers to `pinning.rs`.
- `all_statuses`, `non_transparency_statuses`, `compute_policy_result`, `verify_supply_chain`, `is_clean` to `policy.rs`.

Facade re-exports public types and functions.

- [ ] **Step 4: Split tests**

Move all inline tests to `supply_chain/tests.rs`, grouped with nested `mod provenance`, `mod pinning`, `mod sigstore`, and `mod carrier`.

- [ ] **Step 5: Verify**

Run:

```bash
wc -l crates/assay-registry/src/supply_chain.rs crates/assay-registry/src/supply_chain/*.rs
cargo fmt --check
cargo test -p assay-registry supply_chain
cargo clippy -p assay-registry --all-targets -- -D warnings
scripts/ci/review-split-wave.sh assay-registry '^crates/assay-registry/src/supply_chain'
```

Expected: every file below 600 LOC; facade below 180 LOC.

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/assay-registry/src/supply_chain.rs crates/assay-registry/src/supply_chain
git commit -m "refactor(registry): split supply chain verification"
```

## Task 8: Split Registry Rekor Verification

**Files:**

- Modify: `crates/assay-registry/src/rekor.rs`
- Create: `crates/assay-registry/src/rekor/{checkpoint,tlog,rfc6962,body,verify,tests}.rs`

- [ ] **Step 1: Characterize before move**

Run:

```bash
cargo test -p assay-registry rekor -- --nocapture
cargo test -p assay-registry rfc6962 -- --nocapture
```

Expected: Rekor body schema, checkpoint, and proof tests pass.

- [ ] **Step 2: Move by verification stage**

Move:

- `TransparencyRequirement`, `RekorInclusionOutcome`, `missing_proof` to `verify.rs`.
- `PinnedTlog`, `normalize_origin`, `pinned_tlogs` to `tlog.rs`.
- `CheckpointSig`, `Checkpoint`, `parse_checkpoint` to `checkpoint.rs`.
- `rfc6962_root` to `rfc6962.rs`.
- `HashedRekordBody`, `BodySpec`, `BodyV002`, `BodyData`, `BodySignature`, `BodyVerifier`, `BodyCert` to `body.rs`.
- `verify_rekor_v2_inclusion_offline` to `verify.rs`.

Keep `rekor.rs` as public facade.

- [ ] **Step 3: Move tests**

Move inline tests to `rekor/tests.rs`. Keep body schema cases close to `body.rs` helpers through private imports.

- [ ] **Step 4: Verify**

Run:

```bash
wc -l crates/assay-registry/src/rekor.rs crates/assay-registry/src/rekor/*.rs
cargo fmt --check
cargo test -p assay-registry rekor
cargo clippy -p assay-registry --all-targets -- -D warnings
```

Expected: every file below 600 LOC; facade below 120 LOC.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/assay-registry/src/rekor.rs crates/assay-registry/src/rekor
git commit -m "refactor(registry): split rekor offline verifier"
```

## Task 9: Split Supply-Chain CLI Emitter

**Files:**

- Modify: `crates/assay-cli/src/cli/commands/supply_chain_conformance.rs`
- Create: `crates/assay-cli/src/cli/commands/supply_chain_conformance/{descriptor,carrier,io,tests}.rs`

- [ ] **Step 1: Characterize before move**

Run:

```bash
cargo test -p assay-cli supply_chain_conformance -- --nocapture
cargo test -p assay-cli committed_dsse_example_emits_a_pass_carrier -- --nocapture
```

Expected: CLI emitter and committed example tests pass.

- [ ] **Step 2: Move responsibilities**

Move:

- Descriptor structs and path resolution to `descriptor.rs`.
- `build_carrier`, DSSE descriptor validation, and carrier construction tests to `carrier.rs`.
- `run` and `map_write_result` to `io.rs`.
- inline tests to `tests.rs`.

Facade re-exports `run`.

- [ ] **Step 3: Verify**

Run:

```bash
wc -l crates/assay-cli/src/cli/commands/supply_chain_conformance.rs crates/assay-cli/src/cli/commands/supply_chain_conformance/*.rs
cargo fmt --check
cargo test -p assay-cli supply_chain_conformance
cargo clippy -p assay-cli --all-targets -- -D warnings
```

Expected: every file below 600 LOC; facade below 80 LOC.

- [ ] **Step 4: Commit**

Run:

```bash
git add crates/assay-cli/src/cli/commands/supply_chain_conformance.rs crates/assay-cli/src/cli/commands/supply_chain_conformance
git commit -m "refactor(cli): split supply chain conformance command"
```

## Task 10: Split MCP CLI Command

**Files:**

- Modify: `crates/assay-cli/src/cli/commands/mcp.rs`
- Create: `crates/assay-cli/src/cli/commands/mcp/{coverage,temp,tdt,wrap,tests}.rs`

- [ ] **Step 1: Characterize before move**

Run:

```bash
cargo test -p assay-cli mcp_wrap -- --nocapture
cargo test -p assay-cli tdt_producer -- --nocapture
```

Expected: MCP wrap coverage and tool-decision-truth producer tests pass.

- [ ] **Step 2: Move code**

Move:

- `is_explicit_tool_name`, `collect_declared_tools`, decision event extraction, and normalizer to `coverage.rs`.
- `unique_temp_path`, `generate_session_id`, `TempPathGuard` to `temp.rs`.
- `build_tdt_producer`, `tdt_producer_from_material` to `tdt.rs`.
- `cmd_wrap` and `run` command dispatch to `wrap.rs`.
- inline tests to `tests.rs`.

Facade re-exports `run`.

- [ ] **Step 3: Verify**

Run:

```bash
wc -l crates/assay-cli/src/cli/commands/mcp.rs crates/assay-cli/src/cli/commands/mcp/*.rs
cargo fmt --check
cargo test -p assay-cli mcp_wrap
cargo test -p assay-cli tdt_producer
cargo clippy -p assay-cli --all-targets -- -D warnings
```

Expected: every file below 600 LOC; facade below 80 LOC.

- [ ] **Step 4: Commit**

Run:

```bash
git add crates/assay-cli/src/cli/commands/mcp.rs crates/assay-cli/src/cli/commands/mcp
git commit -m "refactor(cli): split mcp command orchestration"
```

## Task 11: Split CLI Run Output And MCP Execution Verifier

**Files:**

- Modify: `crates/assay-cli/src/cli/commands/run_output.rs`
- Create: `crates/assay-cli/src/cli/commands/run_output/{reason_codes,summary_json,sanitize,tests}.rs`
- Modify: `crates/assay-cli/src/cli/commands/evidence/mcp_execution_records.rs`
- Create: `crates/assay-cli/src/cli/commands/evidence/mcp_execution_records/{args,input,report,binding,checks,print}.rs`

- [ ] **Step 1: Characterize before move**

Run:

```bash
cargo test -p assay-cli run_output -- --nocapture
cargo test -p assay-cli mcp_execution_records -- --nocapture
```

Expected: run-output and verifier tests pass.

- [ ] **Step 2: Split run output**

Move:

- reason-code mapping helpers to `run_output/reason_codes.rs`.
- extended and minimal JSON construction to `run_output/summary_json.rs`.
- `write_sanitized_run_json` to `run_output/sanitize.rs`.
- inline tests to `run_output/tests.rs`.

- [ ] **Step 3: Split MCP execution verifier**

Move:

- args and format enums to `mcp_execution_records/args.rs`.
- input reading and `BindingInput` to `input.rs`.
- report structs and `build_report` to `report.rs`.
- expectation and digest binding helpers to `binding.rs`.
- check helpers to `checks.rs`.
- `print_table_report` to `print.rs`.

Keep `cmd_verify_mcp_records` public through the original facade.

- [ ] **Step 4: Verify**

Run:

```bash
wc -l crates/assay-cli/src/cli/commands/run_output.rs crates/assay-cli/src/cli/commands/run_output/*.rs crates/assay-cli/src/cli/commands/evidence/mcp_execution_records.rs crates/assay-cli/src/cli/commands/evidence/mcp_execution_records/*.rs
cargo fmt --check
cargo test -p assay-cli run_output
cargo test -p assay-cli mcp_execution_records
cargo clippy -p assay-cli --all-targets -- -D warnings
```

Expected: every file below 600 LOC; both facades below 120 LOC.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/assay-cli/src/cli/commands/run_output.rs crates/assay-cli/src/cli/commands/run_output \
  crates/assay-cli/src/cli/commands/evidence/mcp_execution_records.rs \
  crates/assay-cli/src/cli/commands/evidence/mcp_execution_records
git commit -m "refactor(cli): split run output and mcp record verifier"
```

## Task 12: Split Runner Redaction And Kernel Tests

**Files:**

- Modify: `crates/assay-runner-core/src/redact.rs`
- Create: `crates/assay-runner-core/src/redact/{mode,rules,engine,argv,url,tests}.rs`
- Modify: `crates/assay-runner-core/src/kernel/tests.rs`
- Create: `crates/assay-runner-core/src/kernel/tests/{events,network,health,apply}.rs`

- [ ] **Step 1: Characterize before move**

Run:

```bash
cargo test -p assay-runner-core redact -- --nocapture
cargo test -p assay-runner-core kernel -- --nocapture
```

Expected: redaction and kernel tests pass.

- [ ] **Step 2: Split redaction code**

Move:

- `RedactMode`, `RedactionTally` to `mode.rs`.
- `Rule`, regex accessors, `rule_specs`, `build_rules` to `rules.rs`.
- `Redactor`, `redact_value`, `shape_pass`, `find_unredacted` to `engine.rs`.
- argv helpers to `argv.rs`.
- URL userinfo helpers to `url.rs`.
- inline tests to `tests.rs`.

Keep public `Redactor`, `RedactMode`, `RedactionTally`, and `rule_specs` re-exported from `redact.rs`.

- [ ] **Step 3: Split kernel tests**

Move:

- open/exec event tests to `events.rs`.
- connect/send/datagram network tests to `network.rs`.
- health/drop/correlation tests to `health.rs`.
- apply/run-id/platform tests to `apply.rs`.

Keep `kernel/tests.rs` as test module facade.

- [ ] **Step 4: Verify**

Run:

```bash
wc -l crates/assay-runner-core/src/redact.rs crates/assay-runner-core/src/redact/*.rs crates/assay-runner-core/src/kernel/tests.rs crates/assay-runner-core/src/kernel/tests/*.rs
cargo fmt --check
cargo test -p assay-runner-core redact
cargo test -p assay-runner-core kernel
cargo clippy -p assay-runner-core --all-targets -- -D warnings
```

Expected: every file below 600 LOC; facades below 120 LOC.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/assay-runner-core/src/redact.rs crates/assay-runner-core/src/redact \
  crates/assay-runner-core/src/kernel/tests.rs crates/assay-runner-core/src/kernel/tests
git commit -m "refactor(runner): split redaction and kernel tests"
```

## Task 13: Split Evidence Types And MCP Decision Surface

**Files:**

- Modify: `crates/assay-evidence/src/types.rs`
- Create: `crates/assay-evidence/src/types/{producer,event,payload,sandbox,tests}.rs`
- Modify: `crates/assay-mcp-server/src/tool_decision.rs`
- Create: `crates/assay-mcp-server/src/tool_decision/{hash,classifier,surface,sanitize,tests}.rs`

- [ ] **Step 1: Characterize before move**

Run:

```bash
cargo test -p assay-evidence types -- --nocapture
cargo test -p assay-mcp-server tool_decision -- --nocapture
```

Expected: evidence type contract tests and tool decision tests pass.

- [ ] **Step 2: Split evidence types**

Move:

- `ProducerMeta`, `Envelope` alias to `producer.rs`.
- `EvidenceEvent` and builder methods to `event.rs`.
- `Payload` and payload structs to `payload.rs`.
- sandbox degradation enums and payload to `sandbox.rs`.
- inline tests to `tests.rs`.

Keep `types.rs` as public facade:

```rust
mod event;
mod payload;
mod producer;
mod sandbox;

#[cfg(test)]
mod tests;

pub use event::EvidenceEvent;
pub use payload::{Payload, PayloadEnvFiltered, PayloadExecObserved, PayloadPolicySuggested, PayloadProfileFinished, PayloadProfileStarted, PayloadToolDecision};
pub use producer::{Envelope, ProducerMeta};
pub use sandbox::{PayloadSandboxDegraded, SandboxDegradationComponent, SandboxDegradationMode, SandboxDegradationReasonCode};
```

- [ ] **Step 3: Split MCP decision surface**

Move:

- `target_hash` to `hash.rs`.
- classifier types and `classify`, `required_scope_for` to `classifier.rs`.
- `build_decision`, `surface`, `ObservedCall`, `Effect` to `surface.rs`.
- `sanitize` to `sanitize.rs`.
- inline tests to `tests.rs`.

Facade re-exports the existing public API.

- [ ] **Step 4: Verify**

Run:

```bash
wc -l crates/assay-evidence/src/types.rs crates/assay-evidence/src/types/*.rs crates/assay-mcp-server/src/tool_decision.rs crates/assay-mcp-server/src/tool_decision/*.rs
cargo fmt --check
cargo test -p assay-evidence types
cargo test -p assay-mcp-server tool_decision
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo clippy -p assay-mcp-server --all-targets -- -D warnings
```

Expected: every file below 600 LOC; facades below 120 LOC.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/assay-evidence/src/types.rs crates/assay-evidence/src/types \
  crates/assay-mcp-server/src/tool_decision.rs crates/assay-mcp-server/src/tool_decision
git commit -m "refactor: split evidence types and mcp decision surface"
```

## Task 14: Split CLI MCP Execution Integration Tests

**Files:**

- Modify: `crates/assay-cli/tests/evidence_test/mcp_execution_records.rs`
- Create: `crates/assay-cli/tests/evidence_test/mcp_execution_records/{support,pairing,fallback,named_projection}.rs`

- [ ] **Step 1: Characterize before move**

Run:

```bash
cargo test -p assay-cli --test evidence_test mcp_execution_records -- --nocapture
```

Expected: all MCP execution record integration tests pass.

- [ ] **Step 2: Create test facade**

Keep the root integration-test module as a facade:

```rust
mod support;
mod pairing;
mod fallback;
mod named_projection;
```

Keep helper visibility `pub(crate)` when shared across scenario modules.

- [ ] **Step 3: Move tests by subject**

Move:

- shared JSON fixtures and digest helpers to `support.rs`.
- attestation and outcome pairing tests to `pairing.rs`.
- request-envelope fallback tests to `fallback.rs`.
- named projection tests from line 510 onward to `named_projection.rs`.

- [ ] **Step 4: Verify**

Run:

```bash
wc -l crates/assay-cli/tests/evidence_test/mcp_execution_records.rs crates/assay-cli/tests/evidence_test/mcp_execution_records/*.rs
cargo fmt --check
cargo test -p assay-cli --test evidence_test mcp_execution_records
cargo clippy -p assay-cli --all-targets -- -D warnings
```

Expected: every file below 600 LOC; facade below 80 LOC.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/assay-cli/tests/evidence_test/mcp_execution_records.rs crates/assay-cli/tests/evidence_test/mcp_execution_records
git commit -m "test(cli): split mcp execution record scenarios"
```

## Task 15: Efficiency And Hygiene Follow-Up

Run this only after Tasks 2-14 land and the generic LOC gate passes.

**Files:**

- Modify only files touched by prior tasks when a measured cleanup is justified.

- [ ] **Step 1: Run final LOC gate**

Run:

```bash
scripts/ci/review-hotspot-loc-under-600.sh
```

Expected: `PASS: no handwritten Rust files at or above 600 LOC`.

- [ ] **Step 2: Run workspace hygiene**

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all pass.

- [ ] **Step 3: Run targeted performance smoke where behavior was near hot paths**

Run:

```bash
cargo bench -p assay-core --bench store_write_heavy --no-run
cargo bench -p assay-cli --bench suite_run_worstcase --no-run
FORENSIC=1 BMF_JSON=1 ./scripts/perf_assess.sh
```

Expected:

- Bench targets compile.
- `perf_assess.sh` completes.
- No material p95 drift is accepted without a separate performance PR.

- [ ] **Step 4: Make only proven cleanup changes**

Allowed cleanup after the split:

- Replace repeated canonicalization or JSON serialization with a shared helper when the helper is already in the new module boundary.
- Remove clones only when ownership is local and tests cover the output shape.
- Replace repeated fixture builders with shared test helpers.
- Narrow `pub` to `pub(crate)` or private when no external module imports it.
- Delete dead comments that only restate the new module name.

Disallowed cleanup in this wave:

- Changing reason-code precedence.
- Changing JSON field names, omission behavior, digest domains, exit codes, or redaction placeholders.
- Adding dependencies.
- Moving unrelated files below 600 just because they are nearby.

- [ ] **Step 5: Final closure**

Update `docs/contributing/REFACTOR-WAVE-STATUS.md` row to:

```markdown
| Wave71 | Hotspot LOC under 600 | final PR line | Closed-loop | All handwritten Rust files are below 600 LOC, generated `vmlinux.rs` excluded, final gate `scripts/ci/review-hotspot-loc-under-600.sh` passes |
```

Commit:

```bash
git add docs/contributing/REFACTOR-WAVE-STATUS.md
git commit -m "docs: close hotspot loc under 600 wave"
```

## Review Gates Per PR

Every implementation PR must include this checklist in the PR body:

```markdown
## Hotspot split checklist

- [ ] Public API/import paths preserved through facade re-exports
- [ ] Wire/JSON/CLI/evidence contracts unchanged
- [ ] Existing tests moved or identified before implementation
- [ ] No dependency changes
- [ ] No behavior cleanup mixed with mechanical move
- [ ] `wc -l` confirms every changed Rust file is below 600 LOC
- [ ] `cargo fmt --check` passes
- [ ] Targeted `cargo test` commands pass
- [ ] Targeted `cargo clippy -p <crate> --all-targets -- -D warnings` passes
- [ ] `git diff --check` passes
```

For subsystem PRs, also run:

```bash
scripts/ci/review-split-wave.sh <crate-name> '<allowed-path-regex>'
```

For the final closure PR, also run:

```bash
scripts/ci/review-hotspot-loc-under-600.sh
```

## Wave Order

Recommended order:

1. Task 1: freeze and LOC gate.
2. Tasks 2-5: MCP server proxy enforcement and proxy E2E tests.
3. Task 6: core tool-decision truth layer.
4. Tasks 7-8: registry supply-chain and Rekor verification.
5. Tasks 9-11: CLI command and verifier facades.
6. Tasks 12-14: runner/evidence/test-only cleanup.
7. Task 15: measured efficiency and hygiene closure.

Do not combine Tasks 2, 3, 6, 7, 8, 12, or 13 into one PR. Those touch security, evidence, or public contract surfaces and need isolated review.

## Self-Review

Spec coverage:

- Hotspots discovered from the live tree, not the stale Q2 inventory.
- LOC target covers all handwritten files at or above 600 LOC and excludes generated `vmlinux.rs`.
- Efficiency, simplicity, elegance, and hygiene are included as a measured follow-up after mechanical splits.
- Assay standing policy is followed: stable facades, behavior freeze, generic split gate, rolling status doc, no per-wave `SPLIT-*` documents.

Placeholder scan:

- No placeholder markers or unspecified module names are used.
- Every task lists exact files, commands, and expected outcomes.

Type consistency:

- Existing public symbols are preserved through facade re-exports.
- Test-only splits use support modules with `pub(crate)` visibility where shared helpers are needed.
