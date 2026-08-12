#!/usr/bin/env bash
# Self-test for scripts/ci/cargo-audit-with-isolated-db.sh (CI-4F / #2188).
#
# cargo-deny nests under ~/.cargo/advisory-db; cargo-audit refuses that non-empty
# root. CI used `rm -rf` globally; the local hook did not. This gate proves one
# runner isolates --db, preserves a planted hostile deny layout, and is the only
# active audit path in CI + pre-commit.
#
# shellcheck disable=SC2016,SC2088 # intentional literal YAML/toml/shell spellings under test
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUNNER="${ROOT}/scripts/ci/cargo-audit-with-isolated-db.sh"
DENY_TOML="${ROOT}/deny.toml"
WORKFLOW="${ROOT}/.github/workflows/ci.yml"
PRECOMMIT="${ROOT}/.pre-commit-config.yaml"
RUNNER_REL='scripts/ci/cargo-audit-with-isolated-db.sh'

fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { echo "ok   $*"; }
abort_is_failure() {
  local rc="$1"
  [[ "${rc}" -eq 0 ]] || echo "cargo-audit isolated-db self-test aborted (exit ${rc}); treat as failure" >&2
}
trap 'abort_is_failure "$?"' ERR

# macOS /bin/bash 3.2: refuse bash-4.4 @Q in active code.
if awk '
  /^[[:space:]]*#/ { next }
  /\$\{[^}]+@Q\}/ { found=1; print NR ":" $0 }
  END { exit found ? 0 : 1 }
' "${BASH_SOURCE[0]}"; then
  fail "self-test uses bash-4.4 @Q quoting; macOS bash 3.2 aborts with bad substitution"
fi

[[ -f "${RUNNER}" ]] || fail "missing runner ${RUNNER_REL}"
[[ -f "${DENY_TOML}" && -f "${WORKFLOW}" && -f "${PRECOMMIT}" ]] || fail "missing wiring files"

grep -q 'BASH_SOURCE\[0\]' "${RUNNER}" \
  || fail "runner is not source-safe (missing BASH_SOURCE execute guard)"
grep -qE '^assay_cargo_audit_db_path[[:space:]]*\(\)' "${RUNNER}" \
  || fail "runner must define assay_cargo_audit_db_path()"
grep -qE '^run_cargo_audit_with_isolated_db[[:space:]]*\(\)' "${RUNNER}" \
  || fail "runner must define run_cargo_audit_with_isolated_db()"
grep -qF 'exec cargo-audit audit --db' "${RUNNER}" \
  || fail "runner must exec cargo-audit audit --db \"\$db\""

# shellcheck source=scripts/ci/cargo-audit-with-isolated-db.sh
source "${RUNNER}"

SCRATCH="$(mktemp -d)"
trap 'rm -rf "${SCRATCH}"' EXIT

make_stub_cargo_audit() {
  local bin_dir="$1" exit_code="$2"
  mkdir -p "${bin_dir}"
  cat >"${bin_dir}/cargo-audit" <<STUB
#!/usr/bin/env bash
set -euo pipefail
printf 'argv:' >>"\${STUB_LOG}"
for arg in "\$@"; do printf ' %s' "\$arg" >>"\${STUB_LOG}"; done
printf '\n' >>"\${STUB_LOG}"
printf 'cwd=%s\n' "\$(pwd -P)" >>"\${STUB_LOG}"
exit ${exit_code}
STUB
  chmod +x "${bin_dir}/cargo-audit"
}

plant_hostile_deny_layout() {
  local singular_root="$1"
  mkdir -p "${singular_root}/advisory-db-deadbeefcafe"
  printf 'deny-owned-sentinel\n' >"${singular_root}/advisory-db-deadbeefcafe/SENTINEL"
  printf 'lock\n' >"${singular_root}/db.lock"
}

assert_sentinel_intact() {
  local singular_root="$1"
  [[ -f "${singular_root}/advisory-db-deadbeefcafe/SENTINEL" ]] \
    || fail "hostile deny layout sentinel was deleted (global cleanup must not run)"
  [[ "$(cat "${singular_root}/advisory-db-deadbeefcafe/SENTINEL")" == "deny-owned-sentinel" ]] \
    || fail "hostile deny layout sentinel contents changed"
  [[ -f "${singular_root}/db.lock" ]] || fail "hostile deny db.lock was deleted"
}

