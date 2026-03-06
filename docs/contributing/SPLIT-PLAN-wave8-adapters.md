# Split Plan - Wave 8 Adapters (A2A, UCP)

## Summary

This wave refactors the two largest handwritten adapter files on `origin/main`:

- `crates/assay-adapter-a2a/src/lib.rs` (998 LOC)
- `crates/assay-adapter-ucp/src/lib.rs` (981 LOC)

Goals:

- Preserve public contracts and runtime behavior.
- Split each file into reviewable modules behind a stable facade.
- Add strict reviewer gates (scope, workflow-ban, drift no-increase, boundary checks).

Out of scope:

- Generated code (`crates/assay-ebpf/src/vmlinux.rs`).
- Workflow changes.
- Any API or semantic changes.

## Baseline Commands

Run before push for every step:

```bash
cargo fmt --check
cargo clippy -p assay-adapter-api --all-targets -- -D warnings
```

Wave 8A extras:

```bash
cargo clippy -p assay-adapter-a2a --all-targets -- -D warnings
cargo test -p assay-adapter-a2a
bash scripts/ci/test-adapter-a2a.sh
```

Wave 8B extras:

```bash
cargo clippy -p assay-adapter-ucp --all-targets -- -D warnings
cargo test -p assay-adapter-ucp
bash scripts/ci/test-adapter-ucp.sh
```

## Branching and Slice Discipline

Work from clean `main` in `/tmp/assay-wave8a-step1`.

For each step:

1. branch from `main` (`codex/wave8{a,b}-step{1,2,3}`)
2. implement only allowlisted files
3. run reviewer script + baseline commands
4. commit
5. merge back to local `main` with `--ff-only` progression

## Wave 8A - A2A

### Step 1 (freeze + gates)

Deliverables:

- `docs/contributing/SPLIT-CHECKLIST-wave8a-step1-a2a.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave8a-step1-a2a.md`
- `scripts/ci/review-wave8a-step1.sh`

Acceptance:

- No production code movement yet.
- Allowlist-only + workflow-ban gate active.
- Drift counters frozen at no-increase for A2A hotspot.

### Step 2 (mechanical split)

Target structure:

```text
crates/assay-adapter-a2a/src/
  lib.rs
  adapter_impl/
    mod.rs
    convert.rs
    parse.rs
    version.rs
    fields.rs
    mapping.rs
    payload.rs
    tests.rs
```

Deliverables:

- split implementation files above
- `docs/contributing/SPLIT-MOVE-MAP-wave8a-step2-a2a.md`
- `docs/contributing/SPLIT-CHECKLIST-wave8a-step2-a2a.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave8a-step2-a2a.md`
- `scripts/ci/review-wave8a-step2.sh`

Acceptance:

- Public surface and behavior unchanged.
- Facade remains thin and delegates to internal modules.
- Single-source boundary gates pass.

### Step 3 (closure)

Deliverables:

- `docs/contributing/SPLIT-CHECKLIST-wave8a-step3-a2a.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave8a-step3-a2a.md`
- `scripts/ci/review-wave8a-step3.sh`

Acceptance:

- Dead helpers and duplication removed only when 1:1 safe.
- Final allowlist and drift gates remain strict.

## Wave 8B - UCP

### Step 1 (freeze + gates)

Deliverables:

- `docs/contributing/SPLIT-CHECKLIST-wave8b-step1-ucp.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave8b-step1-ucp.md`
- `scripts/ci/review-wave8b-step1.sh`

Acceptance:

- No production code movement yet.
- Allowlist-only + workflow-ban gate active.
- Drift counters frozen at no-increase for UCP hotspot.

### Step 2 (mechanical split)

Target structure:

```text
crates/assay-adapter-ucp/src/
  lib.rs
  adapter_impl/
    mod.rs
    convert.rs
    parse.rs
    version.rs
    fields.rs
    mapping.rs
    payload.rs
    tests.rs
```

Deliverables:

- split implementation files above
- `docs/contributing/SPLIT-MOVE-MAP-wave8b-step2-ucp.md`
- `docs/contributing/SPLIT-CHECKLIST-wave8b-step2-ucp.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave8b-step2-ucp.md`
- `scripts/ci/review-wave8b-step2.sh`

Acceptance:

- Public surface and behavior unchanged.
- Facade remains thin and delegates to internal modules.
- Single-source boundary gates pass.

### Step 3 (closure)

Deliverables:

- `docs/contributing/SPLIT-CHECKLIST-wave8b-step3-ucp.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave8b-step3-ucp.md`
- `scripts/ci/review-wave8b-step3.sh`

Acceptance:

- Dead helpers and duplication removed only when 1:1 safe.
- Final allowlist and drift gates remain strict.

## Contracts and Invariants

The following must not change across the wave:

- `A2aAdapter`, `UcpAdapter`, and `ProtocolAdapter` signatures.
- Event type strings and mapping semantics.
- Strict/lenient behavior and error kind contracts.
- Fixture-driven deterministic conversion behavior.
