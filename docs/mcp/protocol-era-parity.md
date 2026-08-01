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
  [`mcp-upstream-reference.yml`](https://github.com/Rul1an/assay/blob/5e2203e183c6630101f4c6d356cdd7c465ff1364/.github/workflows/mcp-upstream-reference.yml)

Assay also runs a separate, pinned upstream reference lane for the official MCP conformance source
and Rust SDK. That lane checks named upstream scenarios and source integrity. It does not represent
Assay as an MCP client or server and does not turn this corpus into a broad conformance claim.

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
