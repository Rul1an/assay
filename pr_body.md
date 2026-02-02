## Why

This PR delivers **Epic E7 (Judge Reliability)** to meet SOTA 2026 Audit requirements. It implements critical reliability controls, including deterministic orchestration, bias mitigation, and robust audit evidence.

## What changed

### 🚀 E7 Judge Reliability (Audit Grade)
- **Deterministic Orchestration:**
    - Per-test seed derivation (`suite_seed + test_id`).
    - Randomized order default using the derived seed.
    - Soft global budget (telemetry only) to ensure parallel determinism.
- **Bias Mitigation:**
    - **Adaptive Majority:** Sequential 2-of-3 voting on instability.
    - **Borderline Band:** [0.4, 0.6] range triggers abstention or rerun.
    - **Blind Labeling:** Variable swapping ("Response A/B") based on seed parity.
- **Robustness:**
    - **JSON Parsing:** Greedy stream seeker to handle LLM preamble chatter.
    - **Cache Key:** Full fingerprinting of config, seed, and system prompt version.

### 🛡️ E6 Security Hardening (Integrated)
- **Auth Validation:** Added `alg`, `typ`, and `crit` header validation for JWTs.
- **Resource Indicators:** Enforced strict resource matching (RFC 8707).

### 📄 Audit Evidence
- Persisted **Audit Evidence Pack** in `docs/audit/E7-AUDIT.md`.
- Upgraded project documentation status to **Audit Grade**.

## Verification

| Contract Test | Invariant Verified | Result |
| :--- | :--- | :--- |
| `contract_determinism_parallel_replay` | Parallel execution with shared state yields identical metadata. | ✅ PASS |
| `contract_cache_key_unique` | Changing any config param (incl. seed) invalidates cache. | ✅ PASS |
| `contract_two_of_three_majority` | Unstable results trigger sequential reruns and majority vote. | ✅ PASS |

## Notes for Reviewers
- **Audit Pack:** Please review `docs/audit/E7-AUDIT.md` for the threat model and compliance mapping.
- **Compatibility:** This PR introduces stricter defaults for Judge reliability but remains backward compatible with existing config schemas.
