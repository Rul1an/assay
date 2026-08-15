# MCP Policy-Input Safety Design

**Status:** Approved design, awaiting written-spec review
**Programme ledger:** [#2388](https://github.com/Rul1an/assay/issues/2388)
**Baseline:** `addfda3b3f695f11a7dba90f7c1ae24ff442b034` (`origin/main`, 2026-08-15)

## Purpose

Close the MCP policy-input safety gap in three independently reviewable slices:

1. reject structurally invalid policy roots before they can become cached allows;
2. publish bounded, value-free policy-parse diagnostics through one shared boundary; and
3. enforce a policy-file byte ceiling before materialisation.

The sequence is deliberate. Verdict correctness lands first. Diagnostic policy then changes without
being mixed with verdict semantics. Resource limiting lands last through a separate shared reader.

## Measured Baseline

Two independent stdio audits on the baseline SHA established:

- scalar, sequence, and empty YAML roots return `allowed:true` and `isError:false`;
- the first malformed-root call logs a blocklist cache miss and inserts an empty list, while the
  repeat call is a cache hit;
- a malformed 200,000-byte scalar produces approximately 200,044 to 200,071 bytes of public
  `E_POLICY_PARSE` message text in four tools;
- `assay_policy_decide`, `assay_check_args`, `assay_check_coverage`, and `assay_explain_trace`
  expose raw parser diagnostics, while `assay_check_sequence` already uses fixed wording; and
- policy files are fully materialised by the affected tools outside the inbound JSON-RPC message
  ceiling.

The reproduction is local-policy evidence. It does not demonstrate remote exploitability,
`proxy-enforce` bypass, target-tool execution, or an external side effect.

## Standards And Design Basis

The MCP tools contract requires tool outputs to be sanitised. Tool execution failures remain tool
results with `isError:true`; malformed protocol messages remain JSON-RPC errors. This design keeps
that boundary and does not adopt an HTTP Problem Details wire format.

RFC 9457 supplies a useful field taxonomy by analogy: stable machine code, stable short summary,
bounded occurrence detail, and optional structured context. Consumers must not parse occurrence
text. It does not require a generic retryability field, and this design adds none.

MCP 2026-07-28 is a stateless, per-request protocol revision. Its `resultType`, discovery, and
cacheable-result semantics are separate from Assay's internal parsed-policy cache. This programme
does not implement or advertise that revision.

## Architecture

### 1. Typed Policy-Decision Document

`assay_policy_decide` will deserialize into a private document type equivalent to:

```rust
struct PolicyDecisionDocument {
    blocklist: Option<Vec<String>>,
}
```

The type must not use `deny_unknown_fields`. Existing mapping documents may contain unrelated
policy fields. The states are therefore explicit:

- mapping plus absent `blocklist`: valid, empty blocklist, allow-compatible;
- mapping plus `blocklist: []`: valid, empty blocklist, allow-compatible;
- mapping plus string sequence: valid and evaluated;
- mapping plus malformed `blocklist`: `E_POLICY_PARSE`;
- scalar, sequence, null, or empty root: `E_POLICY_PARSE`.

Only a successfully deserialized blocklist may be inserted into the cache. Invalid documents return
before cache insertion. The cache key and valid cache-hit behaviour remain unchanged.

### 2. Public Parse-Diagnostic Boundary

Raw `Display` output from YAML, JSON, or typed deserialization is not a public compatibility
contract. All public policy-parse failures will use one shared constructor or function.

The public error contains:

- code `E_POLICY_PARSE`;
- one short, stable, value-free summary selected from `Policy YAML is invalid`,
  `Policy root must be a mapping`, or `Policy structure is invalid`;
- optional structured line and column values when the parser provides them safely; and
- no raw policy fragment, secret sentinel, policy-root path, absolute path, debug chain, or parser
  implementation wording.

Truncation alone is insufficient because it can expose a secret prefix. The specialised parse path
must avoid source-derived text. In addition, `ToolError` will apply an absolute 4,096-byte ceiling to
every public message as defence in depth. The ceiling is measured in UTF-8 bytes and must not split
a code point.

The four current raw policy-parse sinks migrate to this shared path. Fixed, value-free errors may
continue to use the general constructor, but no raw parser error may bypass the parse constructor.

### 3. Bounded Policy Reader

Policy-file resource limiting is a separate concern and reason path. A shared asynchronous reader
will enforce a configured `max_policy_bytes` before full materialisation.

The initial default is 1,000,000 bytes, matching the existing inbound message ceiling while
remaining a separate named configuration value. The environment override is
`ASSAY_MCP_MAX_POLICY_BYTES`. Install and invocation read this single value from `ServerConfig`.

The reader must:

- read at most `limit + 1` bytes rather than trusting metadata alone;
- reject oversized or concurrently growing files with `E_LIMIT_EXCEEDED`;
- preserve existing not-found and read-error classifications;
- never include file contents in the diagnostic; and
- be used by every MCP tool that reads a policy file.

The implementation plan must first pin the full reader call-site inventory. A tool may not retain a
parallel unbounded implementation.

## Data And Error Flow

```text
tools/call arguments
  -> bounded path resolution
  -> shared bounded policy reader
     -> not found/read/limit error (no parse, no cache)
  -> typed or tool-specific parse through shared public diagnostic policy
     -> E_POLICY_PARSE (no cache)
  -> successful compiled representation
     -> cache insert
  -> policy observation
     -> allowed true/false result
```

Absence of a valid parse, unavailable infrastructure, or a resource-limit failure never becomes a
clean allow.

## Delivery Slices

### Slice 1: #2386 — Root Shape And Cache Safety

Write RED real-stdio tests before production changes. Cover scalar, sequence, empty/null, and a
deny-shaped root sequence. First and repeat calls must return `allowed:false`, `E_POLICY_PARSE`, and
`isError:true`.

Controls must pin mapping-without-blocklist, mapping-with-unrelated-keys, `blocklist: []`, and a
valid deny. A mutation removing typed root validation must make the malformed-root table fail. A
mutation that caches an empty list before returning the error must make the repeat-call assertion
fail.

### Slice 2: #2387 — Diagnostic Safety

Inventory every public policy-parse sink and migrate all of them in one PR. Tests send distinct
sentinels at the beginning, middle, and end of a large malformed value. Assert that none appears in
the complete MCP response and that the response remains below the named ceiling.

Add tests for safe syntax location, absolute-path exclusion, policy-root exclusion, multibyte UTF-8,
and all affected tools. Mutations removing the value-free constructor or general ceiling must fail.
Verdict fields and reason code remain unchanged.

### Slice 3: Bounded Policy Ingest

Create a child issue with the final call-site inventory before implementation. RED tests cover a
sparse oversized file, an exact-limit file, a limit-plus-one file, and a reader that observes growth
while reading. Oversized input returns `E_LIMIT_EXCEEDED` and never reaches parsing or cache
insertion.

Mutation tests must catch a metadata-only size check and a direct `tokio::fs::read` bypass. Valid
files at or below the limit preserve current behaviour.

## Verification Per Slice

Before each push:

- run the focused RED/GREEN contract test;
- run the affected `assay-mcp-server` integration and crate tests;
- run `cargo fmt --all -- --check`;
- run `cargo clippy -p assay-mcp-server --all-targets -- -D warnings`;
- run `git diff --check` and inspect public strings; and
- record exact SHA, worktree, toolchain, tests, mutations, non-claims, and findings in #2388.

GitHub Actions remains the integration proof. Each final PR head requires one non-building-agent
review under `AGENTS.md`. Agents that authored this design or an implementation slice cannot supply
that slice's quorum review.

## Issue Disposition

- #2386 and #2387 remain P1 children of #2388 until their merge evidence is recorded.
- Slice 3 receives a new child issue after its inventory is measured.
- #2385 remains closed as the correct baseline for malformed present `blocklist` values.
- #2359 and #2384 form the next adoption and packaging cluster.
- #2185 remains wire observability and is measured per protocol era when #2358 advances.
- #2358 requires a separate stateless MCP 2026-07-28 design.
- #2177 remains a failure-envelope epic delivered one failure family per PR.
- #2178 is not scheduled without a demonstrated blocked workflow.

## Non-Claims

This programme does not add target-tool enforcement, proxy unification, modern MCP negotiation,
remote transport, OAuth, marketplace approval, portable MCP packaging, generic retryability,
provider-outcome verification, compliance, certification, a scalar trust score, or a safe-agent
claim.

## Primary References

- MCP 2026-07-28 release: <https://blog.modelcontextprotocol.io/posts/2026-07-28/>
- MCP tools: <https://modelcontextprotocol.io/specification/2026-07-28/server/tools>
- MCP caching: <https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/caching>
- RFC 9457: <https://www.rfc-editor.org/rfc/rfc9457.html>
- JSON-RPC 2.0: <https://www.jsonrpc.org/specification>
