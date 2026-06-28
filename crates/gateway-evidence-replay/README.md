# Gateway Evidence Replay

`gateway-evidence-replay` is a small offline verifier for `gateway-path.v0` evidence bundles.

It replays retained gateway-path facts: requested route, disclosed fallback, endpoint, policy hash, stream commitment, coverage, source class, and freshness of gateway evidence. It emits a bounded verdict:

- `path_verified`
- `path_mismatch`
- `incomplete`
- `invalid`

Example:

```bash
gateway-evidence-replay verify fixtures/gateway-path-v0/clean-route.json --format gateway-path.v0 --json
```

This crate does not run a gateway, call a provider, verify a TEE root, or decide response truth. Signature verification and runtime-measurement verification are input facts in the evidence bundle. The verifier recomputes only whether the retained evidence is sufficient to support the gateway-path claim.
