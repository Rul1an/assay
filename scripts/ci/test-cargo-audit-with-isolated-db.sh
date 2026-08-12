#!/usr/bin/env bash
# Self-test for scripts/ci/cargo-audit-with-isolated-db.sh (CI-4F / #2188).
# shellcheck disable=SC2016,SC2088 # intentional literal YAML/toml/shell spellings
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUNNER="${ROOT}/scripts/ci/cargo-audit-with-isolated-db.sh"
DENY_TOML="${ROOT}/deny.toml"
WORKFLOW="${ROOT}/.github/workflows/ci.yml"
PRECOMMIT="${ROOT}/.pre-commit-config.yaml"
RUNNER_REL='scripts/ci/cargo-audit-with-isolated-db.sh'

fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { echo "ok   $*"; }
trap 'rc=$?; [[ $rc -eq 0 ]] || echo "cargo-audit isolated-db self-test aborted (exit ${rc}); treat as failure" >&2' ERR

if awk '
  /^[[:space:]]*#/ { next }
  /\$\{[^}]+@Q\}/ { found=1; print NR ":" $0 }
  END { exit found ? 0 : 1 }
' "${BASH_SOURCE[0]}"; then
  fail "self-test uses bash-4.4 @Q quoting; macOS bash 3.2 aborts with bad substitution"
fi
[[ -f "${RUNNER}" && -f "${DENY_TOML}" && -f "${WORKFLOW}" && -f "${PRECOMMIT}" ]] \
  || fail "missing runner or wiring files"

# Shared runner assertion (live GREEN + --db mutant RED).
assert_runner_exec_uses_db() {
  local path="$1"
  grep -q 'BASH_SOURCE\[0\]' "${path}" \
    || { echo "FAIL [runner-db]: ${path}: missing BASH_SOURCE execute guard" >&2; return 1; }
  grep -qE '^assay_cargo_audit_db_path[[:space:]]*\(\)' "${path}" \
    || { echo "FAIL [runner-db]: ${path}: missing assay_cargo_audit_db_path()" >&2; return 1; }
  grep -qE '^run_cargo_audit_with_isolated_db[[:space:]]*\(\)' "${path}" \
    || { echo "FAIL [runner-db]: ${path}: missing run_cargo_audit_with_isolated_db()" >&2; return 1; }
  if awk '
    /^[[:space:]]*#/ { next }
    /BASH_SOURCE\[0\]/ { guarded=1 }
    !guarded && /^[[:space:]]*set[[:space:]]+-[a-z]*[euo]/ { found=1 }
    END { exit found ? 0 : 1 }
  ' "${path}"; then
    echo "FAIL [runner-db]: ${path}: strict mode before execute guard" >&2
    return 1
  fi
  grep -qF 'exec cargo-audit audit --db' "${path}" \
    || { echo "FAIL [runner-db]: ${path}: must exec cargo-audit audit --db \"\$db\"" >&2; return 1; }
}

assert_runner_exec_uses_db "${RUNNER}" || fail "live runner failed [runner-db]"

# Option probe in a non-strict shell so a leaked top-level set -euo bites.
bash -c '
  set +e; set +u; set +o pipefail
  before="$(set +o; printf "DASH:%s\n" "$-")"
  source "$1"
  after="$(set +o; printf "DASH:%s\n" "$-")"
  [[ "${before}" == "${after}" ]] || {
    echo "FAIL: sourcing runner mutated shell options" >&2
    printf "before:\n%s\nafter:\n%s\n" "${before}" "${after}" >&2
    exit 1
  }
  type assay_cargo_audit_db_path >/dev/null 2>&1 \
    || { echo "FAIL: assay_cargo_audit_db_path missing after source" >&2; exit 1; }
  type run_cargo_audit_with_isolated_db >/dev/null 2>&1 \
    || { echo "FAIL: run_cargo_audit_with_isolated_db missing after source" >&2; exit 1; }
' bash "${RUNNER}" || fail "source-safety option probe failed"
# shellcheck source=scripts/ci/cargo-audit-with-isolated-db.sh
source "${RUNNER}"
ok "source-safe: options unchanged; helpers defined"

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
    || fail "hostile deny sentinel deleted"
  [[ "$(cat "${singular_root}/advisory-db-deadbeefcafe/SENTINEL")" == "deny-owned-sentinel" ]] \
    || fail "hostile deny sentinel contents changed"
  [[ -f "${singular_root}/db.lock" ]] || fail "hostile deny db.lock deleted"
}

parse_db_arg() {
  awk '/^argv: audit --db / { for (i = 1; i <= NF; i++) if ($i == "--db") { print $(i+1); exit } }' "$1"
}

