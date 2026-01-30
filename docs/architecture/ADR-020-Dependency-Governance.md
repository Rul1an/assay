# ADR-020: Dependency Governance

**Status:** Accepted
**Date:** 2026-01-30

---

## Context

Major dependency bumps in Rust can cause ecosystem-wide breakage when crates depend on different major versions of shared dependencies. This is particularly problematic for:

- **Trait incompatibility:** Different major versions of a crate define incompatible traits, even if the trait names are identical.
- **Type mismatches:** Types from different crate versions are considered distinct by the compiler.
- **Cryptographic dependencies:** Crates like `rand_core` define security-critical traits (`CryptoRngCore`) that must be compatible across the dependency tree.

Without explicit documentation, deferred upgrades become "invisible blockers" that accumulate technical debt or get forgotten.

---

## Decision

1. **Document deferred upgrades** in this ADR with:
   - Reason for deferral
   - Bottleneck crate(s)
   - Unblock condition
   - Revisit date (30/60/90 days)
   - Tracking issue link

2. **Dependabot ignore rules** for deferred dependencies must:
   - Be specific (named dependency, not wildcards)
   - Only block the problematic update type (usually `semver-major`)
   - Include a comment referencing this ADR

3. **Revisit triggers:**
   - Calendar reminder at revisit date
   - Upstream release announcements
   - Security advisories (override deferral if critical)

---

## Deferred Dependencies

### rand 0.9 (Deferred: 2026-01-30)

| Field | Value |
|-------|-------|
| **Reason** | `rand 0.9` depends on `rand_core 0.9`; `ed25519-dalek 2.x` depends on `rand_core 0.6`. The `CryptoRngCore` trait bounds are incompatible across these major versions, causing compile errors when using `SigningKey::generate()` with `rand::thread_rng()`. |
| **Bottleneck** | `ed25519-dalek 2.x` |
| **Unblock condition** | `ed25519-dalek 3.0` stable with `rand_core 0.9` support |
| **Revisit date** | 2026-04-30 (90 days) |
| **Tracking** | [#84](https://github.com/Rul1an/assay/issues/84) |
| **Dependabot rule** | `.github/dependabot.yml` line 36-38 |

**Technical details:**

```
error[E0277]: the trait bound `ThreadRng: rand_core::CryptoRngCore` is not satisfied
  --> crates/assay-cli/src/cli/commands/tool/keygen.rs:61:51
   |
61 |     let signing_key = SigningKey::generate(&mut rand::thread_rng());
   |                                                 ^^^^^^^^^^^^^^^^^^
   |
   = note: two types coming from two different versions of the same crate are different types
```

**Migration path when unblocked:**

1. Bump `ed25519-dalek` to 3.0
2. Bump `rand` to 0.9
3. Update `rand::distributions::Alphanumeric` → `rand::distr::Alphanumeric`
4. Update `StdRng::from_entropy()` → `StdRng::from_os_rng()`
5. Remove dependabot ignore rule
6. Close tracking issue

---

## Consequences

### Easier

- Audit trail for why certain upgrades are blocked
- Clear unblock conditions prevent indefinite deferral
- Revisit dates ensure periodic reassessment
- New contributors understand the constraints

### Harder

- Requires discipline to document deferrals
- Must remember to update this ADR when unblocking
- Tracking issues need to be maintained

---

## Related

- [`.github/dependabot.yml`](../../.github/dependabot.yml) — Dependabot ignore rules
- `cargo audit` / `cargo deny` — Security advisory monitoring (overrides deferrals)
