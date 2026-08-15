# MCP Policy Decision Safety Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close #2386 by making `assay_policy_decide` reject malformed roots, canonical name-policy
documents, and mixed dialects before cache insertion while preserving its exact-name 5.x
`blocklist` compatibility contract.

**Architecture:** Parse YAML once into `serde_json::Value`, require an object root, classify the
private blocklist dialect before typed field deserialization, and insert only a successfully parsed
`Vec<String>` into the existing digest cache. Keep exact membership and route full-policy callers to
`assay_check_args`; do not import the full policy engine into this tool.

**Tech Stack:** Rust 1.96, Tokio, serde/serde_yaml/serde_json, real stdio JSON-RPC integration tests,
pre-commit, GitHub Actions.

## Global Constraints

- Do not create the writer branch until PR #2390 has merged. Baseline is the resulting exact
  `origin/main` SHA; record it before creating the branch.
- Implement in one isolated `cursor/2386-policy-decision-safety` worktree owned by Cursor.
- Root `blocklist` uses exact `Vec<String>::contains`; wildcard-looking strings remain literal.
- Root `allow`, root `deny`, `tools`, or a mixture with `blocklist` returns `E_POLICY_PARSE`.
- Non-mapping roots return exactly `Policy root must be a mapping` without source-derived text.
- Unsupported, ambiguous, and malformed field shapes return exactly `Policy structure is invalid`.
- Invalid input returns before cache insertion; first and repeat calls must both fail.
- Do not change `McpPolicy`, proxy enforcement, schemas, sequence semantics, or target-tool behavior.
- Stage only named paths; never use a whole-tree staging command.

---

## File Structure

- Modify `crates/assay-mcp-server/src/tools/policy_decide.rs`: private dialect parser and cache-safe call path.
- Modify `crates/assay-mcp-server/tests/policy_decide_blocklist.rs`: driven contract table and cache assertions.
- Modify `docs/reference/mcp-api.md`: distinguish name-only blocklist from full-policy evaluation.
- Modify `docs/AIcontext/entry-points.md`: stop presenting root `blocklist` as full `McpPolicy`.
- Modify `docs/AIcontext/quick-reference.md`: split the mixed policy example into explicit dialects.
- Modify `docs/mcp/self-correction.md`: align stale `assay_policy_decide` input/output claims.
- Modify `docs/use-cases/self-correction.md`: replace the stale combined-check request/response.
- Modify `CHANGELOG.md`: record the intentional rejection and migration path.

### Task 1: Freeze malformed-root and dialect behavior

**Files:**
- Modify: `crates/assay-mcp-server/tests/policy_decide_blocklist.rs`

**Interfaces:**
- Consumes: existing `spawn_server`, `initialize`, `call_policy_decide`, `assert_parse_error`, and `assert_allowed` helpers.
- Produces: a real-stdio test table that distinguishes root shape, field shape, dialect, matching, and repeat-call cache behavior.

- [ ] **Step 1: Add exact parse-message assertions**

Extend `assert_parse_error` with an expected message argument and assert:

```rust
assert_eq!(
    body.pointer("/error/message").and_then(Value::as_str),
    Some(expected_message),
    "{label}: wrong stable parse summary; body={body}"
);
```

Keep the existing `allowed:false`, `E_POLICY_PARSE`, and MCP `isError:true` assertions.

- [ ] **Step 2: Add malformed-root RED fixtures**

Write scalar, deny-shaped sequence (`- dangerous_tool`), null-document, empty-document, and
200,000-byte scalar-root files. Use fixture names such as `root-null-document.yaml` and
`root-empty-document.yaml` so they cannot collide with the existing #2385 field-shape fixtures.
Construct the large root as one syntactically valid YAML scalar containing `BEGIN_SECRET`, filler,
and `END_SECRET`; do not accidentally exercise the YAML-syntax branch. For first and repeat calls
assert `Policy root must be a mapping`; assert the response contains neither sentinel.

- [ ] **Step 3: Add canonical and mixed dialect RED fixtures**

Use this table:

```rust
let unsupported_dialects = [
    ("root-allow.yaml", "allow:\n  - dangerous_tool\n"),
    ("root-deny.yaml", "deny:\n  - dangerous_tool\n"),
    ("tools-deny.yaml", "tools:\n  deny:\n    - dangerous_tool\n"),
    ("mixed-root-deny.yaml", "blocklist:\n  - dangerous_tool\ndeny:\n  - other_tool\n"),
    ("mixed-tools.yaml", "blocklist:\n  - dangerous_tool\ntools:\n  deny:\n    - other_tool\n"),
];
```

