# ADR-024: Sim Engine Hardening (Limits + Time Budget)

## Status

Proposed (February 2026)

## Context

The `assay sim` attack simulation suite ([ROADMAP §G](../ROADMAP.md#g-sim-engine-hardening-p2)) validates that evidence bundle verification correctly blocks integrity attacks (bitflip, truncate, inject, zip bomb, etc.). Per OWASP and resource-control best practices:

1. **Configurable limits**: Users and CI must be able to override verification limits (e.g. stricter `max_bundle_bytes` for constrained environments).
2. **Time budget**: Suite already has `TimeBudget` (60s default), but it is hardcoded; configurable budget supports predictable CI behavior.
3. **Limits coverage**: Integrity attacks currently use `verify_bundle` (default limits). They should use `verify_bundle_with_limits` so that limit-based blocks are exercised and distinguishable from structural/integrity blocks.
4. **Regression-proof limits test**: The zip bomb attack creates ~1.1GB; if limits were relaxed, it could bypass. A dynamic `limit + 1` bytes attack ensures limits are actually enforced.

Current gaps:

- `SuiteConfig.verify_limits` exists but is never passed to attacks (CLI has `// TODO(sim-verify-limits)`)
- No `--limits` or `--limits-file` CLI flags
- `TimeBudget` is fixed at 60s; no `--time-budget` override
- "Blocked by verify limits" is not distinguishable from "Blocked by integrity check" in attack results

## Decision

### 1. CLI: Limits and Budget Flags

Add to `assay sim run`:

| Flag | Type | Description |
|------|------|-------------|
| `--limits` | JSON string | Partial `VerifyLimits` overrides. Merged with defaults. |
| `--limits-file` | Path | Path to JSON file with limits. Overrides `--limits` if both given. |
| `--time-budget` | Seconds | Suite time budget. Default: 60. |

**Parse rules:**

- Invalid JSON → exit code 3 (config error)
- Unknown keys in `--limits` JSON → reject with clear error
- `--limits-file` path missing → exit 3
- Schema: subset of `VerifyLimits` fields; partial merge (only provided fields override)

**Example:**

```bash
assay sim run --suite quick --target bundle.tar.gz
assay sim run --suite quick --limits '{"max_bundle_bytes": 10485760}' --target bundle.tar.gz
assay sim run --suite quick --limits-file .assay/sim-limits.json --target bundle.tar.gz
assay sim run --suite quick --time-budget 120 --target bundle.tar.gz
```

### 2. Limits Model

- **Source**: `assay_evidence::VerifyLimits` (existing struct)
- **Merge**: Start with `VerifyLimits::default()`, then apply overrides from `--limits` or `--limits-file`
- **JSON schema** (informative): Same field names as `VerifyLimits`; only override provided keys
- **Stable schema**: Document in ADR; breaking changes require version bump

### 3. Integrity Attacks: Pass Limits and Budget

- `check_integrity_attacks` receives `VerifyLimits` and `TimeBudget`
- Use `verify_bundle_with_limits(cursor, limits)` instead of `verify_bundle`
- Before each attack iteration: `if budget.exceeded() { skip remaining; report time_budget }`
- **Exit semantics**: Attack blocked by `LimitBundleBytes` / `LimitDecodeBytes` etc. → `AttackStatus::Blocked` (correct). Not a bypass.

### 4. Dynamic `bundle_size` Attack (Regression-Proof)

Add integrity attack:

- **Name**: `integrity.limit_bundle_bytes`
- **Behavior**: Generate bundle of `limits.max_bundle_bytes + 1` bytes (compressed). Verification must fail with `LimitBundleBytes`.
- **Purpose**: If limits were accidentally bypassed or default raised, this test fails → regression caught.
- **Config**: Use `cfg.verify_limits`; if None, use default (100MB). Attack generates 100MB+1.

### 5. Exit Codes and Status Distinction

| Scenario | Status | Exit (suite) |
|----------|--------|--------------|
| Attack blocked by integrity check | `Blocked` | 0 |
| Attack blocked by verify limits | `Blocked` | 0 |
| Attack bypassed verification | `Bypassed` | 1 |
| Time budget exceeded | `Error` | 2 |
| Config/parse error | — | 3 |

**Rationale**: "Blocked by limits" and "blocked by integrity" are both correct outcomes. Distinction can be inferred from `error_code` (e.g. `LimitBundleBytes`) in the attack result for diagnostics; no need to split `Blocked` into subtypes.

### 6. Time Budget Check Points

Budget checks before:

- Integrity attacks (start)
- After integrity phase (existing)
- After differential phase (existing)
- Before chaos phase (existing)
- Before each expensive verify/iteration in integrity loop (new: optional micro-check if iteration count is high)

**Design**: Single global `TimeBudget` for the whole suite. No per-attack budget to avoid fragmentation.

### 7. `--limits-file` / `@path` Convenience

- `--limits-file path` reads JSON from file
- Future: `--limits @path` as shorthand (optional, not required for v1)

## Consequences

### Positive

- CI can run sim with stricter limits (e.g. 10MB) to catch regressions quickly
- Resource-exhaustion attacks (zip bomb, limit bypass) are regression-tested
- Time budget prevents runaway suites in flaky environments
- Aligns with OWASP "fail fast under load" guidance

### Negative

- Additional CLI surface; must document limits schema
- `--limits` JSON escaping in shell can be awkward; `--limits-file` mitigates

### Neutral

- Differential tests already use `verify_bundle_with_limits` with custom `max_events`; no change needed
- Chaos attacks use subprocess isolation; budget check before phase is sufficient

## Implementation Notes

### Files to Modify

| File | Change |
|------|--------|
| `crates/assay-cli/src/cli/args.rs` | Add `--limits`, `--limits-file`, `--time-budget` to `SimRunArgs` |
| `crates/assay-cli/src/cli/commands/sim.rs` | Parse limits, pass to `SuiteConfig` |
| `crates/assay-sim/src/suite.rs` | Accept configurable `TimeBudget`; pass `verify_limits` to integrity |
| `crates/assay-sim/src/attacks/integrity.rs` | Use `verify_bundle_with_limits`; add `limit_bundle_bytes` attack; budget check |
| `crates/assay-evidence/src/bundle/writer.rs` | `VerifyLimits` already exists; add `impl Serialize/Deserialize` if not present |

### VerifyLimits Serde

If `VerifyLimits` does not implement `Serialize`/`Deserialize`, add behind optional `serde` feature or in assay-evidence for sim use. Partial deserialize: use `#[serde(default)]` for missing fields so partial JSON works.

### Test Plan

1. `assay sim run --suite quick --limits '{"max_bundle_bytes": 1000}'` → zip bomb and limit_bundle_bytes blocked
2. `assay sim run --suite quick --limits-file /nonexistent` → exit 3
3. `assay sim run --suite quick --limits 'invalid'` → exit 3
4. `assay sim run --suite quick --time-budget 1` → time_budget Error after quick phase (if < 1s)
5. Existing quick suite test: no regressions with default limits

## References

- [ROADMAP §G](../ROADMAP.md#g-sim-engine-hardening-p2)
- [assay-evidence VerifyLimits](https://github.com/Rul1an/assay/blob/main/crates/assay-evidence/src/bundle/writer.rs)
- OWASP: Resource exhaustion, fail-fast principles