parse_db_arg() {
  awk '/^argv: audit --db / { for (i = 1; i <= NF; i++) if ($i == "--db") { print $(i+1); exit } }' "$1"
}

run_runner_case() {
  local name="$1" expect_exit="$2"
  shift 2
  local case_dir="${SCRATCH}/${name}"
  local home="${case_dir}/home" cargo_home="${case_dir}/cargo-home" bin="${case_dir}/bin"
  local log="${case_dir}/stub.log" singular="${home}/.cargo/advisory-db"
  mkdir -p "${home}" "${cargo_home}" "${bin}" "${case_dir}/tmp"
  : >"${log}"
  plant_hostile_deny_layout "${singular}"
  make_stub_cargo_audit "${bin}" "${expect_exit}"
  local rc=0
  env -i \
    PATH="${bin}:/usr/bin:/bin" HOME="${home}" CARGO_HOME="${cargo_home}" \
    STUB_LOG="${log}" TMPDIR="${case_dir}/tmp" "$@" \
    bash "${RUNNER}" --quiet --file "${case_dir}/lock" \
    >"${case_dir}/out.txt" 2>&1 || rc=$?
  [[ "${rc}" -eq "${expect_exit}" ]] \
    || fail "${name}: expected exit ${expect_exit}, got ${rc}; out:
$(cat "${case_dir}/out.txt")
log:
$(cat "${log}")"
  assert_sentinel_intact "${singular}"
  printf '%s\n' "${log}"
}

log="$(run_runner_case default_isolated 0)"
grep -q 'argv: audit --db ' "${log}" || fail "default run must reach cargo-audit with audit --db; log:
$(cat "${log}")"
db_arg="$(parse_db_arg "${log}")"
[[ -n "${db_arg}" ]] || fail "could not parse --db from stub log"
[[ "${db_arg}" != *'/.cargo/advisory-db' ]] \
  || fail "default --db must not be the shared singular root; got ${db_arg}"