For first and repeat calls assert `Policy structure is invalid` and the existing error envelope.

- [ ] **Step 4: Add compatibility controls**

Pin all of these:

```text
version: 1                                  -> allow
name: metadata-only                        -> allow
blocklist: []                              -> allow
blocklist: [dangerous_tool]                -> deny dangerous_tool
blocklist: ["dangerous_*"]                -> allow dangerous_tool
blocklist: ["dangerous_*"]                -> deny a literal tool named dangerous_*
blocklist: null                            -> Policy structure is invalid
blocklist:                                 -> Policy structure is invalid
blocklist: [dangerous_tool, 7]             -> Policy structure is invalid
```

Add a helper parameter for the proposed tool name so the literal-wildcard control uses the same server connection.

- [ ] **Step 5: Run the focused RED test**

```bash
cargo test --locked -p assay-mcp-server --test policy_decide_blocklist -- --nocapture
```

Expected: existing #2385 field-shape cases, including present `blocklist: null`, still pass.
Malformed roots and canonical mappings fail RED because they currently produce clean allows. Mixed
dialects fail RED when they deny or allow instead of returning the required structure error. Record
the discriminating assertion names in #2388.

- [ ] **Step 6: Commit only the RED test**

```bash
git add -- crates/assay-mcp-server/tests/policy_decide_blocklist.rs
git commit -m "test(mcp): expose policy_decide root and dialect fail-open"
```

### Task 2: Implement the private dialect parser

**Files:**
- Modify: `crates/assay-mcp-server/src/tools/policy_decide.rs`

**Interfaces:**
- Consumes: YAML bytes and existing `ToolError`/cache types.
- Produces: `fn parse_policy_decision_document(bytes: &[u8]) -> Result<Vec<String>, ToolError>` and private `PolicyDecisionDocument`.

- [ ] **Step 1: Add the typed private document**

```rust
#[derive(serde::Deserialize)]
struct PolicyDecisionDocument {
    #[serde(default)]
    blocklist: Vec<String>,
}
```

Do not add `deny_unknown_fields`; metadata-only mappings remain compatible.

- [ ] **Step 2: Add root and dialect classification**

Implement one parser with this order:

```rust
fn parse_policy_decision_document(bytes: &[u8]) -> Result<Vec<String>, ToolError> {
    let root: Value = serde_yaml::from_slice(bytes)
        .map_err(|_| ToolError::new("E_POLICY_PARSE", "Policy YAML is invalid"))?;
    let object = root.as_object().ok_or_else(|| {
        ToolError::new("E_POLICY_PARSE", "Policy root must be a mapping")
    })?;

    if ["allow", "deny", "tools"]
        .iter()
        .any(|key| object.contains_key(*key))
    {
        return Err(ToolError::new("E_POLICY_PARSE", "Policy structure is invalid"));
    }

    serde_json::from_value::<PolicyDecisionDocument>(root)
        .map(|document| document.blocklist)
        .map_err(|_| ToolError::new("E_POLICY_PARSE", "Policy structure is invalid"))
}
```

The canonical marker check runs whether or not `blocklist` is present, so mixed dialects fail through the same branch.

- [ ] **Step 3: Replace the ad-hoc extraction**

In the cache-miss branch call only:

```rust
let list = match parse_policy_decision_document(&policy_bytes) {
    Ok(list) => list,
    Err(error) => return error.result(),
};
```

Construct and insert the `Arc<Vec<String>>` only after this returns `Ok`. Keep the SHA/cache key and exact `contains` evaluation unchanged.

- [ ] **Step 4: Run GREEN tests**

```bash
cargo test --locked -p assay-mcp-server --test policy_decide_blocklist -- --nocapture
cargo test --locked -p assay-mcp-server --test stdio_edge_cases
cargo test --locked -p assay-mcp-server --test project_install_surfaces
```

Expected: all pass. The large root response contains neither sentinel and remains a stable root summary.

- [ ] **Step 5: Run discriminating mutations**

In disposable copies, separately remove root validation, remove the canonical marker guard, replace
exact membership with wildcard matching, and insert the cache before validation. Run the focused
test after each. Each mutation must fail its own table/control. Restore from `HEAD` after every
mutation and verify only intended files remain changed.

- [ ] **Step 6: Commit the minimal implementation**

```bash
git add -- crates/assay-mcp-server/src/tools/policy_decide.rs
git commit -m "fix(mcp): reject invalid policy_decide dialects before cache"
```

