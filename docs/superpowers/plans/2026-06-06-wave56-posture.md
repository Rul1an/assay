# Wave56 Posture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a small non-runner posture PR that improves toolchain determinism, nextest defaults, and stale-lint-suppression hygiene without changing runtime behavior or entering the delegated eBPF runner lane.

**Architecture:** Keep this as a tooling and lint-hygiene wave only. Do not edit `Cargo.toml`, `Cargo.lock`, runner crates, eBPF crates, monitor crates, or release profiles; those surfaces either require a separate gates=all proof or belong in the later release/lint wave. Convert only proven, low-risk `#[allow(...)]` suppressions to `#[expect(...)]` in non-runner, non-eBPF crates so stale suppressions become visible.

**Tech Stack:** Rust stable 1.96.0, Cargo, Clippy, rustfmt, cargo-nextest, Bash review gate.

---

## Baseline

Measured on `origin/main` at `4d22a495c79fd06d8422c3c8c11c96b00a0674ee`.

```text
workspace edition: 2021
workspace rust lints: unsafe_code=deny, rust_2018_idioms=warn, unused_lifetimes=warn
workspace clippy lints: all=warn, pedantic=allow, unwrap_used=allow
rust-toolchain.toml: absent
.config/nextest.toml: absent
deny/audit config: deny.toml and .cargo/audit.toml present
release profile: panic=abort, strip=true, no workspace lto/codegen-units=1
non-generated #[allow(...)] count: 88
#[expect(...)] count: 0
non-generated .unwrap() count: 2230
```

Current Rust stable on 2026-06-06 is 1.96.0. Pinning to `1.96.0` is intentional: Cargo 1.96 includes the May 2026 crate-tarball hardening, and exact toolchain pinning makes local, CI, and delegated builds deterministic.

## Strict Scope

Allowed files for this PR:

```text
rust-toolchain.toml
.config/nextest.toml
scripts/ci/review-wave56-posture.sh
docs/superpowers/plans/2026-06-06-wave56-posture.md
crates/assay-adapter-a2a/src/adapter_impl/payload.rs
crates/assay-adapter-acp/src/adapter_impl/convert.rs
crates/assay-adapter-ucp/src/adapter_impl/payload.rs
crates/assay-core/src/judge/judge_internal/run.rs
crates/assay-core/src/judge/mod.rs
crates/assay-core/src/mcp/proxy/client.rs
crates/assay-core/src/mcp/proxy/decisions.rs
crates/assay-evidence/src/lint/packs/loader_internal/resolve.rs
crates/assay-evidence/src/types.rs
crates/assay-evidence/tests/pack_engine_manual_test.rs
crates/assay-mcp-server/tests/auth_integration.rs
crates/assay-policy/src/tiers.rs
crates/assay-registry/src/verify_internal/tests/digest.rs
```

Forbidden in this PR:

```text
Cargo.toml
Cargo.lock
crates/assay-ebpf/
crates/assay-monitor/
crates/assay-runner-core/
crates/assay-runner-linux/
crates/assay-runner-schema/
crates/assay-xtask/
crates/assay-cli/src/cli/commands/runner_spike.rs
crates/assay-cli/src/cgroup.rs
.github/workflows/runner-spike-delegated.yml
.github/workflows/runner-spike-sdk.yml
release-profile changes
workspace lint ratchets
edition migration
```

Why: `scripts/ci/assay_runner_lane_check.py` routes runner/eBPF/monitor/xtask and `Cargo.toml`/`Cargo.lock` changes to `gates=all`. Wave56 should avoid that lane. `rust-toolchain.toml` and `.config/nextest.toml` still trigger broad Split Wave 0 gates, so keep the PR small.

## Task 1: Create the branch and verify baseline

**Files:**
- Read: `Cargo.toml`
- Read: `.github/workflows/split-wave0-gates.yml`
- Read: `scripts/ci/assay_runner_lane_check.py`

- [ ] **Step 1: Start from current main**

```bash
git fetch --prune origin
git switch -c codex/wave56-posture origin/main
```

Expected: branch `codex/wave56-posture` starts at `origin/main`.

- [ ] **Step 2: Confirm the baseline signals**

```bash
test ! -f rust-toolchain.toml
test ! -f .config/nextest.toml
rg -n '#\[expect\(' --glob '*.rs' --glob '!crates/assay-ebpf/src/vmlinux.rs' | wc -l
rg -n '#\[allow\(' --glob '*.rs' --glob '!crates/assay-ebpf/src/vmlinux.rs' | wc -l
```

