# Outward Product Truth Slice 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace false `run_root` Merkle claims with the implemented flat digest contract and add a scoped recurrence guard (issue #2222). Open one draft PR. Do not merge. Do not close #2222.

**Architecture:** Define the implemented digest once in current evidence documentation and have public comments and examples refer to that language. A small claim-level allowlist permits only reviewed genuine or explicitly negated Merkle statements; it does not exempt whole directories or rewrite historical records without an additive correction.

**Tech Stack:** Python 3 standard library, Bash mutation tests, Rust docs/comments, Markdown, pre-commit.

## Global Constraints

- `run_root` is SHA-256 over newline-delimited event content-hash strings, with a trailing newline, in event sequence order.
- Do not call `run_root` a Merkle root, Merkle tree, Merkle sequence, or inclusion-proof root.
- Preserve genuine Rekor and RFC 6962 Merkle constructions.
- Historical ADRs and experiments receive dated correction notes or precise wording; their original decision context is not silently rewritten.
- Do not change evidence wire fields, serialization, or runtime behavior.
- Stage only the named vocabulary files and tests (`git add -A <paths>`).
- Do not touch the files that remain Claude-owned:
  - `crates/assay-cli/tests/verify_side_effects_cli.rs`
  - `crates/assay-evidence/src/coding_agent.rs`
- The `verify_side_effects.rs` vocabulary correction was blocked on Claude until #2355 merged. That dependency is resolved; this branch corrects the false Merkle sentence there.

## Normative allowlist (path-bound, current hits)

Exceptions are `path -> complete-line regular-expression patterns` (`fullmatch` on the stripped line), not substring permits or whole-file exemptions. Generated `vmlinux.rs` identifiers are exact `_E` lines, not `.*merkle_tree_.*`. Hand-written import/call lines are exact `_E` text; `.*rfc6962_root.*` must not pass a comment. A substring such as `Merkle inclusion proof` must not pass `run_root provides a Merkle inclusion proof.` The checker is the imported dict; a vacuous entry (path exists, pattern fullmatches 0 lines) is a hard failure. An empty allowlist must not admit a genuine Rekor fixture. Allowlisting a file must not mask an injected `run_root` claim, including piggybacks that `FALSE_CLAIM_RE` does not match.

```python
# Patterns are complete-line fullmatch. See scripts/ci/check-evidence-vocabulary.py.
ALLOWED_MERKLE_USES = {
    "docs/architecture/SPEC-Outward-Product-Truth-v1.md": (
        # eight exact SPEC lines; not the old substring permits
    ),
    "crates/assay-ebpf/src/vmlinux.rs": (
        # exact generated identifier lines only; not .*merkle_tree_.*
    ),
    "scripts/experiments/aee_spike_lib.py": (
        # exact RFC6962-style prose and def lines
    ),
    "scripts/experiments/aee_spike_check.py": (
        # exact merkle_root import and call lines
    ),
    "scripts/experiments/aee_spike_emit.py": (
        # exact merkle_root import and call lines
    ),
    "crates/assay-registry/src/rekor.rs": (
        # exact Merkle inclusion comment plus exact rfc6962_root import/call
    ),
    "crates/assay-registry/src/rekor/checkpoint.rs": (
        # exact RFC 6962 doc-comment line
    ),
    "docs/architecture/ADR-012-Transparency-Log.md": (
        # three exact Merkle tree/proof lines
    ),
    "crates/assay-cli/tests/spec_reason_code_registry.rs": (
        # four exact negative-assertion lines
    ),
    "docs/architecture/ADR-009-WORM-Storage.md": (
        r"- Native Merkle tree verification",
        r"### 3. Custom Merkle Chain on PostgreSQL",
    ),
}
```

`ALLOWED_MERKLE_USES` is only for genuine Merkle constructions, generated identifiers, and explicit negative spec/test assertions. Do not list historical false phrases, demo filenames, or `verify_side_effects.rs`.

