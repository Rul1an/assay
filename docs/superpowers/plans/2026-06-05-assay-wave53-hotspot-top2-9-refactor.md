# Assay Wave53 Hotspot Top 2-9 Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Execute Wave53 as one coordinated refactor wave for the selected top 2 through 9 handwritten Rust hotspot files while preserving all public behavior.

**Architecture:** Keep each current hotspot file as the stable facade and move implementation bodies into private modules. Use Step1 to freeze behavior and gates, then split by readiness: high-readiness pure/data paths first, CLI orchestration next, runner/eBPF paths next, and policy facade last.

**Tech Stack:** Rust workspace, Cargo, Bash reviewer scripts, existing Assay SPLIT-* documentation pattern.

---

## File Structure

Create during Step1:

- `docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step1.md`
- `scripts/ci/review-wave53-hotspot-top2-9-step1.sh`

Create during Step2:

- `crates/assay-registry/src/lockfile_next/model.rs`
- `crates/assay-registry/src/lockfile_next/source.rs`
- `crates/assay-registry/src/lockfile_next/signature.rs`
- `crates/assay-registry/src/lockfile_next/verify.rs`
- `crates/assay-core/src/report/summary/types.rs`
- `crates/assay-core/src/report/summary/metrics.rs`
- `crates/assay-core/src/report/summary/writer.rs`
- `crates/assay-cli/src/cli/commands/bundle/create.rs`
- `crates/assay-cli/src/cli/commands/bundle/verify.rs`
- `crates/assay-cli/src/cli/commands/bundle/paths.rs`
- `crates/assay-cli/src/cli/commands/bundle/coverage.rs`

Create during Step3:

- `crates/assay-cli/src/cli/commands/runner_spike/args.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/spec.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/phases.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/cgroup.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/logs.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/exit_status.rs`
- `crates/assay-cli/src/cli/commands/doctor/fixes.rs`
- `crates/assay-cli/src/cli/commands/doctor/patching.rs`
- `crates/assay-cli/src/cli/commands/doctor/parse_error.rs`
- `crates/assay-cli/src/cli/commands/doctor/preview.rs`

Create during Step4:

- `crates/assay-runner-core/src/kernel/decode.rs`
- `crates/assay-runner-core/src/kernel/stats.rs`
- `crates/assay-runner-core/src/kernel/health.rs`
- `crates/assay-runner-core/src/kernel/notes.rs`
- `crates/assay-ebpf/src/open_events.rs`
- `crates/assay-ebpf/src/connect_events.rs`
- `crates/assay-ebpf/src/fork_events.rs`
- `crates/assay-ebpf/src/path_filter.rs`

Create during Step5:

- `crates/assay-core/src/mcp/policy/types.rs`
- `crates/assay-core/src/mcp/policy/deserialize.rs`
- `crates/assay-core/src/mcp/policy/matcher.rs`
- `crates/assay-core/src/mcp/policy/contracts.rs`

Each step also creates its own checklist, move-map, review-pack, and review script following the
Step1 naming pattern with `step2`, `step3`, `step4`, or `step5`.

For existing facade files such as `summary.rs`, `bundle.rs`, `runner_spike.rs`, `doctor.rs`, and
`kernel.rs`, declare child modules directly from the facade with `mod types;`, `mod create;`, and
similar names. Do not add `summary/mod.rs`, `bundle/mod.rs`, `runner_spike/mod.rs`, `doctor/mod.rs`,
or `kernel/mod.rs`.

### Task 1: Step1 Freeze And Review Gate

**Files:**

- Modify: `docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md`
- Create: `docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step1.md`
- Create: `docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step1.md`
- Create: `docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step1.md`
- Create: `scripts/ci/review-wave53-hotspot-top2-9-step1.sh`

- [ ] **Step 1: Record baseline inventory**

Run:

```bash
rg --files -g '*.rs' | xargs wc -l | sort -nr | sed -n '1,12p'
```

Expected: the inventory is recorded for context. If current LOC order differs from the selected
Wave53 snapshot, keep Wave53 scope fixed to the plan and do not substitute new files mid-wave.

- [ ] **Step 2: Write Step1 checklist**

Create a checklist with these entries:

```markdown
# Wave53 Step1 Checklist - Top 2-9 Hotspot Freeze

- [ ] Plan exists: `docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md`
- [ ] Move-map exists: `docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step1.md`
- [ ] Review pack exists: `docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step1.md`
- [ ] Review script exists: `scripts/ci/review-wave53-hotspot-top2-9-step1.sh`
- [ ] No Rust source files changed in Step1
- [ ] No test files changed in Step1
- [ ] No workflow files changed in Step1
- [ ] `bash scripts/ci/review-wave53-hotspot-top2-9-step1.sh` passes
```

