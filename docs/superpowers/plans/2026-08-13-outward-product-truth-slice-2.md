# Outward Product Truth Slice 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close issue #2222 by replacing false `run_root` Merkle claims with the implemented flat digest contract and adding a scoped recurrence guard.

**Architecture:** Define the implemented digest once in current evidence documentation and have public comments and examples refer to that language. A small claim-level allowlist permits only reviewed genuine or explicitly negated Merkle statements; it does not exempt whole files or rewrite historical records without an additive correction.

**Tech Stack:** Python 3 standard library, Bash mutation tests, Rust docs/comments, Markdown, pre-commit.

## Global Constraints

- `run_root = sha256(concat(entry_hashes_in_manifest_order))` is the implemented rule.
- Do not call `run_root` a Merkle root, Merkle tree, Merkle sequence, or inclusion-proof root.
- Preserve genuine Rekor and RFC 6962 Merkle constructions.
- Historical ADRs and experiments receive dated correction notes or precise wording; their original decision context is not silently rewritten.
- Do not change evidence wire fields, serialization, or runtime behavior.
- Stage only the named vocabulary files and tests.

---

### Task 1: Build the scoped vocabulary guard RED first

**Files:**
- Create: `scripts/ci/check-evidence-vocabulary.py`
- Create: `scripts/ci/test-evidence-vocabulary.sh`
- Modify: `.pre-commit-config.yaml`

**Interfaces:**
- Consumes: tracked text files and an explicit allowlist of genuine Merkle paths.
- Produces: exit zero when every tracked `Merkle` occurrence matches an exact reviewed path-and-pattern rule; bounded diagnostics otherwise.

- [ ] **Step 1: Write the mutation test**

The shell test creates a temporary Git repository with the checker, an allowed Rekor fixture, and a current evidence fixture. It must prove:

```text
case baseline                         PASS
case false-run-root-merkle            FAIL: unapproved Merkle claim
case lowercase-false-run-root-merkle  FAIL: unapproved Merkle claim
case genuine-rekor-merkle             PASS
case missing-allowlisted-path          FAIL: stale allowlist entry
case binary-input                     PASS without decoding failure
```

Inject `run_root is a Merkle root` into `docs/lint/index.md` for the two false cases.

- [ ] **Step 2: Run the self-test and confirm RED**

```bash
bash scripts/ci/test-evidence-vocabulary.sh
```

Expected: FAIL because the checker does not exist.

- [ ] **Step 3: Implement the minimal checker**

Use `git ls-files -z`, skip binary files containing NUL, and identify false evidence claims through explicit case-insensitive patterns such as `run_root` paired with `Merkle`, `Merkle root`, `Merkle inclusion proof`, and `Merkle-chained`. Do not treat every use of the word as false. Permit reviewed uses through exact path-and-pattern rules:

```python
ALLOWED_MERKLE_USES = {
    "crates/assay-registry/src/rekor.rs": (r"RFC 6962", r"Merkle proof"),
    "docs/architecture/ADR-012-Transparency-Log.md": (r"RFC 6962",),
    "crates/assay-cli/tests/spec_reason_code_registry.rs": (r"not a Merkle",),
}
```

Represent exceptions as `path -> exact regular-expression patterns`, not whole-file exemptions. Permit only the specific Rekor/RFC 6962 construction, generated kernel identifiers, experiment code that actually implements its named tree model, and explicit negative assertions in the spec or tests. Treat `docs/superpowers/plans/` as non-normative implementation records and exclude it explicitly from the outward-claim scan. Inspect `ADR-009-WORM-Storage.md` before deciding whether each occurrence names a real construction; do not exempt the file by default. Fail if an expected path or expected pattern is missing, so stale exceptions cannot accumulate silently. The checker diagnostic must include path, line, and text.

Add a pre-commit hook triggered by the checker, test, `.pre-commit-config.yaml`, and every currently corrected file. The checker itself scans all tracked text files, so set `pass_filenames: false`.

- [ ] **Step 4: Run the guard against current main**

```bash
bash scripts/ci/test-evidence-vocabulary.sh
python3 scripts/ci/check-evidence-vocabulary.py
```

Expected: self-test PASS; live checker FAIL and list every false current claim. Do not commit the intermediate red repository state; keep the guard paths owned by this writer until Task 3 makes the live check green.

### Task 2: Correct current product and source vocabulary

**Files:**
- Modify: `CLAUDE.md`
- Modify: `docs/AIcontext/CLAUDE.md`
- Modify: `docs/lint/index.md`
- Modify: `docs/examples/tool-decision-truth/README.md`
- Modify: `docs/launch/SHOW_HN.md`
- Modify: `crates/assay-cli/src/cli/commands/evidence/verify_side_effects.rs`
- Modify: `crates/assay-cli/src/cli/commands/evidence/verify_skill_supply_chain.rs`
- Modify: `crates/assay-cli/src/cli/commands/evidence/verify_tool_decision_truth.rs`
- Modify: `crates/assay-cli/src/cli/commands/project_otel.rs`

**Interfaces:**
- Consumes: `compute_run_root` in `assay-evidence`.
- Produces: one consistent public phrase: “manifest hashes and the deterministic run-root digest.”

- [ ] **Step 1: Replace false current claims**

Use these exact semantics:

```text
manifest.json: schema, run metadata, file hashes, and a deterministic SHA-256 run-root digest
```

For `BundleReader::open` comments:

```rust
//! `BundleReader::open` verifies manifest hashes and the deterministic run-root digest.
```

For lint prose:

```text
Changing bundle content changes its content hashes and invalidates the recorded run-root digest.
```

For launch prose, remove “cryptographically prove what an agent did.” State only that a verifier can recompute whether carried bytes match the recorded manifest. Preserve the non-claim that this does not prove an external side effect or provider outcome.