Expected:

```text
0
88
```

- [ ] **Step 3: Confirm this PR avoids runner-lane files**

```bash
python3 scripts/ci/assay_runner_lane_check.py --self-test
```

Expected: self-test exits 0. This proves the lane helper is runnable before adding the Wave56 review gate.

## Task 2: Add an exact Rust toolchain pin

**Files:**
- Create: `rust-toolchain.toml`

- [ ] **Step 1: Add `rust-toolchain.toml`**

Create the file with exactly:

```toml
[toolchain]
channel = "1.96.0"
components = ["clippy", "rustfmt"]
profile = "minimal"
```

Do not list host-specific targets. Do not add eBPF targets in this PR; the eBPF build path is intentionally outside Wave56.

- [ ] **Step 2: Install and verify the pinned toolchain locally**

```bash
rustup toolchain install 1.96.0 --profile minimal --component clippy --component rustfmt
rustc --version
cargo --version
rustfmt --version
cargo clippy --version
```

Expected: each tool reports the 1.96 toolchain. If the local machine already has 1.96.0, `rustup` reports it as installed/up to date.

- [ ] **Step 3: Verify the pin did not require Cargo changes**

```bash
git diff -- Cargo.toml Cargo.lock
```

Expected: no output.

## Task 3: Add repository nextest defaults

**Files:**
- Create: `.config/nextest.toml`

- [ ] **Step 1: Add `.config/nextest.toml`**

Create the file with exactly:

```toml
[profile.default]
failure-output = "immediate-final"
success-output = "never"
slow-timeout = { period = "60s", terminate-after = 2 }
retries = { backoff = "fixed", count = 1, delay = "1s" }
```

Rationale: one fixed retry absorbs transient process/test-runner flakes without hiding deterministic failures. `failure-output = "immediate-final"` preserves actionable logs in CI. `success-output = "never"` keeps logs small.

- [ ] **Step 2: Validate nextest config syntax**

```bash
cargo nextest --version || cargo install --locked cargo-nextest
cargo nextest show-config version
cargo nextest show-config test-groups -p assay-evidence --lib
```

Expected: all commands exit 0. `test-groups` can be empty; this step is only schema/config validation. Keep this package-scoped: the unscoped workspace command can pull in `assay-python-sdk`/PyO3 on local Python 3.14 environments, which is outside Wave56's intended nextest-config smoke.

- [ ] **Step 3: Run a small nextest smoke**

```bash
cargo nextest run -p assay-evidence --lib
```

Expected: assay-evidence library tests pass under the new default profile.

## Task 4: Convert proven `allow` suppressions to `expect`

**Files:**
- Modify: the non-runner, non-eBPF files listed in Strict Scope.

Do not convert cfg-sensitive suppressions in this first posture PR:

```text
dead_code
unused_imports
unused_mut
unused_variables
unsafe_code
unused_assignments
```

Convert only these stable, currently-triggered lint suppressions:

```text
clippy::too_many_arguments
clippy::too_many_lines
clippy::large_enum_variant
clippy::field_reassign_with_default
clippy::needless_range_loop
clippy::manual_is_multiple_of
deprecated
```

- [ ] **Step 1: Convert clippy structural suppressions**

Replace the exact annotations below:

```text
#[allow(clippy::too_many_arguments)] -> #[expect(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)] -> #[expect(clippy::too_many_lines)]
#[allow(clippy::large_enum_variant)] -> #[expect(clippy::large_enum_variant)]
#[allow(clippy::field_reassign_with_default)] -> #[expect(clippy::field_reassign_with_default)]
#[allow(clippy::needless_range_loop)] -> #[expect(clippy::needless_range_loop)]
#[allow(clippy::manual_is_multiple_of)] -> #[expect(clippy::manual_is_multiple_of)]
```

Apply only in:

```text
crates/assay-adapter-a2a/src/adapter_impl/payload.rs
crates/assay-adapter-acp/src/adapter_impl/convert.rs
crates/assay-adapter-ucp/src/adapter_impl/payload.rs
crates/assay-core/src/judge/judge_internal/run.rs
crates/assay-core/src/judge/mod.rs
crates/assay-core/src/mcp/proxy/client.rs
crates/assay-core/src/mcp/proxy/decisions.rs
crates/assay-evidence/src/lint/packs/loader_internal/resolve.rs
crates/assay-evidence/src/types.rs
crates/assay-mcp-server/tests/auth_integration.rs
crates/assay-policy/src/tiers.rs
```