- [ ] **Step 3: Write Step1 move-map**

Create a move-map that states Step1 moves no Rust code and freezes the target files listed in the
Wave53 plan.

- [ ] **Step 4: Write Step1 review pack**

Include:

````markdown
# SPLIT REVIEW PACK - Wave53 Step1 - Top 2-9 Hotspot Freeze

## Scope

Docs and gate only for Wave53.

## Verification

```bash
bash scripts/ci/review-wave53-hotspot-top2-9-step1.sh
```

## Reviewer Focus

- Confirm the wave does not overlap Wave51 scope.
- Confirm generated `vmlinux.rs` is out of scope.
- Confirm Step2 through Step5 preserve stable facades.
````

- [ ] **Step 5: Write Step1 review script**

Use this script body:

```bash
#!/usr/bin/env bash
set -euo pipefail

base_ref="${BASE_REF:-origin/main}"

changed="$(git diff --name-only "$base_ref"...HEAD)"

if printf '%s\n' "$changed" | rg '^\.github/workflows/' >/dev/null; then
  echo "FAIL: workflow edits are out of scope for Wave53 Step1"
  exit 1
fi

if printf '%s\n' "$changed" | rg '^crates/.+\.rs$|^crates/.+/tests/' >/dev/null; then
  echo "FAIL: Rust source/test edits are out of scope for Wave53 Step1"
  exit 1
fi

required=(
  "docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md"
  "docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step1.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step1.md"
  "scripts/ci/review-wave53-hotspot-top2-9-step1.sh"
)

for path in "${required[@]}"; do
  test -f "$path" || {
    echo "FAIL: missing required file: $path"
    exit 1
  }
done

rg -n 'vmlinux\.rs.*out of scope|generated `vmlinux\.rs`' \
  docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md >/dev/null

cargo fmt --check
git diff --check

echo "PASS: Wave53 Step1 freeze gate"
```

- [ ] **Step 6: Verify Step1**

Run:

```bash
chmod +x scripts/ci/review-wave53-hotspot-top2-9-step1.sh
bash scripts/ci/review-wave53-hotspot-top2-9-step1.sh
```

Expected: PASS.

- [ ] **Step 7: Commit Step1**

Run:

```bash
git add docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md \
  docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step1.md \
  docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step1.md \
  docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step1.md \
  scripts/ci/review-wave53-hotspot-top2-9-step1.sh
git commit -m "docs: freeze wave53 hotspot refactor"
```

### Task 2: Step2 High-Readiness Split

**Files:**

- Modify: `crates/assay-registry/src/lockfile.rs`
- Modify: `crates/assay-registry/src/lockfile_next/mod.rs`
- Create/modify: `crates/assay-registry/src/lockfile_next/{model,source,signature,verify}.rs`
- Modify: `crates/assay-core/src/report/summary.rs`
- Create: `crates/assay-core/src/report/summary/{mod,types,metrics,writer}.rs`
- Modify: `crates/assay-cli/src/cli/commands/bundle.rs`
- Create: `crates/assay-cli/src/cli/commands/bundle/{mod,create,verify,paths,coverage}.rs`
- Create: Step2 checklist, move-map, review-pack, review script

- [ ] **Step 1: Write or identify contract tests**

Run:

```bash
cargo test -q -p assay-registry
cargo test -q -p assay-core --lib report::summary
cargo test -q -p assay-cli -- bundle
```

Expected: existing tests pass or the failing selector is replaced in the review pack with the exact
available test selectors that cover lockfile, summary, and bundle behavior.

- [ ] **Step 2: Split `summary.rs`**

Move public structs into `summary/types.rs`, `judge_metrics_from_results` into `summary/metrics.rs`,
and `write_summary` into `summary/writer.rs`. Keep `summary.rs` as facade:

```rust
mod metrics;
mod types;
mod writer;

pub use metrics::judge_metrics_from_results;
pub use types::{
    JudgeMetrics, PerformanceMetrics, PhaseTimings, Provenance, ResultsSummary, SarifOutputInfo,
    Seeds, SlowestTest, Summary,
};
pub use writer::write_summary;
```

- [ ] **Step 3: Split `bundle.rs`**

Move `cmd_create` and run-json extraction helpers into `bundle/create.rs`, `cmd_verify` into
`bundle/verify.rs`, path selection helpers into `bundle/paths.rs`, and replay coverage helpers into
`bundle/coverage.rs`. Keep `bundle.rs` as the command facade and preserve existing command entrypoint
names.

- [ ] **Step 4: Finish `lockfile.rs` facade thinning**

Use existing `lockfile_next/*` where present. Move remaining model/source/signature/verify helper
bodies into the new modules only when the move is 1:1 and public types remain re-exported from
`lockfile.rs`.