Exclude exactly these two guard-implementation paths from the outward-claim scan (they must spell Merkle to state the rule; listing those self-hits is allowlist sprawl). Do **not** exclude `scripts/ci/` as a directory — a sibling product file under that directory must still fail:

- `scripts/ci/check-evidence-vocabulary.py`
- `scripts/ci/test-evidence-vocabulary.sh`

Do **not** keep a `SCAN_PREFIX_EXCLUDES` escape hatch. Implementation plans are
publication-excluded only when `mkdocs.yml` `exclude_docs` lists
`/superpowers/plans/`; the checker reads that list (one function). A published
docs path with a false claim must still fail.

Historical ADR/RFC/experiment prose is a **corrected-history** rule, not
`ALLOWED_MERKLE_USES`: exact original line, adjacent exact dated 2026-08-14
correction (canonical algorithm, including no `event_id` field). Frozen
generated results keep original bytes plus a dated sidecar pinned by path and
digest. Removing or changing the correction fails. Do not exempt whole files.

The live checker stayed RED on `verify_side_effects.rs` until #2355 merged (the #2354 follow-up). Do not add an allowlist or `TEMPORARY_DEBT` exception for a false phrase. After #2355, this branch corrects that sentence and enables the live-tree pre-commit hook.

Public replacement phrase where a current claim is rewritten:

```text
manifest hashes and the deterministic run-root digest
```

---

### Task 1: Correct this plan (this commit)

- [x] Replace the vacuous allowlist with the measured path-bound pairs above.
- [x] Record that each exception has ≥1 current hit.
- [x] Split the remaining work into separate reviewable commits.

### Task 2: Build the scoped vocabulary guard RED first

**Files:**
- Create: `scripts/ci/check-evidence-vocabulary.py`
- Create: `scripts/ci/test-evidence-vocabulary.sh`
- Modify: `.pre-commit-config.yaml`

**Interfaces:**
- Consumes: tracked text files and `ALLOWED_MERKLE_USES`.
- Produces: exit zero when every scanned `Merkle`/`merkle` occurrence matches an exact reviewed path-and-pattern rule, no allowlist entry is vacuous, and no affirmative `run_root`-as-Merkle phrase is present; bounded diagnostics otherwise.

- [ ] **Step 1: Write the mutation test**

The shell test creates a temporary Git repository with the checker, an allowed Rekor fixture, and a current evidence fixture. It imports `ALLOWED_MERKLE_USES` from the checker (one-rule-one-function). It must prove:

```text
case baseline                         PASS
case false-run-root-merkle            FAIL (inject "run_root is a Merkle root")
case lowercase-false-run-root-merkle  FAIL
case genuine-rekor-merkle             PASS
case vacuous-allowlist-entry          FAIL (path exists, pattern 0 matches)
case allowlist-does-not-mask-claim    FAIL (allowlist file; injected claim still caught)
case empty-allowlist                  FAIL (genuine Rekor fixture, empty dict)
case missing-allowlisted-path         FAIL: stale allowlist entry
case binary-input                     PASS without decoding failure / NUL crash
```

Inject the false claim into `docs/lint/index.md` for the false-claim cases. Construct the injection in the test without placing `run_root` and `Merkle` on the same source line of the test file.

- [ ] **Step 2: Run the self-test and confirm RED**

```bash
bash scripts/ci/test-evidence-vocabulary.sh
```

Expected: FAIL because the checker does not exist.

- [ ] **Step 3: Implement the minimal checker**

Use `git ls-files -z`, skip binary files containing NUL, and fail closed on an affirmative `run_root is a Merkle root` (and close variants) even when the file is allowlisted. Every other `merkle` occurrence must match a path-bound allowlist pattern. Fail if an expected path is missing or a pattern has 0 matches.

Add a pre-commit self-test hook (`pass_filenames: false`) triggered by the checker, the mutation test, and `.pre-commit-config.yaml`. Do not enable the live-tree hook until Task 4 makes the tree green; a live hook here would block the later correction commits.

