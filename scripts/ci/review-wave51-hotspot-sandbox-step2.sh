#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

SANDBOX="crates/assay-cli/src/cli/commands/sandbox.rs"
CHILD="crates/assay-cli/src/cli/commands/sandbox/child.rs"
DEGRADATION="crates/assay-cli/src/cli/commands/sandbox/degradation.rs"
ENV_MOD="crates/assay-cli/src/cli/commands/sandbox/env.rs"
PROFILE="crates/assay-cli/src/cli/commands/sandbox/profile.rs"
TMP_MOD="crates/assay-cli/src/cli/commands/sandbox/tmp.rs"

echo "[review] workflow and generated-file guard"
if ! git diff --quiet -- .github/workflows; then
  echo "FAIL: Wave 51 Sandbox Step2 must not touch workflows"
  exit 1
fi
if ! git diff --quiet -- crates/assay-ebpf/src/vmlinux.rs; then
  echo "FAIL: generated vmlinux.rs must stay out of scope"
  exit 1
fi

echo "[review] facade thinness"
sandbox_code_lines="$(
  awk 'BEGIN{n=0; in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} !in_tests{n++} END{print n}' "$SANDBOX"
)"
echo "sandbox non-test lines: $sandbox_code_lines"
if [ "$sandbox_code_lines" -gt 250 ]; then
  echo "FAIL: sandbox facade is too thick"
  exit 1
fi

echo "[review] boundary markers"
rg -n 'mod child;|mod degradation;|mod env;|mod profile;|mod tmp;' "$SANDBOX" >/dev/null || {
  echo "FAIL: sandbox submodule declarations missing"
  exit 1
}
rg -n 'run_child\(' "$SANDBOX" >/dev/null || {
  echo "FAIL: sandbox facade does not delegate child execution"
  exit 1
}
rg -n 'tokio::process::Command|timeout|TMPDIR|maybe_profile_finish' "$CHILD" >/dev/null || {
  echo "FAIL: child execution markers missing"
  exit 1
}
rg -n 'EnvFilter::(passthrough|strict|default)|with_strip_exec|with_allowed|with_safe_path' "$ENV_MOD" >/dev/null || {
  echo "FAIL: env filter markers missing"
  exit 1
}
rg -n 'PayloadSandboxDegraded|BackendUnavailable|PolicyConflict' "$DEGRADATION" >/dev/null || {
  echo "FAIL: degradation markers missing"
  exit 1
}
rg -n 'save_atomic|evidence_profile_run_id|to_evidence_profile' "$PROFILE" >/dev/null || {
  echo "FAIL: profile finish markers missing"
  exit 1
}
rg -n 'XDG_RUNTIME_DIR|create_dir|remove_dir_all|set_permissions|0o700' "$TMP_MOD" >/dev/null || {
  echo "FAIL: scoped tmp markers missing"
  exit 1
}

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-cli
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo test -p assay-cli sandbox
cargo test -p assay-cli --test profile_integration_test
git diff --check

echo "[review] PASS"
