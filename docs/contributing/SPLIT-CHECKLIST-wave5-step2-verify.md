# Wave5 Step2 checklist: verify split behind stable facade

Scope lock:
- Step2 performs mechanical moves only.
- `verify.rs` stays as public facade with unchanged symbol signatures.
- No behavior/perf changes.

Artifacts:
- `docs/contributing/SPLIT-MOVE-MAP-wave5-step2-verify.md`
- `docs/contributing/SPLIT-CHECKLIST-wave5-step2-verify.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave5-step2-verify.md`
- `scripts/ci/review-wave5-step2.sh`

Runbook:
```bash
BASE_REF=origin/codex/wave5-step1-verify-freeze bash scripts/ci/review-wave5-step2.sh
```

Optional override:
```bash
BASE_REF=<your-base-ref> bash scripts/ci/review-wave5-step2.sh
```

Hard gates:
- verify facade has expected delegation callsites for all public helpers
- no `verify_next::` path usage outside `verify.rs`
- `verify.rs` code-only has no heavy parsing/crypto internals
- `policy.rs` has no low-level DSSE crypto primitives
- `dsse.rs` has no policy tokens
- canonicalization helpers stay out of `policy.rs`, `wire.rs`, `keys.rs`
- DSSE crypto helper calls are single-source in `verify_next/dsse.rs`
- `policy.rs` has exactly one DSSE boundary call (`verify_dsse_signature_bytes_impl`)
- diff stays within Step2 allowlist

Definition of done:
- Reviewer script passes.
- Step1 anchor tests remain green.
- Move-map covers all moved functions and caller chains.