- [ ] **Step 4: Confirm self-test GREEN**

```bash
bash scripts/ci/test-evidence-vocabulary.sh
```

Expected: PASS. The live checker against this branch may still list current and historical false claims; do not require it green until Task 4.

### Task 3: Correct current product and source vocabulary

**Files:**
- Modify: `CLAUDE.md`
- Modify: `docs/AIcontext/CLAUDE.md`
- Modify: `docs/lint/index.md`
- Modify: `docs/examples/tool-decision-truth/README.md`
- Modify: `docs/launch/SHOW_HN.md`
- Modify: `crates/assay-cli/src/cli/commands/evidence/verify_skill_supply_chain.rs`
- Modify: `crates/assay-cli/src/cli/commands/evidence/verify_tool_decision_truth.rs`
- Modify: `crates/assay-cli/src/cli/commands/project_otel.rs`

Task 3 originally deferred `verify_side_effects.rs` while it was Claude-owned. #2355 resolved that; the final integration corrects the vocabulary sentence there. Do not touch `verify_side_effects_cli.rs` or `coding_agent.rs`.

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

- [ ] **Step 2: Run focused checks**

```bash
bash scripts/ci/test-evidence-vocabulary.sh
```

If Rust module-doc strings changed, run clippy on those assay-cli targets only. Do not run a workspace build.

### Task 4: Correct historical and demo claims without erasing provenance

**Files:**
- Modify: `docs/architecture/ADR-007-Deterministic-Provenance.md`
- Modify: `docs/architecture/ADR-034-Evidence-Redaction-At-Capture.md`
- Modify: `docs/architecture/ADR-039-evidence-bundle-attestation.md`
- Modify: `docs/architecture/RFC-001-dx-ux-governance.md`
- Inspect only: `docs/architecture/ADR-009-WORM-Storage.md` (genuine QLDB / rejected custom chain; do not rewrite)
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
> Correction (2026-08-14): the shipped `run_root` is SHA-256 over newline-delimited
> event content-hash strings, with a trailing newline, in event sequence order —
> not a tree root, and not `event_id` bytes. References below to the historical
> tree proposal describe the model used at the time and are not claims about the
> shipped evidence format.
```

Where the surrounding prose is a current implementation description rather than a recorded proposal, replace the false term directly. Do not rewrite tagged CHANGELOG entries.

- [ ] **Step 2: Make cost tests describe what they measure**

Rename comments and test labels from “Merkle inclusion proof” to “logarithmic proof model used by this experiment” unless the test actually calls a Merkle implementation. If the measured value is not consumed by production verification, state that explicitly.

- [ ] **Step 3: Correct the demo assets**

Keep file paths stable to avoid breaking the video pipeline, but replace captions, narration, mock output, and scene labels with:

```text
SHA-256 content hashes, JCS canonicalization, deterministic run-root digest.
```

The visual may show an ordered hash chain; it must not label it a Merkle tree.

- [ ] **Step 4: Enable the live-tree hook and run the full guard GREEN**

```bash
bash scripts/ci/test-evidence-vocabulary.sh
python3 scripts/ci/check-evidence-vocabulary.py
bash -n demo/mocks/assay-mock.sh demo/produce_video.sh
git diff --check
```

Expected: all pass. Remaining `merkle` hits are allowlisted genuine uses, checker/test self-text, SPEC negation, generated `merkle_tree_*`, stable `merkle-chain.*` filenames, or ADR-009 alternatives. The `verify_side_effects.rs` sentence is corrected after #2355; it is not remaining debt.

### Task 5: Draft PR evidence (do not merge, do not close #2222)

- [ ] Record exact head SHA, verification, mutations, and non-claims. The `verify_side_effects` dependency was resolved by #2355; this branch includes that correction.
- [ ] Open one draft PR linked to #2222 without closing it.