case "${db_arg}" in
  */assay/*|"${SCRATCH}"/*) ;;
  *) fail "default --db must be assay-owned under CARGO_HOME or temp; got ${db_arg}" ;;
esac
[[ -d "$(dirname "${db_arg}")" ]] || fail "runner must mkdir parent of --db"
grep -qF -- '--quiet --file ' "${log}" || fail "runner must pass through caller args; log:
$(cat "${log}")"
ok "default isolated --db reaches audit; hostile singular root preserved"

override_db="${SCRATCH}/override-db/custom-advisory-db"
log="$(run_runner_case override_env 0 ASSAY_CARGO_AUDIT_DB="${override_db}")"
grep -qF "argv: audit --db ${override_db}" "${log}" \
  || fail "ASSAY_CARGO_AUDIT_DB must become --db; log:
$(cat "${log}")"
[[ -d "$(dirname "${override_db}")" ]] || fail "override path parent must be created"
ok "ASSAY_CARGO_AUDIT_DB override respected"

log="$(run_runner_case exit_passthrough 7)"
ok "cargo-audit exit code passed through (7)"

# Fuzz cwd/--file: runner must not cd away from caller cwd.
fuzz_cwd="${SCRATCH}/fuzz-cwd"
mkdir -p "${fuzz_cwd}" "${SCRATCH}/fuzz-home" "${SCRATCH}/fuzz-bin" "${SCRATCH}/fuzz-cargo"
: >"${SCRATCH}/fuzz.log"
plant_hostile_deny_layout "${SCRATCH}/fuzz-home/.cargo/advisory-db"
make_stub_cargo_audit "${SCRATCH}/fuzz-bin" 0
fuzz_rc=0
(
  cd "${fuzz_cwd}"
  env -i \
    PATH="${SCRATCH}/fuzz-bin:/usr/bin:/bin" \
    HOME="${SCRATCH}/fuzz-home" CARGO_HOME="${SCRATCH}/fuzz-cargo" \
    STUB_LOG="${SCRATCH}/fuzz.log" ASSAY_CARGO_AUDIT_DB="${SCRATCH}/fuzz-db/advisory-db" \
    bash "${RUNNER}" --file "${ROOT}/fuzz/Cargo.lock"
) || fuzz_rc=$?
[[ "${fuzz_rc}" -eq 0 ]] || fail "fuzz-cwd runner failed (exit ${fuzz_rc})"
grep -qF "cwd=$(cd "${fuzz_cwd}" && pwd -P)" "${SCRATCH}/fuzz.log" \
  || fail "runner must preserve caller cwd; log:
$(cat "${SCRATCH}/fuzz.log")"
grep -qF -- "--file ${ROOT}/fuzz/Cargo.lock" "${SCRATCH}/fuzz.log" \
  || fail "fuzz --file arg must pass through; log:
$(cat "${SCRATCH}/fuzz.log")"
assert_sentinel_intact "${SCRATCH}/fuzz-home/.cargo/advisory-db"
ok "fuzz cwd/--file semantics preserved"

deps_security_job() {
  awk '
    /^  deps-security:[[:space:]]*$/ { in_job=1; print; next }
    in_job && /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { exit }
    in_job { print }
  ' "$1"
}

job="$(deps_security_job "${WORKFLOW}")"
[[ -n "${job}" ]] || fail "could not find deps-security job"
active_job="$(printf '%s\n' "${job}" | grep -v '^[[:space:]]*#' || true)"
if printf '%s\n' "${active_job}" | grep -qE 'rm[[:space:]]+-rf[[:space:]]+.*advisory-db'; then
  fail "deps-security still has active rm -rf …advisory-db"
fi
root_hits="$(printf '%s\n' "${active_job}" | grep -cF "${RUNNER_REL}" || true)"
[[ "${root_hits}" -ge 2 ]] \
  || fail "deps-security must invoke ${RUNNER_REL} for both audits; active hits=${root_hits}"
if printf '%s\n' "${active_job}" | grep -E '^[[:space:]]*cargo-audit[[:space:]]+audit([[:space:]]|$)'; then
  fail "deps-security still has a bare cargo-audit audit line"
fi
printf '%s\n' "${active_job}" | grep -qF 'cd "${RUNNER_TEMP}"' \
  || fail "fuzz audit must still cd to RUNNER_TEMP"
printf '%s\n' "${active_job}" | grep -qF -- '--file "${GITHUB_WORKSPACE}/fuzz/Cargo.lock"' \
  || fail "fuzz audit must still pass --file for fuzz/Cargo.lock"

hook_entry="$(awk '
  /^[[:space:]]*- id: cargo-audit[[:space:]]*$/ { in_hook=1; next }
  in_hook && /^[[:space:]]*- id:/ { exit }
  in_hook && /^[[:space:]]*entry:/ { print; exit }
' "${PRECOMMIT}")"
[[ "${hook_entry}" == *"${RUNNER_REL}"* ]] \
  || fail "pre-commit cargo-audit entry must use ${RUNNER_REL}; got:
${hook_entry:-<missing>}"
if grep -qE "entry:[[:space:]]*bash -lc 'cargo-audit audit'" "${PRECOMMIT}"; then
  fail "pre-commit still has bare bash -lc 'cargo-audit audit'"
fi
ok "CI + pre-commit share the isolated runner; no global rm"

deny_db_path="$(awk '
  /^\[advisories\]/ { in_adv=1; next }
  in_adv && /^\[/ { exit }
  in_adv && /^db-path[[:space:]]*=/ {
    sub(/^db-path[[:space:]]*=[[:space:]]*/, "")
    gsub(/^"/, ""); gsub(/"$/, "")
    print; exit
  }
' "${DENY_TOML}")"
[[ "${deny_db_path}" == '$CARGO_HOME/advisory-dbs' ]] \
  || fail "deny.toml db-path must be \$CARGO_HOME/advisory-dbs (plural); got '${deny_db_path:-<missing>}'"
ok "deny.toml uses plural \$CARGO_HOME/advisory-dbs"

