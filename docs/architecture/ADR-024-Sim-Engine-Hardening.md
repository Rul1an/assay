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
| `--limits` | JSON string or `@path` | Partial limits overrides. If value starts with `@`, treat as file path and load from file (equivalent to `--limits-file`). Otherwise parse as JSON string. Shell escaping can be awkward; prefer `--limits-file` or `--limits @.assay/limits.json`. |
| `--limits-file` | Path | Path to JSON file with limits. **Recommended** for CI. Overrides `--limits` if both given. |
| `--time-budget` | Seconds | Suite time budget. Default: 60. Must be > 0; reject ≤0 with exit 3. |
| `--print-config` | Flag | Print effective merged limits and time budget (debug / CI diagnostics). |

**Parse rules:**

- Invalid JSON → exit code 3 (config error)
- Unknown keys in `--limits` / `--limits-file` JSON → reject with clear "unknown field" error (exit 3)
- `--limits-file` path missing → exit 3
- `--time-budget` ≤ 0 → exit 3
- Schema: `VerifyLimitsOverrides` with `deny_unknown_fields`; partial merge

**Examples (prefer file-based config for CI):**

```bash
assay sim run --suite quick --target bundle.tar.gz
assay sim run --suite quick --limits-file .assay/sim-limits.json --target bundle.tar.gz
assay sim run --suite quick --limits @.assay/sim-limits.json --target bundle.tar.gz  # file via @
assay sim run --suite quick --limits '{"max_bundle_bytes": 10485760}' --target bundle.tar.gz  # JSON string
assay sim run --suite quick --time-budget 120 --target bundle.tar.gz
assay sim run --suite quick --print-config --target bundle.tar.gz  # effective limits + budget
```

### 2. Limits Model

- **Source**: `assay_evidence::VerifyLimits` (existing struct)
- **Parsing**: Use a `VerifyLimitsOverrides` struct with `Option<T>` fields + `#[serde(deny_unknown_fields)]`. Unknown keys in JSON → hard fail (exit 3) with clear "unknown field" error. Partial deserialize: only provided keys override; merge with `VerifyLimits::default()`.
- **Merge precedence**: `VerifyLimits::default()` → apply `--limits` (if given) → apply `--limits-file` (overrides `--limits` if both given).
- **Stable schema**: Document in ADR; breaking changes require version bump.

**Limits schema (field names + semantics):**

| Field | Unit | Semantics |
|-------|------|-----------|
| `max_bundle_bytes` | bytes | **Container/compressed** size limit (input stream). Verification fails with `LimitBundleBytes` when the raw gzip stream exceeds this before decompression. |
| `max_decode_bytes` | bytes | **Decoded/unpacked** size limit (decompressed). Zip-bomb protection; fails with `LimitDecodeBytes` when inflated data exceeds this. |
| `max_manifest_bytes` | bytes | Max manifest.json size |
| `max_events_bytes` | bytes | Max events.ndjson size |
| `max_events` | count | Max event count |
| `max_line_bytes` | bytes | Max single line length |
| `max_path_len` | chars | Max path component length |
| `max_json_depth` | levels | Max JSON recursion depth |

### 3. Integrity Attacks: Pass Limits and Budget

- `check_integrity_attacks` receives `VerifyLimits` and `TimeBudget`
- Use `verify_bundle_with_limits(cursor, limits)` instead of `verify_bundle`
- Before each attack iteration: `if budget.exceeded() { skip remaining; report time_budget }`
- **Exit semantics**: Attack blocked by `LimitBundleBytes` / `LimitDecodeBytes` etc. → `AttackStatus::Blocked` (correct). Not a bypass.

### 4. Dynamic `bundle_size` Attack (Regression-Proof)

Add integrity attack:

