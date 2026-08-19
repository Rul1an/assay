---
title: MCP 2025/2026 Protocol-Era Parity
description: Reproduce Assay's pinned MCP 2025-06-18 and 2026-07-28 resultType parity corpus for complete, input_required, and interim tool results.
---

# MCP Protocol-Era Parity

Assay carries an executable, exploratory corpus for one narrow compatibility question across MCP
`2025-06-18` and `2026-07-28`:

> When otherwise equivalent privileged tool calls use the result framing of different protocol
> eras, does Assay preserve the same evidence interpretation without reading an interim result as
> completed?

The corpus keeps schema acceptance, wire observation, semantic conclusion, and the surrounding
privileged-action evidence baseline separate. Those layers can disagree on purpose. A schema-valid
message is evidence about its shape, not proof that a tool action completed or had an external
effect.

## Reproduce The Pinned Run

The following snapshot includes all implementation slices behind this note:

```bash
git clone https://github.com/Rul1an/assay.git
cd assay
git checkout --detach 5e2203e183c6630101f4c6d356cdd7c465ff1364
cargo test --locked -p assay-core mcp::era_parity_tests --lib
```

Expected result:

```text
test result: ok. 38 passed; 0 failed; 0 ignored
```

The schemas are vendored and digest-checked before use, so the test does not fetch a moving MCP
schema. Cargo may still need to download Rust dependencies on a fresh machine.

## What The Run Establishes

The fixed corpus exercises these boundaries:

| Case | Bounded conclusion |
|---|---|
| Equivalent 2025 and 2026 calls | Same referenced profile baseline and equivalent result conclusion |
| 2025 result without `resultType` | Terminal under the legacy compatibility rule |
| 2026 `complete` result without a non-null `inputRequests` or `requestState` | Terminal |
| 2026 `complete` result with any non-null `inputRequests` or `requestState` | Incomplete because completion contradicts the continuation |
| 2026 `input_required` with a string `requestState` or object `inputRequests`, and no malformed sibling | Valid but non-terminal |
| 2026 `input_required` with neither such member, or with a non-null member of another top-level JSON type | Invalid rather than an ordinary unfinished call; `null` is treated as absent |
| 2026 result without `resultType` | Invalid once the era is known |
| Present but unrecognized result token | Incomplete when capabilities are readable, absent, or unavailable; malformed modern capabilities are invalid before recognition |
| Unknown protocol era | No terminal conclusion |
| Conflicting header and request-metadata eras | Refused distinctly from an unknown era |
| Multi-hop interim flow | Separate records; no pairing or eventual completion is inferred |
| `traceparent` present or absent | Correlation only; no conclusion changes |

The load-bearing parity check references the same frozen four-cell profile row for equivalent calls.
It does not derive that profile row from the transcript. In particular, wire bytes do not establish
that Assay recorded a policy decision, that a caller observed a denial, that an upstream accepted a
request, or that an external side effect occurred.

## Pinned Evidence

- Assay implementation snapshot:
  [`5e2203e183c6630101f4c6d356cdd7c465ff1364`](https://github.com/Rul1an/assay/commit/5e2203e183c6630101f4c6d356cdd7c465ff1364)
- Corpus contract and maturity statement:
  [`mcp-era-parity-v0/README.md`](https://github.com/Rul1an/assay/blob/5e2203e183c6630101f4c6d356cdd7c465ff1364/crates/assay-core/tests/fixtures/mcp-era-parity-v0/README.md)
- Machine-readable vectors:
  [`MANIFEST.json`](https://github.com/Rul1an/assay/blob/5e2203e183c6630101f4c6d356cdd7c465ff1364/crates/assay-core/tests/fixtures/mcp-era-parity-v0/MANIFEST.json)
- MCP source pin and vendored schema digests:
  [`PIN.json`](https://github.com/Rul1an/assay/blob/5e2203e183c6630101f4c6d356cdd7c465ff1364/crates/assay-core/tests/fixtures/mcp-era-parity-v0/PIN.json)
- Canonical MCP specification commit:
  [`5f5440bb26a62e2cf3440b92da5a667efa03b267`](https://github.com/modelcontextprotocol/modelcontextprotocol/commit/5f5440bb26a62e2cf3440b92da5a667efa03b267)
- Upstream reference-lane definition and pins:
  [`mcp-upstream-reference.yml`](https://github.com/Rul1an/assay/blob/f31f839e08b79205954aa5a85650295b06497eba/.github/workflows/mcp-upstream-reference.yml)

Assay also runs a separate, pinned upstream reference lane for the official MCP conformance source
and Rust SDK. That lane checks named upstream scenarios and source integrity. It does not represent
Assay as an MCP client or server and does not turn this corpus into a broad conformance claim.

## 2026-07-28 Wire-Model Ownership

The vendored `schema-2026-07-28.json` digest in `PIN.json` is the final schema pin used by the
wire-model tests: MCP specification commit
`5f5440bb26a62e2cf3440b92da5a667efa03b267`. The model validates only the required request
metadata, `resultType`, `CacheableResult` hints, `server/discover` shapes, and the reserved
`-32022` error shape. It does not accept a request, negotiate a protocol version, or produce a
`server/discover` response.

Assay's custom codec remains the single production owner. The following is an ownership comparison,
not an assertion that the two implementations have matching runtime behavior:

| Concern | Assay custom codec | Official Rust SDK lane |
|---|---|---|
| Message ceilings | Assay applies its own bounded parsing and public-message ceilings. | Reference-only upstream build; no Assay message-ceiling behavior is imported. |
| Timeouts | Assay applies its configured tool timeout. | Reference scenarios do not own Assay tool execution timeouts. |
| Stdio authentication | Assay rejects unsupported `ASSAY_AUTH_*` stdio configuration before protocol I/O. | Reference-only; it does not configure Assay authentication behavior. |
| Policy root | Assay canonicalizes and confines `--policy-root`. | No Assay policy-root semantics. |
| Five tools | Assay owns `assay_check_args`, `assay_check_sequence`, `assay_policy_decide`, `assay_check_coverage`, and `assay_explain_trace`. | No Assay tool surface or dispatch. |

The official Rust SDK remains a pinned upstream reference in
[`mcp-upstream-reference.yml`](../../.github/workflows/mcp-upstream-reference.yml), not a
production dependency. There is no dual implementation: Assay's codec owns the production model.
## Closed 2026-07-28 server adapter

`assay-mcp-server` compiles a complete stateless adapter for the pinned revision: per-request
`_meta` validation, `server/discover`, deterministic `tools/list` with `ttlMs=0` /
`cacheScope=private`, and the five release tools with modern result metadata. The public stdio
loop does not call that adapter. A client still cannot obtain a modern result: `_meta: 2026-07-28`
stays `-32022` with the legacy supported set, `server/discover` stays `-32601`, and initialize
still falls back to `2025-11-25`. This is not negotiation and not an advertisement that the
revision is served.

## Claim Ceiling

This run supports a bounded statement: Assay preserves the measured privileged-action evidence
interpretation across the pinned 2025 and 2026 cases, and does not read the measured interim results
as terminal.

It does **not** establish:

- general MCP protocol conformance;
- SDK certification or interoperability across all implementations;
- HTTP, OAuth, Tasks, Apps, session, or identity support;
- eventual completion of an interim result;
- upstream delivery or an external side effect;
- a whole-action verdict, scalar trust score, or compliance claim.

The corpus remains **exploratory**. It is intentionally lower-maturity than the frozen
Privileged MCP Action conformance corpus and carries no independent reproduction request.