if awk '
  /^\[advisories\]/ { in_adv=1; next }
  in_adv && /^\[/ { in_adv=0 }
  in_adv && /RUSTSEC-2023-0071/ { found=1 }
  END { exit found ? 0 : 1 }
' "${DENY_TOML}" "${ROOT}/.cargo/audit.toml"; then
  fail "RUSTSEC-2023-0071 reintroduced in deny.toml or .cargo/audit.toml"
fi
ok "RUSTSEC-2023-0071 ignore remains absent"

# --- Mutations that must bite ---

mut_runner="${SCRATCH}/mut-runner-no-db.sh"
sed 's/exec cargo-audit audit --db "${db}"/exec cargo-audit audit/' "${RUNNER}" >"${mut_runner}"
grep -qF 'exec cargo-audit audit --db' "${mut_runner}" && fail "mutant still contains --db"
mut_bin="${SCRATCH}/mut-nodb/bin"
mut_log="${SCRATCH}/mut-nodb/stub.log"
mkdir -p "${SCRATCH}/mut-nodb/home" "${SCRATCH}/mut-nodb/cargo-home" "${mut_bin}"
: >"${mut_log}"
plant_hostile_deny_layout "${SCRATCH}/mut-nodb/home/.cargo/advisory-db"
make_stub_cargo_audit "${mut_bin}" 0
env -i PATH="${mut_bin}:/usr/bin:/bin" HOME="${SCRATCH}/mut-nodb/home" \
  CARGO_HOME="${SCRATCH}/mut-nodb/cargo-home" STUB_LOG="${mut_log}" \
  bash "${mut_runner}" >"${SCRATCH}/mut-nodb/out.txt" 2>&1 || true
grep -q 'argv: audit --db ' "${mut_log}" \
  && fail "mutation removing --db still showed --db in argv"
assert_sentinel_intact "${SCRATCH}/mut-nodb/home/.cargo/advisory-db"
ok "mutation: removing --db from runner is detectable"

mut_deny="${SCRATCH}/mut-deny.toml"
sed 's|db-path = "\$CARGO_HOME/advisory-dbs"|db-path = "~/.cargo/advisory-db"|' "${DENY_TOML}" >"${mut_deny}"
mut_deny_path="$(awk '
  /^\[advisories\]/ { in_adv=1; next }
  in_adv && /^\[/ { exit }
  in_adv && /^db-path[[:space:]]*=/ {
    sub(/^db-path[[:space:]]*=[[:space:]]*/, "")
    gsub(/^"/, ""); gsub(/"$/, "")
    print; exit
  }
' "${mut_deny}")"
[[ "${mut_deny_path}" == '~/.cargo/advisory-db' ]] \
  || fail "deny.toml singular mutation did not apply; got '${mut_deny_path}'"
ok "mutation: deny.toml shared singular db-path is detectable"

mut_wf="${SCRATCH}/mut-ci.yml"
deps_security_job "${WORKFLOW}" >"${mut_wf}"
python3 - "${mut_wf}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
needle = "bash scripts/ci/cargo-audit-with-isolated-db.sh"
if needle not in text:
    raise SystemExit("could not find runner invocation to mutate")
path.write_text(text.replace(needle, "rm -rf ~/.cargo/advisory-db\n          cargo-audit audit", 1))
PY
grep -v '^[[:space:]]*#' "${mut_wf}" | grep -qE 'rm[[:space:]]+-rf[[:space:]]+.*advisory-db' \
  || fail "workflow rm -rf mutation did not apply"
ok "mutation: restoring rm -rf ~/.cargo/advisory-db is detectable"

mut_pc="${SCRATCH}/mut-pre-commit.yaml"
sed "s|entry: bash ${RUNNER_REL}|entry: bash -lc 'cargo-audit audit'|" "${PRECOMMIT}" >"${mut_pc}"
grep -qE "entry:[[:space:]]*bash -lc 'cargo-audit audit'" "${mut_pc}" \
  || fail "pre-commit bare-audit mutation did not apply"
ok "mutation: pre-commit bare cargo-audit bypass is detectable"

# Hostile cargo-audit layout control for deny separation (config/static, no network).
case "${deny_db_path}" in
  '~/.cargo/advisory-db'|*/advisory-db)
    fail "deny.toml collides with cargo-audit default singular root (${deny_db_path})"
    ;;
  '$CARGO_HOME/advisory-dbs') ;;
  *) fail "deny.toml db-path unexpected: ${deny_db_path}" ;;
esac
ok "hostile layout control: deny plural root separated from cargo-audit singular"

echo "ALL cargo-audit isolated-db self-tests passed"