run_runner_case() {
  local name="$1" expect_exit="$2"
  shift 2
  local case_dir="${SCRATCH}/${name}"
  local home="${case_dir}/home" cargo_home="${case_dir}/cargo-home" bin="${case_dir}/bin"
  local log="${case_dir}/stub.log" singular="${home}/.cargo/advisory-db" rc=0
  mkdir -p "${home}" "${cargo_home}" "${bin}" "${case_dir}/tmp"
  : >"${log}"
  plant_hostile_deny_layout "${singular}"
  make_stub_cargo_audit "${bin}" "${expect_exit}"
  env -i PATH="${bin}:/usr/bin:/bin" HOME="${home}" CARGO_HOME="${cargo_home}" \
    STUB_LOG="${log}" TMPDIR="${case_dir}/tmp" "$@" \
    bash "${RUNNER}" --quiet --file "${case_dir}/lock" \
    >"${case_dir}/out.txt" 2>&1 || rc=$?
  [[ "${rc}" -eq "${expect_exit}" ]] || fail "${name}: exit ${rc} want ${expect_exit}
$(cat "${case_dir}/out.txt")
$(cat "${log}")"
  assert_sentinel_intact "${singular}"
  printf '%s\n' "${log}"
}

log="$(run_runner_case default_isolated 0)"
grep -q 'argv: audit --db ' "${log}" || fail "missing audit --db; log: $(cat "${log}")"
db_arg="$(parse_db_arg "${log}")"
[[ -n "${db_arg}" && "${db_arg}" != *'/.cargo/advisory-db' ]] \
  || fail "bad default --db: ${db_arg:-<empty>}"
case "${db_arg}" in */assay/*|"${SCRATCH}"/*) ;; *) fail "default --db not assay-owned: ${db_arg}" ;; esac
[[ -d "$(dirname "${db_arg}")" ]] || fail "missing --db parent"
grep -qF -- '--quiet --file ' "${log}" || fail "args not passed through"
ok "default isolated --db; hostile singular root preserved"

override_db="${SCRATCH}/override-db/custom-advisory-db"
log="$(run_runner_case override_env 0 ASSAY_CARGO_AUDIT_DB="${override_db}")"
grep -qF "argv: audit --db ${override_db}" "${log}" || fail "override ignored"
[[ -d "$(dirname "${override_db}")" ]] || fail "override parent missing"
ok "ASSAY_CARGO_AUDIT_DB override respected"

log="$(run_runner_case exit_passthrough 7)"
ok "exit code passthrough (7)"

fuzz_cwd="${SCRATCH}/fuzz-cwd"
mkdir -p "${fuzz_cwd}" "${SCRATCH}/fuzz-home" "${SCRATCH}/fuzz-bin" "${SCRATCH}/fuzz-cargo"
: >"${SCRATCH}/fuzz.log"
plant_hostile_deny_layout "${SCRATCH}/fuzz-home/.cargo/advisory-db"
make_stub_cargo_audit "${SCRATCH}/fuzz-bin" 0
fuzz_rc=0
(
  cd "${fuzz_cwd}"
  env -i PATH="${SCRATCH}/fuzz-bin:/usr/bin:/bin" \
    HOME="${SCRATCH}/fuzz-home" CARGO_HOME="${SCRATCH}/fuzz-cargo" \
    STUB_LOG="${SCRATCH}/fuzz.log" ASSAY_CARGO_AUDIT_DB="${SCRATCH}/fuzz-db/advisory-db" \
    bash "${RUNNER}" --file "${ROOT}/fuzz/Cargo.lock"
) || fuzz_rc=$?
[[ "${fuzz_rc}" -eq 0 ]] || fail "fuzz-cwd exit ${fuzz_rc}"
grep -qF "cwd=$(cd "${fuzz_cwd}" && pwd -P)" "${SCRATCH}/fuzz.log" || fail "cwd not preserved"
grep -qF -- "--file ${ROOT}/fuzz/Cargo.lock" "${SCRATCH}/fuzz.log" || fail "--file not passed"
assert_sentinel_intact "${SCRATCH}/fuzz-home/.cargo/advisory-db"
ok "fuzz cwd/--file preserved"

deps_security_job() {
  awk '
    /^  deps-security:[[:space:]]*$/ { in_job=1; print; next }
    in_job && /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { exit }
    in_job { print }
  ' "$1"
}

deny_db_path_of() {
  awk '
    /^\[advisories\]/ { in_adv=1; next }
    in_adv && /^\[/ { exit }
    in_adv && /^db-path[[:space:]]*=/ {
      sub(/^db-path[[:space:]]*=[[:space:]]*/, "")
      gsub(/^"/, ""); gsub(/"$/, "")
      print; exit
    }
  ' "$1"
}