- [ ] **Step 5: Verify Step2**

Run:

```bash
cargo fmt --check
cargo check -p assay-registry
cargo test -q -p assay-registry
cargo clippy -p assay-registry --all-targets -- -D warnings
cargo check -p assay-core
cargo test -q -p assay-core --lib report::summary
cargo clippy -p assay-core --all-targets -- -D warnings
cargo check -p assay-cli
cargo test -q -p assay-cli -- bundle
cargo clippy -p assay-cli --all-targets -- -D warnings
git diff --check
bash scripts/ci/review-wave53-hotspot-top2-9-step2.sh
```

Expected: all pass.

- [ ] **Step 6: Commit Step2**

Run:

```bash
git add crates/assay-registry/src/lockfile.rs crates/assay-registry/src/lockfile_next \
  crates/assay-core/src/report/summary.rs crates/assay-core/src/report/summary \
  crates/assay-cli/src/cli/commands/bundle.rs crates/assay-cli/src/cli/commands/bundle \
  docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step2.md \
  docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step2.md \
  docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step2.md \
  scripts/ci/review-wave53-hotspot-top2-9-step2.sh
git commit -m "refactor: split wave53 high-readiness hotspots"
```

### Task 3: Step3 CLI Command Split

**Files:**

- Modify: `crates/assay-cli/src/cli/commands/runner_spike.rs`
- Create: `crates/assay-cli/src/cli/commands/runner_spike/{mod,args,spec,phases,cgroup,logs,exit_status}.rs`
- Modify: `crates/assay-cli/src/cli/commands/doctor.rs`
- Create: `crates/assay-cli/src/cli/commands/doctor/{mod,fixes,patching,parse_error,preview}.rs`
- Create: Step3 checklist, move-map, review-pack, review script

- [ ] **Step 1: Characterize CLI behavior**

Run:

```bash
cargo test -q -p assay-cli -- runner_spike
cargo test -q -p assay-cli -- doctor
```

Expected: existing selectors pass, or the review pack records exact available selectors for these
commands.

- [ ] **Step 2: Split `runner_spike.rs`**

Move argument/spec helpers into `args.rs` and `spec.rs`, phase timing into `phases.rs`, cgroup
helpers into `cgroup.rs`, log decision helpers into `logs.rs`, and exit-status mapping into
`exit_status.rs`. Keep public command structs visible through the facade.

- [ ] **Step 3: Split `doctor.rs`**

Move fix operation selection into `fixes.rs`, patch application/diff rendering into `patching.rs`,
parse-error repair helpers into `parse_error.rs`, and preview output into `preview.rs`.

- [ ] **Step 4: Verify Step3**

Run:

```bash
cargo fmt --check
cargo check -p assay-cli
cargo test -q -p assay-cli -- runner_spike
cargo test -q -p assay-cli -- doctor
cargo clippy -p assay-cli --all-targets -- -D warnings
git diff --check
bash scripts/ci/review-wave53-hotspot-top2-9-step3.sh
```

Expected: all pass.

- [ ] **Step 5: Commit Step3**

Run:

```bash
git add crates/assay-cli/src/cli/commands/runner_spike.rs \
  crates/assay-cli/src/cli/commands/runner_spike \
  crates/assay-cli/src/cli/commands/doctor.rs \
  crates/assay-cli/src/cli/commands/doctor \
  docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step3.md \
  docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step3.md \
  docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step3.md \
  scripts/ci/review-wave53-hotspot-top2-9-step3.sh
git commit -m "refactor: split wave53 cli hotspots"
```

### Task 4: Step4 Runner And eBPF Split

**Files:**

- Modify: `crates/assay-runner-core/src/kernel.rs`
- Create: `crates/assay-runner-core/src/kernel/{mod,decode,stats,health,notes}.rs`
- Modify: `crates/assay-ebpf/src/main.rs`
- Create: `crates/assay-ebpf/src/{open_events,connect_events,fork_events,path_filter}.rs`
- Create: Step4 checklist, move-map, review-pack, review script

- [ ] **Step 1: Characterize runner kernel behavior**

Run:

```bash
cargo test -q -p assay-runner-core
cargo check -p assay-runner-core
```

Expected: pass.

- [ ] **Step 2: Split `kernel.rs`**

Move monitor-event decoding into `kernel/decode.rs`, stats delta/breakdown helpers into
`kernel/stats.rs`, health classification into `kernel/health.rs`, and note rendering into
`kernel/notes.rs`. Keep `KernelLayerBuilder` and public structs re-exported from `kernel.rs`.

- [ ] **Step 3: Characterize eBPF build path**

Run:

```bash
cargo check -p assay-ebpf
```

