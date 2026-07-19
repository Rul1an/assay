# Gateway Evidence Replay Gate-D Demo

Run this demo when deciding whether `gateway-evidence-replay` should move from standalone MVP to Assay facts-only capture.

Opening line:

> Here are four retained gateway evidence bundles. We do not ask you to trust the gateway, the provider, or an LLM summary. We replay the bytes and produce a bounded verdict.

## Cases

| Case | Fixture | Verdict | What it proves |
| --- | --- | --- | --- |
| A | `clean-route.json` | `path_verified` | Clean retained evidence can confirm a path at the source-class ceiling. |
| B | `partial-route-substitution.json` | `path_mismatch` | Partial coverage can refute but never confirm. |
| C | `stale-attestation.json` | `incomplete` | Integrity-looking evidence is not enough when freshness is stale. |
| D | `unknown-source.json` | `invalid` | Unknown provenance fails closed before content is trusted. |

## Run

The verifier lives in its own repository, [Rul1an/gateway-evidence-replay](https://github.com/Rul1an/gateway-evidence-replay). It is deliberately not vendored here: a replay verifier is only worth anything if every copy computes the same verdict, and two buildable copies cannot guarantee that.

```bash
git clone https://github.com/Rul1an/gateway-evidence-replay && cd gateway-evidence-replay
cargo test --locked --test demo_fixtures --test demo_tamper
cargo run -- verify fixtures/gateway-path-v0/demo/clean-route.json --format gateway-path.v0 --json
cargo run -- replay-pack fixtures/gateway-path-v0/demo --json
```

The demo is digest-pinned:

- `manifest.json` pins every fixture and `expected.json`.
- `manifest-sha256.txt` pins `manifest.json`.
- `demo_tamper.rs` proves a fixture mutation, expected-verdict mutation, manifest mutation, or replay mismatch fails closed.

Closing line:

> This does not prove provider honesty or response truth. It proves the retained gateway-path evidence is, or is not, sufficient for this bounded replay claim.

## Decision Checklist

1. Who would produce the retained gateway-path evidence?
2. Who would consume the replay verdict?
3. Which source class is realistic for the first capture path?
4. Is the value the replay verdict, or is the team actually asking for enforcement?
5. Is the next step facts-only capture, or should the MVP remain a standalone lab tool?

Only question 5 can unlock capture, and only if questions 1 and 2 name a concrete producer and consumer.