- [ ] **Step 2: Convert deprecated-test suppressions**

Replace:

```text
#[allow(deprecated)] -> #[expect(deprecated)]
```

Apply only in:

```text
crates/assay-evidence/tests/pack_engine_manual_test.rs
crates/assay-registry/src/verify_internal/tests/digest.rs
```

Execution note: Rust/Clippy 1.96 rejected several initial candidates under `-D unfulfilled-lint-expectations`, so they remain `#[allow(...)]` and are excluded from the final diff: CLI-test `deprecated` suppressions, `crates/assay-adapter-ucp/src/adapter_impl/mapping.rs` `clippy::too_many_arguments`, and `crates/assay-core/tests/decision_emit_invariant/fixtures.rs` `clippy::field_reassign_with_default`.

- [ ] **Step 3: Validate no unfulfilled expectations**

```bash
cargo clippy -p assay-core --all-targets -- -D warnings
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo clippy -p assay-policy --all-targets -- -D warnings
cargo clippy -p assay-registry --all-targets -- -D warnings
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo clippy -p assay-mcp-server --all-targets -- -D warnings
cargo clippy -p assay-adapter-a2a --all-targets -- -D warnings
cargo clippy -p assay-adapter-acp --all-targets -- -D warnings
cargo clippy -p assay-adapter-ucp --all-targets -- -D warnings
```

Expected: all commands exit 0. If one `#[expect(...)]` is unfulfilled, revert only that single annotation back to `#[allow(...)]` and rerun the relevant crate clippy command. Do not widen the lint list to make the conversion pass.

## Task 5: Add a Wave56 review gate

**Files:**
- Create: `scripts/ci/review-wave56-posture.sh`

- [ ] **Step 1: Add the review script**

Create the file with exactly:

