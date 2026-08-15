# MCP Policy-Input Safety Design

**Status:** Revised after written-spec review, awaiting owner reconfirmation
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

The tools do not all parse the same policy dialect. `assay_check_args` loads the full `McpPolicy`
schema, whose canonical name rules are `tools.allow` and `tools.deny` (plus the legacy root-level
`allow` and `deny`). `assay_policy_decide` is a narrower, name-only compatibility surface whose
public input is a root-level `blocklist`. A `blocklist`-only file is therefore not a full
`McpPolicy`. Existing documentation that presents these dialects as one schema is stale.

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

### 0. Policy-Dialect Decision

This programme preserves `assay_policy_decide` as a deliberately narrow 5.x compatibility
surface. Its root-level `blocklist` remains the contract exercised by the published tool schema,
stdio integration tests, and project-install proof. It is not an alias for `McpPolicy.tools.deny`,
and Slice 1 must not make `assay_policy_decide` evaluate `tools.deny`, legacy root `deny`, schemas,
sequence rules, or the full policy engine.

The compatibility boundary must not turn recognised canonical name-policy intent into a clean
allow. A mapping containing root `allow`, root `deny`, or `tools` is an unsupported full-policy
dialect for this tool and returns `E_POLICY_PARSE` with `Policy structure is invalid`. A document
mixing any of those markers with `blocklist` is ambiguous and returns the same failure. Neither
case reaches cache insertion. Mappings that contain no `blocklist` and no canonical name-policy
marker remain allow-compatible for 5.x compatibility.

`assay_check_args` remains the full `McpPolicy` evaluator. Documentation and tool descriptions must
call the distinction explicit: `assay_policy_decide` is a name-only blocklist check, while
`assay_check_args` is the argument-aware full-policy check. Consolidating these dialects would be a
breaking contract decision and is outside this programme.

### 1. Typed Policy-Decision Document

`assay_policy_decide` will deserialize into a private document type equivalent to:

```rust
struct PolicyDecisionDocument {
    #[serde(default)]
    blocklist: Vec<String>,
}
```

The type must not use `deny_unknown_fields`. Existing mapping documents may contain unrelated
policy fields. The states are therefore explicit:

- mapping plus absent `blocklist`: valid, default empty blocklist, allow-compatible;
- mapping plus `blocklist: []`: valid, empty blocklist, allow-compatible;
- mapping plus string sequence: valid and evaluated with exact string membership; wildcard-looking
  strings remain literal;
- mapping plus explicit null, bare `blocklist:`, or any other malformed `blocklist`:
  `E_POLICY_PARSE`;
- mapping plus root `allow`, root `deny`, or `tools`: unsupported canonical dialect,
  `E_POLICY_PARSE`;
- mapping that mixes `blocklist` with one of those canonical markers: ambiguous dialect,
  `E_POLICY_PARSE`;
- scalar, sequence, null, or empty root: `E_POLICY_PARSE`.

Parsing has two explicit stages. A shared `require_mapping_root` check rejects non-mapping roots
with the stable, value-free root summary before typed field deserialization. The private document
then distinguishes absent `blocklist` from a present null or malformed value without reflecting
source text. Only a successfully deserialized blocklist may be inserted into the cache. Invalid
documents return before cache insertion. The cache key and valid cache-hit behaviour remain
unchanged.

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

The summary selection is fixed rather than inferred by each caller:

| Failure class | Public summary |
|---|---|
| YAML/JSON syntax cannot be parsed | `Policy YAML is invalid` |
| parsed document root is not a mapping | `Policy root must be a mapping` |
| mapping has a field with an invalid type or shape | `Policy structure is invalid` |

Truncation alone is insufficient because it can expose a secret prefix. The specialised parse path
must avoid source-derived text. In addition, `ToolError` will apply an absolute 4,096-byte ceiling to
every public `message` as defence in depth. This intentionally widens #2387 beyond policy parse
errors. The ceiling is measured in UTF-8 bytes and must not split a code point.