- **Name**: `integrity.limit_bundle_bytes`
- **Target**: `max_bundle_bytes` (container/compressed size). The verifier enforces this on the raw gzip input stream *before* decompression; exceeding it yields `LimitBundleBytes`.
- **Behavior**: Generate a bundle whose **compressed** size equals `limits.max_bundle_bytes + 1` bytes. Verification must fail with `LimitBundleBytes`. (Note: `max_decode_bytes` targets decompressed size—zip bombs; this attack targets the input-stream limit.)
- **Purpose**: If limits were accidentally bypassed or default raised, this test fails → regression caught.
- **Optional addition**: `integrity.limit_decode_bytes` — craft a payload that is small compressed but inflates to > `max_decode_bytes` (classic zip-bomb pattern) to explicitly regression-test the decode limit. Quick tier: target ~20MB decode with tier-specific `max_decode_bytes`; avoids the full ~1.1GB zip bomb in quick, but still validates `LimitDecodeBytes` is enforced.
- **Quick-suite safety**: To avoid generating 100MB+ in the quick tier, use **tier-specific defaults** when no explicit `--limits` are given: Quick → `max_bundle_bytes: 5_242_880` (5MB); Nightly/Stress/Chaos → full default (100MB). Attack generates limit+1, so quick stays fast (~5MB). Rationale: 5MB keeps the quick suite under ~30s on typical CI runners; 1MB would risk false limits-hit on slow I/O, 10MB would bloat quick runtime.

### 5. Exit Codes and Status Distinction

| Scenario | Status | Exit (suite) |
|----------|--------|--------------|
| Attack blocked by integrity check | `Blocked` | 0 |
| Attack blocked by verify limits | `Blocked` | 0 |
| Attack bypassed verification | `Bypassed` | 1 |
| Time budget exceeded | `Error` | 2 |
| Config/parse error | — | 3 |

**Rationale**: "Blocked by limits" and "blocked by integrity" are both correct outcomes. No need to split `Blocked` into subtypes.

**Machine-readable output contract** (normative): Attack result metadata must include:
- `blocked_by`: string — error code when status is Blocked (e.g. `LimitBundleBytes`, `LimitDecodeBytes`, `IntegrityHashMismatch`)
- `phase`: string — `integrity` | `differential` | `chaos`
- `skipped_phases`: array of strings — when time budget exceeded, list phases that were skipped (e.g. `["differential", "chaos"]`)
- `time_budget_exceeded`: boolean — true when exit 2

### 6. Time Budget Check Points

Budget checks before:

- Integrity attacks (start)
- After integrity phase (existing)
- After differential phase (existing)
- Before chaos phase (existing)
- In integrity loop: check *after* each expensive verify (not every iteration—keep checks cheap; avoid hot-loop overhead)

**Design**: Single global `TimeBudget` for the whole suite. No per-attack budget to avoid fragmentation.

**Budget-exceeded output**: When time budget is exceeded, output must clearly show:
- Which phases were skipped (e.g. "skipped: differential, chaos") — both human-readable and in `skipped_phases` metadata
- Time consumed / remaining
- A deterministic message: "budget exceeded during integrity phase after N/M cases" (or equivalent)
- `time_budget_exceeded: true` in result metadata for downstream tooling (CI dashboards, telemetry)

### 7. `--limits` @path Parsing

**Unambiguous rule**: When `--limits` is given:
- If the value **starts with `@`**, interpret the remainder as a file path and load JSON from that file (equivalent to `--limits-file path`).
- Otherwise, parse the value as a JSON string.
- This avoids support/UX confusion; no separate `@path` shorthand flag needed.

## Consequences

### Positive

- CI can run sim with stricter limits (e.g. 10MB) to catch regressions quickly
- Resource-exhaustion attacks (zip bomb, limit bypass) are regression-tested
- Time budget prevents runaway suites in flaky environments
- Aligns with OWASP "fail fast under load" guidance

### Negative

- Additional CLI surface; must document limits schema
- `--limits` JSON escaping in shell can be awkward; `--limits-file` is recommended
- Tier-specific default limits add a small config dimension (Quick vs other tiers)

### Neutral

- Differential tests already use `verify_bundle_with_limits` with custom `max_events`; no change needed
- Chaos attacks use subprocess isolation; budget check before phase is sufficient

## Epics