Expected: pass on configured eBPF hosts. If host prerequisites fail, record the exact stderr in the
review pack and run the repo-supported fallback eBPF check from the existing CI scripts.

- [ ] **Step 4: Split `assay-ebpf/src/main.rs` conservatively**

Move open tracepoint helpers into `open_events.rs`, connect helpers into `connect_events.rs`, fork
helpers into `fork_events.rs`, and loader/dedup path helpers into `path_filter.rs`. Keep tracepoint
entry functions and map declarations in `main.rs` unless moving them is required by the existing eBPF
crate pattern.

- [ ] **Step 5: Verify Step4**

Run:

```bash
cargo fmt --check
cargo check -p assay-runner-core
cargo test -q -p assay-runner-core
cargo clippy -p assay-runner-core --all-targets -- -D warnings
cargo check -p assay-ebpf
git diff --check
bash scripts/ci/review-wave53-hotspot-top2-9-step4.sh
```

Expected: all configured checks pass, with any eBPF host prerequisite limitation recorded.

- [ ] **Step 6: Commit Step4**

Run:

```bash
git add crates/assay-runner-core/src/kernel.rs crates/assay-runner-core/src/kernel \
  crates/assay-ebpf/src/main.rs crates/assay-ebpf/src/open_events.rs \
  crates/assay-ebpf/src/connect_events.rs crates/assay-ebpf/src/fork_events.rs \
  crates/assay-ebpf/src/path_filter.rs \
  docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step4.md \
  docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step4.md \
  docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step4.md \
  scripts/ci/review-wave53-hotspot-top2-9-step4.sh
git commit -m "refactor: split wave53 runner and ebpf hotspots"
```

### Task 5: Step5 Policy Facade Split And Closure

**Files:**

- Modify: `crates/assay-core/src/mcp/policy/mod.rs`
- Create: `crates/assay-core/src/mcp/policy/{types,deserialize,matcher,contracts}.rs`
- Create: Step5 checklist, move-map, review-pack, review script

- [ ] **Step 1: Freeze policy behavior**

Run:

```bash
cargo test -q -p assay-core --test policy_engine_test
cargo test -q -p assay-core --lib mcp::tests
cargo test -q -p assay-cli --test e2e_policy_test
```

Expected: pass.

- [ ] **Step 2: Split policy public surface**

Move public data types into `types.rs`, custom deserializers into `deserialize.rs`, pattern matching
helpers into `matcher.rs`, and decision contract structs/helpers into `contracts.rs`. Keep
`mcp/policy/mod.rs` as the stable public facade with `pub use` exports for existing consumers.

- [ ] **Step 3: Verify no policy drift**

Run:

```bash
cargo fmt --check
cargo check -p assay-core
cargo test -q -p assay-core --test policy_engine_test
cargo test -q -p assay-core --lib mcp::tests
cargo test -q -p assay-cli --test e2e_policy_test
cargo clippy -p assay-core --all-targets -- -D warnings
git diff --check
bash scripts/ci/review-wave53-hotspot-top2-9-step5.sh
```

Expected: all pass.

- [ ] **Step 4: Update closure review pack**

Record actual LOC deltas for all eight files:

```bash
wc -l crates/assay-runner-core/src/kernel.rs \
  crates/assay-cli/src/cli/commands/runner_spike.rs \
  crates/assay-ebpf/src/main.rs \
  crates/assay-registry/src/lockfile.rs \
  crates/assay-core/src/mcp/policy/mod.rs \
  crates/assay-cli/src/cli/commands/bundle.rs \
  crates/assay-core/src/report/summary.rs \
  crates/assay-cli/src/cli/commands/doctor.rs
```

Expected: review pack includes before/after LOC and lists any remaining file above 500 LOC.

- [ ] **Step 5: Commit Step5**

Run:

```bash
git add crates/assay-core/src/mcp/policy/mod.rs \
  crates/assay-core/src/mcp/policy/types.rs \
  crates/assay-core/src/mcp/policy/deserialize.rs \
  crates/assay-core/src/mcp/policy/matcher.rs \
  crates/assay-core/src/mcp/policy/contracts.rs \
  docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step5.md \
  docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step5.md \
  docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step5.md \
  scripts/ci/review-wave53-hotspot-top2-9-step5.sh
git commit -m "refactor: split wave53 policy facade"
```

## Self-Review

- Spec coverage: covered all eight target files from ranks 2 through 9 and excluded generated
  `vmlinux.rs`.
- Scope control: Step1 is docs/gate only; Step2 through Step5 are ordered by readiness and crate risk.
- Type consistency: public facade names match the Wave53 plan.
- Verification: each task includes crate checks, targeted tests, review script execution, and commit
  boundaries.
