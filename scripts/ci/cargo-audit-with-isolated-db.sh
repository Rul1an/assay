#!/usr/bin/env bash
# Run cargo-audit against an isolated advisory DB (CI-4F / #2188).
#
# cargo-deny's default root is $CARGO_HOME/advisory-dbs (plural). cargo-audit's
# default is ~/.cargo/advisory-db (singular). Sharing the singular root makes
# sequential hooks fail: deny nests a hashed clone, audit refuses the non-empty
# directory. This runner is the one --db rule for the local pre-push hook and
# both CI audits. Override with ASSAY_CARGO_AUDIT_DB when a caller needs a
# specific path (tests, or an operator-chosen cache).
#
# Bash 3.2 compatible (macOS /bin/bash). Source-safe: sourcing defines helpers
# only and must not change the caller's shell options; executing enables
# strict mode then runs the audit.

# Resolve the advisory DB path. Prefer an explicit override; otherwise a bounded
# assay-owned leaf under CARGO_HOME (outside any worktree). Fall back to TMPDIR
# when neither CARGO_HOME nor HOME is available.
assay_cargo_audit_db_path() {
  if [[ -n "${ASSAY_CARGO_AUDIT_DB:-}" ]]; then
    printf '%s\n' "${ASSAY_CARGO_AUDIT_DB}"
    return 0
  fi

  local base
  if [[ -n "${CARGO_HOME:-}" ]]; then
    base="${CARGO_HOME}"
  elif [[ -n "${HOME:-}" ]]; then
    base="${HOME}/.cargo"
  else
    base="${TMPDIR:-/tmp}/assay-cargo-audit"
  fi
  printf '%s\n' "${base}/assay/cargo-audit/advisory-db"
}

run_cargo_audit_with_isolated_db() {
  local db parent
  db="$(assay_cargo_audit_db_path)"
  parent="$(dirname "${db}")"
  mkdir -p "${parent}"
  exec cargo-audit audit --db "${db}" "$@"
}

# Strict mode belongs to execution only. A top-level `set -euo pipefail` would
# mutate a caller's options on `source` and break source-safety.
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  set -euo pipefail
  run_cargo_audit_with_isolated_db "$@"
fi