`ToolError` is a public Rust type whose fields are already public. This programme does not remove
that API in a minor release. Instead, one `bound_public_message` function is used both by
`ToolError::new` and by a custom `Serialize` implementation that replaces the current derive. The
serialization boundary therefore catches direct struct construction and post-construction field
mutation before the server publishes the value. `ToolError::result` must continue to serialize the
type rather than repacking its fields. Safe line and column numbers use the existing `details`
object rather than message text.

The four current raw policy-parse sinks migrate to this shared path. Fixed, value-free errors may
continue to use the general constructor, but no raw parser error may bypass the parse constructor.

### 3. Bounded Policy Reader

Policy-file resource limiting is a separate concern and reason path. A shared asynchronous
`read_policy_bounded` entry point will enforce a configured `max_policy_bytes` before full
materialisation. It uses the existing `assay_common::limits::LimitReader` mechanism around
`std::fs::File` in a blocking task rather than reimplementing the inclusive `limit + 1` rule.

The initial default is 1,000,000 bytes, matching the existing inbound message ceiling while
remaining a separate named configuration value. The environment override is
`ASSAY_MCP_MAX_POLICY_BYTES`. Install and invocation read this single value from `ServerConfig`.

The reader must:

- read at most `limit + 1` bytes rather than trusting metadata alone;
- reject oversized or concurrently growing files with `E_LIMIT_EXCEEDED`;
- preserve existing not-found and read-error classifications;
- never include file contents in the diagnostic; and
- be used by every MCP tool that reads a policy file.

The measured inventory is four direct `tokio::fs::read` call sites in `assay_policy_decide`,
`assay_check_coverage`, `assay_check_sequence`, and `assay_explain_trace`, plus the indirect
`McpPolicy::from_file` / `std::fs::read_to_string` path in `assay_check_args`.

The tools retain their distinct parse dialects after the shared read. `McpPolicy` gains one
bytes-in parse API containing the current deserialize, legacy-normalisation, migration, and
validation sequence. `McpPolicy::from_file` delegates to that API, and `assay_check_args` passes the
bytes returned by `read_policy_bounded` to the same API. A tool may not retain a parallel unbounded
read or a second implementation of the full-policy parse rule.

The server-tool slice does not silently absorb proxy-enforcement startup policies, declared MCP
manifests, trust policies, general CLI policy readers, parser-depth limits, or YAML alias-expansion
limits. Those have different configuration and lifecycle contracts and require separate measured
issues if they remain actionable.

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

Controls must pin mapping-without-blocklist, mapping-with-unrelated-keys, `blocklist: []`, a present
null/bare `blocklist:` parse error, a valid exact-name deny, and a wildcard-looking literal. Root
`allow`, root `deny`, nested `tools`, and mixed `blocklist` plus canonical markers must fail as an
unsupported or ambiguous dialect on first and repeat calls. The malformed-root path already uses
`Policy root must be a mapping`; it must not wait for Slice 2 or expose a large scalar. A mutation
removing `require_mapping_root` must make the malformed-root table fail. Mutations that remove the
dialect guard, switch to wildcard matching, or cache an empty list before returning the error must
fail their corresponding contract assertions.

Update the stale MCP references that present root `blocklist` as part of the full `McpPolicy`
schema. The tool description continues to advertise name-only blocklist semantics and explicitly
routes full-policy and argument-aware evaluation to `assay_check_args`.

### Slice 2: #2387 — Diagnostic Safety

Inventory every public policy-parse sink and migrate all of them in one PR. Tests send distinct
sentinels at the beginning, middle, and end of a large malformed value. Assert that none appears in
the complete MCP response and that `error.message` remains at or below 4,096 UTF-8 bytes. The full
MCP envelope is expected to be larger than the message ceiling.

Add tests for safe syntax location, absolute-path exclusion, policy-root exclusion, multibyte UTF-8,
and all affected tools. Constructor, direct-struct, and post-construction-mutation tests must all
serialize a message of at most 4,096 bytes. Mutations removing the value-free constructor, the
shared bound, or the custom serializer must fail distinct assertions. Verdict fields and reason
code remain unchanged.

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

The #2386 issue body must be amended before implementation: its existing non-claim against
`McpPolicy` unification remains correct, but its DoD must pin the private compatibility dialect,
present-null behaviour, value-free root diagnostic, and documentation correction above.

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