| Epic | Scope | Deps |
|------|-------|------|
| **E1** | `VerifyLimitsOverrides` in assay-evidence; `apply()` merge with defaults; `deny_unknown_fields` | — |
| **E2** | CLI: `--limits`, `--limits-file`, `--time-budget`, `--print-config`; `@path` parsing; merge precedence; exit 3 on parse error | E1 |
| **E3** | SuiteConfig: configurable `TimeBudget`; tier-default limits (Quick: 5MB); pass to integrity | E1 |
| **E4** | Integrity attacks: `verify_bundle_with_limits`; budget check; `blocked_by` in results | E1, E3 |
| **E5** | Dynamic `integrity.limit_bundle_bytes` attack | E4 |
| **E6** | Report metadata: `time_budget_exceeded`, `blocked_by`, `phase`, `skipped_phases`; budget-exceeded UX | E4 |
| **E7** | `--print-config` impl; test plan execution | E2, E6 |

## Implementation Notes

### Files to Modify

| File | Change |
|------|--------|
| `crates/assay-cli/src/cli/args.rs` | Add `--limits`, `--limits-file`, `--time-budget`, `--print-config` to `SimRunArgs` |
| `crates/assay-cli/src/cli/commands/sim.rs` | Parse limits via `VerifyLimitsOverrides`; `--limits @path` when value starts with `@`; merge precedence; pass to `SuiteConfig` |
| `crates/assay-sim/src/suite.rs` | Accept configurable `TimeBudget`; pass `verify_limits` to integrity; tier-default limits (Quick: 5MB) |
| `crates/assay-sim/src/attacks/integrity.rs` | Use `verify_bundle_with_limits`; add `limit_bundle_bytes` (+ optional `limit_decode_bytes`) attack; budget check; `blocked_by` in results |
| `crates/assay-sim/src/report.rs` | `time_budget_exceeded`, `blocked_by` in result metadata |
| `crates/assay-evidence` | `VerifyLimitsOverrides` struct with `deny_unknown_fields` (serde already present) |

### VerifyLimitsOverrides (assay-evidence)

**Location**: `crates/assay-evidence` — where `VerifyLimits` lives. assay-evidence already depends on serde; add `VerifyLimitsOverrides` there to keep schema co-located and avoid drift.

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifyLimitsOverrides {
    max_bundle_bytes: Option<u64>,
    max_decode_bytes: Option<u64>,
    // ... other Option<T> fields
}
```

- `deny_unknown_fields` → unknown keys hard fail
- Partial merge: `defaults.apply(overrides)` (only `Some` values override)

### Test Plan

1. `assay sim run --suite quick --limits '{"max_bundle_bytes": 1000}'` → zip bomb and limit_bundle_bytes blocked
2. `assay sim run --suite quick --limits-file /nonexistent` → exit 3
3. `assay sim run --suite quick --limits 'invalid'` → exit 3
4. `assay sim run --suite quick --limits '{"max_bundle_bytess": 1}'` → exit 3 with "unknown field" (deny_unknown_fields)
5. `assay sim run --suite quick --limits '{"max_bundle_bytes": 1000}' --limits-file .assay/stricter.json` → file wins; test merge precedence
6. `assay sim run --suite quick --time-budget 0` → exit 3 (reject ≤0)
7. `assay sim run --suite quick --time-budget 1` → exit 2 with "budget exceeded" marker; assert output contains "skipped:" (regression: UX marker must not disappear)
8. `assay sim run --suite quick --print-config --target bundle.tar.gz` → output includes effective limits keys and time budget; smoke test presence (not exact string match)
9. Existing quick suite test: no regressions with tier defaults; limit_bundle_bytes generates ~5MB in quick (not 100MB+)

## References

- [ROADMAP §G](../ROADMAP.md#g-sim-engine-hardening-p2)
- [assay-evidence VerifyLimits](https://github.com/Rul1an/assay/blob/main/crates/assay-evidence/src/bundle/writer.rs)
- OWASP: Resource exhaustion, fail-fast principles
