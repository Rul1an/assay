# Review Pack: `assay evidence verify --eval` (ADR-025 E2 Phase 3)

MVP for tamper-evident evaluation sidecar verification. Use this pack for PR review or self-check before merge.

---

## Scope

| Item | Description |
|------|-------------|
| **Feature** | `assay evidence verify --eval eval.json bundle.tar.gz` |
| **ADR** | ADR-025-E2 Phase 3 (consumer sidecar) |
| **Goal** | Prove (a) eval belongs to this bundle, (b) pack inputs not swapped, (c) results_digest corresponds to report |

---

## Files to Review

| Path | Purpose |
|------|---------|
| `crates/assay-evidence/src/evaluation.rs` | Schema, `verify_evaluation()`, `ReportInline`, digest helpers |
| `crates/assay-evidence/src/lib.rs` | Re-exports `verify_evaluation`, `VerifyEvalResult` |
| `crates/assay-cli/src/cli/commands/evidence/mod.rs` | `EvidenceVerifyArgs`, `cmd_verify`, `cmd_verify_eval` |
| `crates/assay-evidence/tests/verify_eval_test.rs` | Integration roundtrip test |
| `crates/assay-evidence/src/evaluation.rs` (tests) | Unit: `test_verify_evaluation_ok`, `test_verify_evaluation_bundle_digest_mismatch` |

---

## Review Checklist

### 1. CLI

- [ ] `assay evidence verify --eval <PATH> [BUNDLE]` — eval required, bundle positional (or `-` for stdin; not allowed with `--eval`)
- [ ] `--pack cicd-starter[,eu-ai-act-baseline]` — comma-delimited pack refs
- [ ] `--strict` — fail when packs unverifiable (no `--pack` or pack not resolvable)
- [ ] `--json` — machine-readable output
- [ ] `-q, --quiet` — exit code only
- [ ] Help text references ADR-025 E2 Phase 3

### 2. Verification Logic

- [ ] **Schema:** `schema_version == "evaluation-v1"` else fail
- [ ] **evaluation_id:** must be valid UUID
- [ ] **created_at:** must be RFC3339
- [ ] **Digest format:** `sha256:<64 hex>` for `bundle_digest`, `manifest_digest`, `results_digest`; fail on invalid
- [ ] **Bundle binding:** `compute_bundle_digest(manifest)` == `evaluation.inputs.bundle_digest`
- [ ] **Manifest binding:** `compute_manifest_digest(manifest)` == `evaluation.inputs.manifest_digest`
- [ ] **Results digest:** if `report_inline` present → recompute JCS digest over report → match `outputs.results_digest`
- [ ] **Results digest (no inline):** warn "not verifiable"; `--strict` does not fail on this (only on packs)
- [ ] **Pack digests:** when `--pack` given, resolve packs, compare `packs_applied[].digest`; mismatch → fail
- [ ] **Pack unverifiable:** pack in eval not in resolved set → warn; `--strict` → fail
- [ ] **Pack not resolved:** no `--pack` but eval has `packs_applied` → unverifiable count; `--strict` → fail with "pass --pack to resolve"

### 3. Exit Codes

- [ ] **0** — all checks pass (warnings OK in non-strict)
- [ ] **1** — verification failed (digest mismatch, schema error, pack mismatch)
- [ ] **2** — infra/config (missing file, invalid JSON, bundle verify failed, pack resolution error)

### 4. Output UX

- [ ] Human: `✅ Evaluation verified` + bundle_digest, manifest_digest, results_digest status, packs summary
- [ ] Human failure: `❌ Evaluation verification failed` + exact error(s)
- [ ] JSON: `ok`, `bundle`, `results`, `packs`, `warnings` per spec

### 5. Inline Report (results_digest)

- [ ] `build_evaluation_from_lint` emits `report_inline: ReportInline { schema_version, report }` with canonical lint report
- [ ] `canonical_report_json()` matches structure used for digest
- [ ] `compute_results_digest_from_inline()` uses JCS over report
- [ ] Verify: `report_inline` present → recompute and compare; absent → mark unverifiable

### 6. Tests

- [ ] `test_verify_eval_roundtrip` — lint → build eval → verify → ok
- [ ] `test_verify_evaluation_ok` — unit roundtrip
- [ ] `test_verify_evaluation_bundle_digest_mismatch` — wrong bundle_digest → fail
- [ ] Manual: bundle_digest mutation → exit 1
- [ ] Manual: manifest_digest mutation → exit 1
- [ ] Manual: report payload mutation → results_digest mismatch → exit 1
- [ ] Manual: pack digest mutation + `--pack` → exit 1
- [ ] Manual: `--strict` without `--pack` (eval has packs) → exit 1

### 7. Integration

- [ ] Bundle must be verified first (`verify_bundle`); manifest from `VerifyResult`
- [ ] Pack resolution via `load_packs()`; digest from `LoadedPack.digest`
- [ ] No bundle mutation by lint/verify (sidecar is separate file)

### 8. Documentation / ADR

- [ ] ADR-025-E2: "verify --eval" checkbox/merge gate updated
- [ ] ADR mentions `report_inline` for portable results_digest verification

---

## Manual Verification Commands

```bash
# Happy path
assay evidence lint tests/fixtures/evidence/test-bundle.tar.gz --emit-eval /tmp/eval.json
assay evidence verify --eval /tmp/eval.json tests/fixtures/evidence/test-bundle.tar.gz
# Expected: exit 0

# With pack digest verification
assay evidence verify --eval /tmp/eval.json tests/fixtures/evidence/test-bundle.tar.gz --pack cicd-starter
# Expected: exit 0, packs: 1 ok

# Strict without --pack (eval has packs)
assay evidence verify --eval /tmp/eval.json tests/fixtures/evidence/test-bundle.tar.gz --strict
# Expected: exit 1, "--strict: 1 pack(s) unverifiable"

# Mutate bundle_digest in eval → exit 1
# Mutate manifest_digest in eval → exit 1
# Mutate report payload (e.g. "total": 1 → 99) → exit 1
# Mutate packs_applied digest + --pack cicd-starter → exit 1
```

---

## Acceptance Criteria (from spec)

| # | Criterium | Status |
|---|-----------|--------|
| 1 | `verify --eval eval.json bundle.tar.gz` exit 0 on valid pair | ✅ |
| 2 | Mutate eval.bundle_digest → exit 1, clear message | ✅ |
| 3 | Mutate eval.manifest_digest → exit 1 | ✅ |
| 4 | Mutate inline report → results_digest mismatch → exit 1 | ✅ |
| 5 | Pack digest mismatch → exit 1; unverifiable + strict → exit 1 | ✅ |

---

## Known Gaps / Follow-ups

- **ADR-025-E2 merge gate:** checkbox "Verification: assay evidence verify --eval..." should be marked done
- **Soak integration:** `assay sim soak --emit-eval` will reuse same pattern; verify --eval will support soak sidecar later
- **Exit code 2:** Infra errors (file not found, invalid JSON) currently bubble via `anyhow`; CLI returns 1 on `Result::Err`. Consider explicit mapping for 2.
- **JSON output:** `results_digest_verifiable` field present; consumers can distinguish "verified" vs "not verifiable (no report_inline)"