```bash
#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"

changed_files="$(mktemp)"
unexpected_files="$(mktemp)"
trap 'rm -f "${changed_files}" "${unexpected_files}"' EXIT

{
  git diff --name-only "${BASE_REF}" --
  git ls-files --others --exclude-standard -- \
    .config/nextest.toml \
    docs/superpowers/plans/2026-06-06-wave56-posture.md \
    rust-toolchain.toml \
    scripts/ci/review-wave56-posture.sh
} | sort -u > "${changed_files}"

required_files=(
  ".config/nextest.toml"
  "docs/superpowers/plans/2026-06-06-wave56-posture.md"
  "rust-toolchain.toml"
  "scripts/ci/review-wave56-posture.sh"
)

for path in "${required_files[@]}"; do
  if ! grep -Fxq "${path}" "${changed_files}"; then
    echo "missing required Wave56 file: ${path}" >&2
    exit 1
  fi
done

forbidden_regex='^(Cargo\.toml|Cargo\.lock|crates/assay-ebpf/.*|crates/assay-monitor/.*|crates/assay-runner-core/.*|crates/assay-runner-linux/.*|crates/assay-runner-schema/.*|crates/assay-xtask/.*|crates/assay-cli/src/cli/commands/runner_spike\.rs|crates/assay-cli/src/cgroup\.rs|\.github/workflows/runner-spike-delegated\.yml|\.github/workflows/runner-spike-sdk\.yml)$'

if grep -Eq "${forbidden_regex}" "${changed_files}"; then
  echo "Wave56 touched runner/eBPF/release-lane forbidden paths:" >&2
  grep -E "${forbidden_regex}" "${changed_files}" >&2
  exit 1
fi

allowed_regex='^(\.config/nextest\.toml|docs/superpowers/plans/2026-06-06-wave56-posture\.md|rust-toolchain\.toml|scripts/ci/review-wave56-posture\.sh|crates/assay-adapter-a2a/src/adapter_impl/payload\.rs|crates/assay-adapter-acp/src/adapter_impl/convert\.rs|crates/assay-adapter-ucp/src/adapter_impl/payload\.rs|crates/assay-core/src/judge/judge_internal/run\.rs|crates/assay-core/src/judge/mod\.rs|crates/assay-core/src/mcp/proxy/client\.rs|crates/assay-core/src/mcp/proxy/decisions\.rs|crates/assay-evidence/src/lint/packs/loader_internal/resolve\.rs|crates/assay-evidence/src/types\.rs|crates/assay-evidence/tests/pack_engine_manual_test\.rs|crates/assay-mcp-server/tests/auth_integration\.rs|crates/assay-policy/src/tiers\.rs|crates/assay-registry/src/verify_internal/tests/digest\.rs)$'

if grep -Ev "${allowed_regex}" "${changed_files}" > "${unexpected_files}"; then
  echo "Wave56 touched paths outside the allowlist:" >&2
  cat "${unexpected_files}" >&2
  exit 1
fi

if ! grep -Fq 'channel = "1.96.0"' rust-toolchain.toml; then
  echo "rust-toolchain.toml must pin channel = \"1.96.0\"" >&2
  exit 1
fi

if ! grep -Fq 'components = ["clippy", "rustfmt"]' rust-toolchain.toml; then
  echo "rust-toolchain.toml must install clippy and rustfmt" >&2
  exit 1
fi

if ! grep -Fq 'retries = { backoff = "fixed", count = 1, delay = "1s" }' .config/nextest.toml; then
  echo ".config/nextest.toml must keep the scoped one-retry policy" >&2
  exit 1
fi

if grep -Eq '(^|/)Cargo\.(toml|lock)$' "${changed_files}"; then
  echo "Wave56 must not change Cargo.toml or Cargo.lock" >&2
  exit 1
fi

expect_count="$(rg -n '#\[expect\(' --glob '*.rs' --glob '!crates/assay-ebpf/src/vmlinux.rs' | wc -l | tr -d ' ')"
if [[ "${expect_count}" -lt 1 ]]; then
  echo "expected at least one #[expect(...)] migration" >&2
  exit 1
fi

rustc --version | grep -Fq 'rustc 1.96.0'
cargo --version | grep -Fq 'cargo 1.96.0'
rustfmt --version | grep -Fq 'rustfmt 1.9.0'
cargo clippy --version | grep -Fq 'clippy 0.1.96'

cargo fmt --check
cargo check -p assay-core
cargo check -p assay-evidence
cargo check -p assay-policy
cargo check -p assay-registry
cargo check -p assay-cli
cargo check -p assay-mcp-server
cargo check -p assay-adapter-a2a
cargo check -p assay-adapter-acp
cargo check -p assay-adapter-ucp
cargo nextest --version || cargo install --locked cargo-nextest
cargo nextest show-config version
cargo nextest show-config test-groups -p assay-evidence --lib
cargo nextest run -p assay-evidence --lib
cargo clippy -p assay-core --all-targets -- -D warnings
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo clippy -p assay-policy --all-targets -- -D warnings
cargo clippy -p assay-registry --all-targets -- -D warnings
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo clippy -p assay-mcp-server --all-targets -- -D warnings
cargo clippy -p assay-adapter-a2a --all-targets -- -D warnings
cargo clippy -p assay-adapter-acp --all-targets -- -D warnings
cargo clippy -p assay-adapter-ucp --all-targets -- -D warnings

git diff --check "${BASE_REF}" --
while IFS= read -r path; do
  if [[ -f "${path}" ]] && grep -n '[[:blank:]]$' "${path}"; then
    echo "trailing whitespace in ${path}" >&2
    exit 1
  fi
done < "${changed_files}"
```

- [ ] **Step 2: Make the script executable**

```bash
chmod +x scripts/ci/review-wave56-posture.sh
```

- [ ] **Step 3: Run the script**

```bash
BASE_REF=origin/main bash scripts/ci/review-wave56-posture.sh
```

Expected: exits 0.

## Task 6: Full verification and PR

**Files:**
- Read: all changed files
- Write: Git commit and PR metadata only

- [ ] **Step 1: Run local verification**

```bash
cargo fmt --check
BASE_REF=origin/main bash scripts/ci/review-wave56-posture.sh
git diff --check origin/main --
```

Expected: all commands exit 0.

- [ ] **Step 2: Inspect changed files**

```bash
git diff --name-only origin/main...HEAD
```

Expected: changed files are limited to the Strict Scope allowlist.

- [ ] **Step 3: Commit**

