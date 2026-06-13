# Tool Annotation Conformance Increment 5 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a fourth orthogonal carrier, `assay.tool_annotation_conformance.v0`, that compares untrusted MCP tool annotation hints with Assay's own call classification without affecting enforcement verdicts.

**Architecture:** Increment 5 starts with a pure producer contract: a new `proxy::annotation_conformance` module evaluates `readOnlyHint` and `destructiveHint` against `tool_decision::classify` output and emits a small append-only conformance record. Later slices will capture annotations from `tools/list`, emit the carrier beside the existing per-call carriers, and vendor/consume it in Plimsoll. `assay.enforcement_decision.v0` remains the only verdict carrier.

**Tech Stack:** Rust (`assay-mcp-server`, `serde_json`, Cargo tests), golden JSON fixtures, later Python/Plimsoll vendor freshness guards.

---

## Scope Boundaries

Increment 5a does **not** wire the carrier into the live proxy, does not read `tools/list` annotations yet, and does not change `enforce::decide()` or any allow/deny behavior. It creates only pure logic, tests, a canonical producer fixture, and this plan.

`assay.tool_annotation_conformance.v0` answers: *did untrusted server-declared tool annotation hints contradict Assay's own classified behavior for this call?*

It does **not** answer:

- whether the server is trusted;
- whether a side effect occurred;
- whether the call should be allowed or denied;
- whether descriptions/prompts were malicious;
- whether `idempotentHint` or `openWorldHint` are true.

## Contract Semantics

v0 assesses only axes that a single classified call can honestly compare:

- `readOnlyHint`: `true` contradicts any classified mutating/destructive call.
- `destructiveHint`: only `false` can be contradicted, and only by an observed destructive class. `true` means "may be destructive", so one non-destructive observation cannot refute it.

`idempotentHint` and `openWorldHint` are recorded as declared values when present, but they are not assessed in v0.

Conformance states:

- `undeclared`: no assessed hints were present.
- `consistent`: assessed hints were not contradicted by the observed classification. This is **not** trust certification.
- `mismatched`: at least one assessed hint contradicts observed behavior.
- `inconclusive`: the call was not fully classified, so no conformance claim is made.

Absent hints remain `null`; v0 does not apply MCP schema defaults as if the server declared them.

## Files

Assay producer side:

- Create: `crates/assay-mcp-server/src/proxy/annotation_conformance.rs`
- Modify: `crates/assay-mcp-server/src/proxy/mod.rs`
- Create: `crates/assay-mcp-server/tests/fixtures/tool_annotation_conformance_contract.v0.json`
- Create: `docs/superpowers/plans/2026-06-13-tool-annotation-conformance-increment5.md`

Later slices:

- 5b: capture annotations from `tools/list`, emit `--tool-annotation-conformance-out <path>`, and fail closed on allow if a required record write fails.
- 5c: vendor the producer fixture in Plimsoll, add freshness guard, classifier, summary/findings, and CLI/review wire-in.
- 5d: extend combined carrier acceptance to include conformance as a third independent per-call carrier.

## Task 1: Plan of Record

- [ ] **Step 1: Save this plan**

Write this file to:

```bash
docs/superpowers/plans/2026-06-13-tool-annotation-conformance-increment5.md
```

Expected: plan is private engineering documentation, not public spec positioning.

- [ ] **Step 2: Commit the plan with the producer slice**

The plan should land in the same PR as 5a so reviewers see the bounded claim before the carrier shape.

## Task 2: Pure Producer Module and Unit Tests

**Files:**

- Create: `crates/assay-mcp-server/src/proxy/annotation_conformance.rs`
- Modify: `crates/assay-mcp-server/src/proxy/mod.rs`

- [ ] **Step 1: Write failing tests**

Add tests first for:

- read-only declared true + observed mutating -> `mismatched` / `declared_read_only_observed_mutating`
- non-destructive declared false + observed destructive -> `mismatched` / `declared_non_destructive_observed_destructive`
- no assessed hints -> `undeclared`
- classified additive/mutating + read-only false/destructive false -> `consistent`
- unknown or incomplete classification -> `inconclusive`
- idempotent/open-world are recorded but not assessed
- record contains no verdict/delivery/credential/target fields

Run:

```bash
cargo test -p assay-mcp-server annotation_conformance -- --nocapture
```

Expected before implementation: tests fail because the module does not exist.

- [ ] **Step 2: Implement minimal module**

Create a small API:

```rust
pub const TOOL_ANNOTATION_CONFORMANCE_SCHEMA: &str = "assay.tool_annotation_conformance.v0";

pub struct DeclaredToolAnnotations {
    pub read_only: Option<bool>,
    pub destructive: Option<bool>,
    pub idempotent: Option<bool>,
    pub open_world: Option<bool>,
}

pub fn conformance_for(declared: &DeclaredToolAnnotations, tool_name: &str, args: &serde_json::Value) -> serde_json::Value;
```

The builder calls `tool_decision::classify`, derives `observed.behavior_class`, assesses only read-only/destructive, and emits a carrier record.

- [ ] **Step 3: Verify focused tests pass**

Run:

```bash
cargo test -p assay-mcp-server annotation_conformance -- --nocapture
```

Expected: tests pass.

## Task 3: Canonical Producer Fixture

**Files:**

- Modify: `crates/assay-mcp-server/src/proxy/annotation_conformance.rs`
- Create: `crates/assay-mcp-server/tests/fixtures/tool_annotation_conformance_contract.v0.json`

- [ ] **Step 1: Add golden fixture helpers**

Inside the test module, generate one real record per stable producer state:

- `consistent_read_only_false_additive`
- `consistent_destructive_false_additive`
- `mismatched_read_only_mutating`
- `mismatched_non_destructive_destructive`
- `undeclared`
- `inconclusive_unknown_tool`
- `unassessed_axes_recorded`

- [ ] **Step 2: Regenerate fixture**

Run:

```bash
ASSAY_UPDATE_GOLDEN=1 cargo test -p assay-mcp-server tool_annotation_conformance_contract_fixture
```

Expected: fixture file is written.

- [ ] **Step 3: Verify fixture without update env**

Run:

```bash
cargo test -p assay-mcp-server tool_annotation_conformance_contract_fixture
```

Expected: committed fixture equals generated fixture.

## Task 4: Full Verification and PR

- [ ] **Step 1: Run local verification**

Run:

```bash
cargo test -p assay-mcp-server annotation_conformance
cargo test -p assay-mcp-server --bins
cargo fmt --check
cargo clippy -p assay-mcp-server --all-targets -- -D warnings
git diff --check
```

Expected: all green.

- [ ] **Step 2: Confirm existing carrier fixtures are untouched**

Run:

```bash
git diff -- crates/assay-mcp-server/tests/fixtures/enforcement_decision_contract.v0.json crates/assay-mcp-server/tests/fixtures/manifest_establish_contract.v0.json
```

Expected: no diff.

- [ ] **Step 3: Commit and open PR**

Use a technical commit message:

```bash
git add docs/superpowers/plans/2026-06-13-tool-annotation-conformance-increment5.md \
  crates/assay-mcp-server/src/proxy/mod.rs \
  crates/assay-mcp-server/src/proxy/annotation_conformance.rs \
  crates/assay-mcp-server/tests/fixtures/tool_annotation_conformance_contract.v0.json
git commit -s -m "feat(mcp-server): add tool annotation conformance contract"
git push -u origin codex/tool-annotation-conformance
gh pr create --title "feat(mcp-server): add tool annotation conformance contract" --body "..."
```

Expected: PR is ready for review; no live proxy behavior changes.