- [ ] **Step 2: Run focused Rust tests and vocabulary checker**

```bash
python3 scripts/ci/check-evidence-vocabulary.py
cargo test -p assay-cli spec_reason_code_registry
cargo test -p assay-evidence
```

Expected: the checker still fails only on historical/demo files left for Task 3; Rust tests pass. Do not commit while the live checker remains red.

### Task 3: Correct historical and demo claims without erasing provenance

**Files:**
- Modify: `docs/architecture/ADR-007-Deterministic-Provenance.md`
- Modify: `docs/architecture/ADR-034-Evidence-Redaction-At-Capture.md`
- Modify: `docs/architecture/ADR-039-evidence-bundle-attestation.md`
- Modify: `docs/architecture/RFC-001-dx-ux-governance.md`
- Modify: `docs/experiments/evidence-mutation-cost-2026-06/README.md`
- Modify: `docs/experiments/runner-vs-otel-2026-05/workload/src/manifest-binding.ts`
- Modify: `crates/assay-evidence/tests/e3_verify_cost_curve.rs`
- Modify: `crates/assay-sim/tests/e3_mutation_matrix.rs`
- Modify: `demo/AI-VIDEO-PLAYBOOK.md`
- Modify: `demo/captions.srt`
- Modify: `demo/mocks/assay-mock.sh`
- Modify: `demo/produce_video.sh`
- Modify: `demo/scenes/merkle-chain.tape`

**Interfaces:**
- Consumes: the original historical artifact plus the measured implementation.
- Produces: dated correction notes and demos that no longer claim a Merkle proof.

- [ ] **Step 1: Add dated corrections to historical records**

At the first false statement in each ADR/RFC/experiment, add:

```markdown
> Correction (2026-08-13): the shipped `run_root` is a flat SHA-256 digest over
> ordered entry hashes, not a tree root. References below to the historical tree
> proposal describe the model used at the time and are not claims about the
> shipped evidence format.
```

Where the surrounding prose is a current implementation description rather than a recorded proposal, replace the false term directly.

- [ ] **Step 2: Make cost tests describe what they measure**

Rename comments and test labels from “Merkle inclusion proof” to “logarithmic proof model used by this experiment” unless the test actually calls a Merkle implementation. If the measured value is not consumed by production verification, state that explicitly.

- [ ] **Step 3: Correct the demo assets**

Keep file paths stable to avoid breaking the video pipeline, but replace captions, narration, mock output, and scene labels with:

```text
SHA-256 content hashes, JCS canonicalization, deterministic run-root digest.
```

The visual may show an ordered hash chain; it must not label it a Merkle tree.

- [ ] **Step 4: Run the full guard GREEN**

```bash
bash scripts/ci/test-evidence-vocabulary.sh
python3 scripts/ci/check-evidence-vocabulary.py
cargo test -p assay-evidence --test e3_verify_cost_curve
cargo test -p assay-sim --test e3_mutation_matrix
bash -n demo/mocks/assay-mock.sh demo/produce_video.sh
```

Expected: all pass.

- [ ] **Step 5: Commit the green guard and all vocabulary corrections together**

```bash
git add -- scripts/ci/check-evidence-vocabulary.py scripts/ci/test-evidence-vocabulary.sh .pre-commit-config.yaml CLAUDE.md docs/AIcontext/CLAUDE.md docs/lint/index.md docs/examples/tool-decision-truth/README.md docs/launch/SHOW_HN.md crates/assay-cli/src/cli/commands/evidence/verify_side_effects.rs crates/assay-cli/src/cli/commands/evidence/verify_skill_supply_chain.rs crates/assay-cli/src/cli/commands/evidence/verify_tool_decision_truth.rs crates/assay-cli/src/cli/commands/project_otel.rs docs/architecture/ADR-007-Deterministic-Provenance.md docs/architecture/ADR-034-Evidence-Redaction-At-Capture.md docs/architecture/ADR-039-evidence-bundle-attestation.md docs/architecture/RFC-001-dx-ux-governance.md docs/experiments/evidence-mutation-cost-2026-06/README.md docs/experiments/runner-vs-otel-2026-05/workload/src/manifest-binding.ts crates/assay-evidence/tests/e3_verify_cost_curve.rs crates/assay-sim/tests/e3_mutation_matrix.rs demo/AI-VIDEO-PLAYBOOK.md demo/captions.srt demo/mocks/assay-mock.sh demo/produce_video.sh demo/scenes/merkle-chain.tape
git commit -m "docs(evidence): correct historical Merkle claims"
```

### Task 4: Final verification and issue closure evidence

**Files:**
- No new production files.

**Interfaces:**
- Consumes: all Slice 2 commits.
- Produces: exact-head proof for issue #2222 and PR review.

- [ ] **Step 1: Run the full affected suite**

```bash
bash scripts/ci/test-evidence-vocabulary.sh
python3 scripts/ci/check-evidence-vocabulary.py
cargo test -p assay-cli spec_reason_code_registry
cargo test -p assay-evidence
cargo test -p assay-sim
cargo fmt --all -- --check
cargo clippy -p assay-cli -p assay-evidence -p assay-sim -- -D warnings
pre-commit run --all-files
git diff --check origin/main...HEAD
```

- [ ] **Step 2: Record allowlist evidence**

List every remaining `Merkle` occurrence and classify it as Rekor, RFC 6962, kernel-generated source, negative test text, or the checker itself. A remaining occurrence without a named genuine construction is a failure.

- [ ] **Step 3: Open a draft PR linked to #2222**

The PR body states that runtime behavior and wire format are unchanged. Request one independent exact-head review; close #2222 only after merge.