```bash
git add rust-toolchain.toml .config/nextest.toml scripts/ci/review-wave56-posture.sh \
  docs/superpowers/plans/2026-06-06-wave56-posture.md \
  crates/assay-adapter-a2a/src/adapter_impl/payload.rs \
  crates/assay-adapter-acp/src/adapter_impl/convert.rs \
  crates/assay-adapter-ucp/src/adapter_impl/payload.rs \
  crates/assay-core/src/judge/judge_internal/run.rs \
  crates/assay-core/src/judge/mod.rs \
  crates/assay-core/src/mcp/proxy/client.rs \
  crates/assay-core/src/mcp/proxy/decisions.rs \
  crates/assay-evidence/src/lint/packs/loader_internal/resolve.rs \
  crates/assay-evidence/src/types.rs \
  crates/assay-evidence/tests/pack_engine_manual_test.rs \
  crates/assay-mcp-server/tests/auth_integration.rs \
  crates/assay-policy/src/tiers.rs \
  crates/assay-registry/src/verify_internal/tests/digest.rs
git commit -m "chore(tooling): pin Rust and ratchet lint expectations"
```

- [ ] **Step 4: Open a draft PR**

```bash
git push -u origin codex/wave56-posture
gh pr create \
  --repo Rul1an/assay \
  --base main \
  --head codex/wave56-posture \
  --draft \
  --title "chore(tooling): pin Rust and ratchet lint expectations" \
  --body "$(cat <<'PR_BODY'
## Summary

Wave56 posture adds deterministic Rust tooling and stale-lint-suppression hygiene without changing runtime behavior:

- pin Rust to 1.96.0 with clippy/rustfmt components
- add repository nextest defaults with one fixed retry and concise output
- migrate proven non-runner/non-eBPF `#[allow(...)]` suppressions to `#[expect(...)]`

## Scope

No `Cargo.toml`, `Cargo.lock`, eBPF, monitor, runner, xtask, release-profile, workspace-lint, or edition changes.

## Verification

- `cargo fmt --check`
- `BASE_REF=origin/main bash scripts/ci/review-wave56-posture.sh`
- `git diff --check origin/main --`

## Gate expectation

This PR should not require Runner Spike Delegated `gates=all` because it avoids the runner/eBPF/monitor/xtask and Cargo manifest surfaces. It will trigger broad Split Wave 0 gates because `rust-toolchain.toml` is a global config path.
PR_BODY
)"
```

- [ ] **Step 5: Mark ready only after CI**

Wait for GitHub PR checks. Required before ready:

```text
lane-check: pass or no delegated proof required
Split Wave 0 feature matrix: pass
Split Wave 0 quality gates: pass
Split Wave 0 semver checks: pass
CI: pass
MCP Security: pass
Perf PR compare: pass
review threads: 0 unresolved
```

If lane-check unexpectedly requires delegated proof, stop and inspect changed files against `scripts/ci/assay_runner_lane_check.py`; do not dispatch `gates=all` until the unexpected classification is understood.

## Non-goals and next waves

Do not do these in Wave56:

```text
[profile.release] lto/codegen-units changes
workspace unwrap_used ratchets
workspace pedantic ratchets
unsafe-doc lints workspace-wide
edition 2024 migration
eBPF artifact caching
runner VM/proof infrastructure changes
```

Recommended follow-up:

```text
Wave57 release/lint:
- measure `lto = "thin"` plus `codegen-units = 1` with binary size, timings, and criterion/hyperfine
- ratchet `unwrap_used = "warn"` crate-by-crate for assay-evidence, assay-policy, assay-registry, and runtime assay-core
- run Runner Spike Delegated gates=all if Cargo.toml or eBPF-adjacent build behavior changes
```

Edition 2024 remains a later standalone wave after unsafe and lint posture are stable.

## Self-review

Spec coverage:

```text
toolchain pin: Task 2
nextest config: Task 3
allow-to-expect migration: Task 4
no eBPF proof needed: Strict Scope, Task 5, Task 6
release-profile measurement deferred: Non-goals and next waves
runner instability not touched: Strict Scope and Non-goals
```

Placeholder scan:

```text
No incomplete markers remain.
All commands are concrete.
All files are explicitly named.
```

Risk review:

```text
The risky broad items from the pasted analysis are deliberately excluded.
Rust 1.96.0 may require CI cache refresh, but that is expected for a toolchain pin.
`#[expect(...)]` migration is intentionally limited to lint classes that should be fulfilled today; unfulfilled expectations are handled by reverting that single annotation, not by broadening suppressions.
```