### Task 3: Correct the outward contract

**Files:**
- Modify: `docs/reference/mcp-api.md`
- Modify: `docs/AIcontext/entry-points.md`
- Modify: `docs/AIcontext/quick-reference.md`
- Modify: `docs/mcp/self-correction.md`
- Modify: `docs/use-cases/self-correction.md`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: the landed tool schema wording in `tools/mod.rs` and implemented exact-name behavior.
- Produces: one consistent public explanation of the two policy dialects.

- [ ] **Step 1: State the name-only contract**

Use this normative wording:

```text
assay_policy_decide performs an exact-name check against its compatibility-only root `blocklist`.
It does not parse full `McpPolicy` controls such as `tools.allow` or `tools.deny`; use
assay_check_args for full, argument-aware policy evaluation. Passing canonical name-policy fields
to assay_policy_decide is an error, not a clean allow.
```

- [ ] **Step 2: Split mixed examples**

Any example combining root `blocklist` with `tools`, `schemas`, root `allow`, or root `deny` becomes
two explicitly labelled examples. This is documentation cleanup: `schemas` is not a dialect marker
and must not be added to the parser's marker list. Do not silently rewrite the compatibility dialect
into `tools.deny`.

- [ ] **Step 3: Record the intentional hardening change**

Add a changelog entry and a migration example. State that canonical or mixed policy documents may
previously have returned a false clean allow or ignored canonical fields; after this change they
return `E_POLICY_PARSE`. Route full-policy callers to `assay_check_args`. Do not call this input class
"unsupported with no impact" because the CLI accepted it before this slice.

- [ ] **Step 4: Run documentation guards**

```bash
pre-commit run --files \
  docs/reference/mcp-api.md \
  docs/AIcontext/entry-points.md \
  docs/AIcontext/quick-reference.md \
  docs/mcp/self-correction.md \
  docs/use-cases/self-correction.md \
  CHANGELOG.md
rg -n "assay_policy_decide|blocklist|tools\.deny" \
  docs/reference/mcp-api.md \
  docs/AIcontext/entry-points.md \
  docs/AIcontext/quick-reference.md \
  docs/mcp/self-correction.md \
  docs/use-cases/self-correction.md \
  CHANGELOG.md
```

Expected: hooks pass; every hit is consistent with the two-dialect decision.

- [ ] **Step 5: Commit documentation**

```bash
git add -- \
  docs/reference/mcp-api.md \
  docs/AIcontext/entry-points.md \
  docs/AIcontext/quick-reference.md \
  docs/mcp/self-correction.md \
  docs/use-cases/self-correction.md \
  CHANGELOG.md
git commit -m "docs(mcp): distinguish blocklist and full policy evaluation"
```

### Task 4: Verify and publish the slice

**Files:**
- Verify only; no new production files.

**Interfaces:**
- Produces: exact-head verification and PR review packet for #2386/#2388.

- [ ] **Step 1: Run the affected suite**

```bash
cargo test --locked -p assay-mcp-server --test policy_decide_blocklist -- --nocapture
cargo test --locked -p assay-mcp-server
cargo fmt --all -- --check
cargo clippy -p assay-mcp-server --all-targets -- -D warnings
git diff --check
git status --short
```

Expected: all pass; only intended committed changes exist.

- [ ] **Step 2: Inspect public strings and diff scope**

```bash
git diff --stat origin/main...HEAD
git diff origin/main...HEAD -- \
  crates/assay-mcp-server/src/tools/policy_decide.rs \
  crates/assay-mcp-server/tests/policy_decide_blocklist.rs \
  docs/reference/mcp-api.md \
  docs/AIcontext/entry-points.md \
  docs/AIcontext/quick-reference.md \
  docs/mcp/self-correction.md \
  docs/use-cases/self-correction.md \
  CHANGELOG.md
```

Confirm no full-policy evaluation, wildcard matcher, proxy change, or raw source-derived parse text.

- [ ] **Step 3: Push and open the PR**

Push `cursor/2386-policy-decision-safety`, open a PR to current `main`, and record branch, PR, exact
head SHA, tests, mutation evidence, non-claims, and open findings in #2388.

- [ ] **Step 4: Obtain exact-head quorum**

Use one fresh non-building reviewer who authored neither this plan/spec nor the implementation.
Fix or technically disposition all findings. Any push invalidates the prior review. Enable
auto-merge only after required checks, exact-head review, and any delegated proof are valid.