# One contract over deny/workflow/precommit paths. Classes:
# deny-db-path | workflow-rm | workflow-runner | workflow-bare-audit | precommit-entry
check_isolated_advisory_cache_contract() {
  local deny_toml="$1" workflow="$2" precommit="$3"
  local job active_job root_hits hook_entry deny_db_path

  deny_db_path="$(deny_db_path_of "${deny_toml}")"
  if [[ "${deny_db_path}" != '$CARGO_HOME/advisory-dbs' ]]; then
    echo "FAIL [deny-db-path]: want \$CARGO_HOME/advisory-dbs; got '${deny_db_path:-<missing>}'" >&2
    return 1
  fi

  job="$(deps_security_job "${workflow}")"
  [[ -n "${job}" ]] || { echo "FAIL [workflow-runner]: no deps-security job" >&2; return 1; }
  active_job="$(printf '%s\n' "${job}" | grep -v '^[[:space:]]*#' || true)"
  if printf '%s\n' "${active_job}" | grep -qE 'rm[[:space:]]+-rf[[:space:]]+.*advisory-db'; then
    echo "FAIL [workflow-rm]: active rm -rf …advisory-db" >&2
    return 1
  fi
  root_hits="$(printf '%s\n' "${active_job}" | grep -cF "${RUNNER_REL}" || true)"
  if [[ "${root_hits}" -lt 2 ]]; then
    echo "FAIL [workflow-runner]: need ${RUNNER_REL} twice; hits=${root_hits}" >&2
    return 1
  fi
  if printf '%s\n' "${active_job}" | grep -E '^[[:space:]]*cargo-audit[[:space:]]+audit([[:space:]]|$)'; then
    echo "FAIL [workflow-bare-audit]: bare cargo-audit audit line" >&2
    return 1
  fi
  printf '%s\n' "${active_job}" | grep -qF 'cd "${RUNNER_TEMP}"' \
    || { echo "FAIL [workflow-runner]: fuzz must cd RUNNER_TEMP" >&2; return 1; }
  printf '%s\n' "${active_job}" | grep -qF -- '--file "${GITHUB_WORKSPACE}/fuzz/Cargo.lock"' \
    || { echo "FAIL [workflow-runner]: fuzz must pass --file" >&2; return 1; }

  hook_entry="$(awk '
    /^[[:space:]]*- id: cargo-audit[[:space:]]*$/ { in_hook=1; next }
    in_hook && /^[[:space:]]*- id:/ { exit }
    in_hook && /^[[:space:]]*entry:/ { print; exit }
  ' "${precommit}")"
  if [[ "${hook_entry}" != *"${RUNNER_REL}"* ]]; then
    echo "FAIL [precommit-entry]: want ${RUNNER_REL}; got:
${hook_entry:-<missing>}" >&2
    return 1
  fi
  if grep -qE "entry:[[:space:]]*bash -lc 'cargo-audit audit'" "${precommit}"; then
    echo "FAIL [precommit-entry]: bare bash -lc 'cargo-audit audit'" >&2
    return 1
  fi
}

expect_contract_red() {
  local class="$1" deny="$2" wf="$3" pc="$4" out rc=0
  out="$(check_isolated_advisory_cache_contract "${deny}" "${wf}" "${pc}" 2>&1)" && rc=$? || rc=$?
  [[ "${rc}" -ne 0 ]] || fail "expected RED [${class}] but GREEN"
  printf '%s\n' "${out}" | grep -qF "[${class}]" \
    || fail "expected [${class}]; got:
${out}"
}

check_isolated_advisory_cache_contract "${DENY_TOML}" "${WORKFLOW}" "${PRECOMMIT}" \
  || fail "live wiring contract RED"
ok "live wiring contract GREEN"

if awk '
  /^\[advisories\]/ { in_adv=1; next }
  in_adv && /^\[/ { in_adv=0 }
  in_adv && /RUSTSEC-2023-0071/ { found=1 }
  END { exit found ? 0 : 1 }
' "${DENY_TOML}" "${ROOT}/.cargo/audit.toml"; then
  fail "RUSTSEC-2023-0071 reintroduced"
fi
ok "RUSTSEC-2023-0071 ignore absent"

python3 - "${PRECOMMIT}" <<'PY'
import re, sys
from pathlib import Path
pc, in_hook, files_line = Path(sys.argv[1]).read_text(encoding="utf-8"), False, None
for line in pc.splitlines():
    if re.search(r"id:\s*cargo-audit-isolated-db-self-test\s*$", line):
        in_hook = True; continue
    if in_hook and re.match(r"^\s+- id:\s*", line):
        break
    m = re.match(r"^\s+files:\s*(.+)\s*$", line) if in_hook else None
    if m:
        files_line = m.group(1).strip(); break
if not files_line:
    raise SystemExit("FAIL: files: line missing")
cre = re.compile(files_line)
need = ("scripts/ci/cargo-audit-with-isolated-db.sh",
        "scripts/ci/test-cargo-audit-with-isolated-db.sh", "deny.toml",
        ".github/workflows/ci.yml", ".pre-commit-config.yaml")
missed = [p for p in need if cre.fullmatch(p) is None]
if missed:
    raise SystemExit(f"FAIL: files: fullmatch missed {missed}; pattern={files_line!r}")
if cre.fullmatch("not/.pre-commit-config.yaml") or cre.fullmatch("x/.github/workflows/ci.yml"):
    raise SystemExit("FAIL: files: fullmatched non-root path")
print("ok   files: regex fullmatches wiring paths")
PY

# Mutations: same assertions must RED with the right class.
mut_runner="${SCRATCH}/mut-runner-no-db.sh"
sed 's/exec cargo-audit audit --db "${db}"/exec cargo-audit audit/' "${RUNNER}" >"${mut_runner}"
chmod +x "${mut_runner}"
mut_out="$(assert_runner_exec_uses_db "${mut_runner}" 2>&1)" && mut_rc=0 || mut_rc=$?
[[ "${mut_rc}" -ne 0 ]] || fail "--db mutant stayed GREEN under assert_runner_exec_uses_db"
printf '%s\n' "${mut_out}" | grep -qF '[runner-db]' || fail "want [runner-db]; got:
${mut_out}"
mut_bin="${SCRATCH}/mut-nodb/bin"; mut_log="${SCRATCH}/mut-nodb/stub.log"
mkdir -p "${SCRATCH}/mut-nodb/home" "${SCRATCH}/mut-nodb/cargo-home" "${mut_bin}"
: >"${mut_log}"
plant_hostile_deny_layout "${SCRATCH}/mut-nodb/home/.cargo/advisory-db"
make_stub_cargo_audit "${mut_bin}" 0
env -i PATH="${mut_bin}:/usr/bin:/bin" HOME="${SCRATCH}/mut-nodb/home" \
  CARGO_HOME="${SCRATCH}/mut-nodb/cargo-home" STUB_LOG="${mut_log}" \
  bash "${mut_runner}" >"${SCRATCH}/mut-nodb/out.txt" 2>&1 || true
grep -q 'argv: audit --db ' "${mut_log}" && fail "mutant still passed --db"
grep -q '^argv: audit' "${mut_log}" || fail "mutant never reached cargo-audit"
assert_sentinel_intact "${SCRATCH}/mut-nodb/home/.cargo/advisory-db"
ok "mutation: --db strip fails [runner-db] + behavioural stub"

mut_deny="${SCRATCH}/mut-deny.toml"
sed 's|db-path = "\$CARGO_HOME/advisory-dbs"|db-path = "~/.cargo/advisory-db"|' "${DENY_TOML}" >"${mut_deny}"
expect_contract_red deny-db-path "${mut_deny}" "${WORKFLOW}" "${PRECOMMIT}"
ok "mutation: singular deny fails [deny-db-path]"

mut_wf="${SCRATCH}/mut-ci.yml"
cp "${WORKFLOW}" "${mut_wf}"
python3 -c '
from pathlib import Path
import sys
p = Path(sys.argv[1]); t = p.read_text(); n = "bash scripts/ci/cargo-audit-with-isolated-db.sh"
assert n in t, "runner invocation missing"
p.write_text(t.replace(n, "rm -rf ~/.cargo/advisory-db\n          cargo-audit audit", 1))
' "${mut_wf}"
expect_contract_red workflow-rm "${DENY_TOML}" "${mut_wf}" "${PRECOMMIT}"
ok "mutation: rm -rf fails [workflow-rm]"

mut_pc="${SCRATCH}/mut-pre-commit.yaml"
sed "s|entry: bash ${RUNNER_REL}|entry: bash -lc 'cargo-audit audit'|" "${PRECOMMIT}" >"${mut_pc}"
expect_contract_red precommit-entry "${DENY_TOML}" "${WORKFLOW}" "${mut_pc}"
ok "mutation: bare pre-commit audit fails [precommit-entry]"
echo "ALL cargo-audit isolated-db self-tests passed"
