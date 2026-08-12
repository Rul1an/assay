#!/usr/bin/env bash
# Contract for split-wave cargo plugin pins (CI-4D2 / #2224).
#
# Before this gate, split-wave0-gates.yml installed cargo-nextest, cargo-hack, and
# cargo-semver-checks with `cargo install --locked <crate>` and no shared pin. `--locked`
# pins each tool's own dependencies, not the tool version, so a new upstream release can
# redden an unrelated PR. The optional API-drift helper had a second unpinned install route
# for cargo-semver-checks. Pins and install-root assertions must read one checked-in value
# from scripts/ci/cargo-plugin-versions.sh (no second installer abstraction).
#
# Discipline matches CI-4D1: dedicated install-step exact argv and exact install counts.
# No shell/package parser.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSIONS="${ROOT}/scripts/ci/cargo-plugin-versions.sh"
WORKFLOW="${ROOT}/.github/workflows/split-wave0-gates.yml"
OPTIONAL="${ROOT}/scripts/ci/optional-public-api-drift.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

ok() { echo "ok   $*"; }

abort_is_failure() {
  local rc="$1"
  [[ "${rc}" -eq 0 ]] || echo "split-wave plugin-versions contract aborted (exit ${rc}); treat as failure" >&2
}
trap 'abort_is_failure "$?"' ERR

# macOS ships /bin/bash 3.2.57. The @Q parameter transformation is bash 4.4+ (#2250).
if awk '
  /^[[:space:]]*#/ { next }
  /\$\{[^}]+@Q\}/ { found=1; print NR ":" $0 }
  END { exit found ? 0 : 1 }
' "${BASH_SOURCE[0]}"; then
  echo "FAIL self-test uses bash-4.4 @Q quoting; macOS bash 3.2 aborts with bad substitution" >&2
  exit 1
fi

[[ -f "${WORKFLOW}" ]] || fail "missing ${WORKFLOW#"${ROOT}"/}"
[[ -f "${OPTIONAL}" ]] || fail "missing ${OPTIONAL#"${ROOT}"/}"
[[ -f "${VERSIONS}" ]] || fail "missing shared version source ${VERSIONS#"${ROOT}"/}"

grep -q 'BASH_SOURCE\[0\]' "${VERSIONS}" \
  || fail "cargo-plugin-versions.sh is not source-safe (missing BASH_SOURCE execute guard)"
grep -qE '^(export[[:space:]]+)?CARGO_NEXTEST_VERSION=' "${VERSIONS}" \
  || fail "cargo-plugin-versions.sh must define CARGO_NEXTEST_VERSION"
grep -qE '^(export[[:space:]]+)?CARGO_HACK_VERSION=' "${VERSIONS}" \
  || fail "cargo-plugin-versions.sh must define CARGO_HACK_VERSION"
grep -qE '^(export[[:space:]]+)?CARGO_SEMVER_CHECKS_VERSION=' "${VERSIONS}" \
  || fail "cargo-plugin-versions.sh must define CARGO_SEMVER_CHECKS_VERSION"
grep -qE '^cargo_plugin_bin_path[[:space:]]*\(\)' "${VERSIONS}" \
  || fail "cargo-plugin-versions.sh must define cargo_plugin_bin_path()"
grep -qE '^assert_cargo_plugin_version[[:space:]]*\(\)' "${VERSIONS}" \
  || fail "cargo-plugin-versions.sh must define assert_cargo_plugin_version()"

# shellcheck source=scripts/ci/cargo-plugin-versions.sh
source "${VERSIONS}"

NEXTEST_PIN="${CARGO_NEXTEST_VERSION:-}"
HACK_PIN="${CARGO_HACK_VERSION:-}"
SEMVER_PIN="${CARGO_SEMVER_CHECKS_VERSION:-}"
AUDIT_PIN="${CARGO_AUDIT_VERSION:-}"
[[ -n "${NEXTEST_PIN}" ]] || fail "CARGO_NEXTEST_VERSION is empty after sourcing ${VERSIONS#"${ROOT}"/}"
[[ -n "${HACK_PIN}" ]] || fail "CARGO_HACK_VERSION is empty after sourcing ${VERSIONS#"${ROOT}"/}"
[[ -n "${SEMVER_PIN}" ]] || fail "CARGO_SEMVER_CHECKS_VERSION is empty after sourcing ${VERSIONS#"${ROOT}"/}"
[[ -n "${AUDIT_PIN}" ]] || fail "CARGO_AUDIT_VERSION is empty after sourcing ${VERSIONS#"${ROOT}"/}"

# --- Workflow extractors (no YAML/shell parser beyond dedicated step bodies) ------------

feature_matrix_job() {
  local wf="$1"
  awk '
    /^  feature-matrix:[[:space:]]*$/ { in_job=1; print; next }
    in_job && /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { exit }
    in_job { print }
  ' "${wf}"
}

semver_public_job() {
  local wf="$1"
  awk '
    /^  semver-public:[[:space:]]*$/ { in_job=1; print; next }
    in_job && /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { exit }
    in_job { print }
  ' "${wf}"
}

# Active (non-comment, non-blank) lines of a named install step's run block.
# Whole-line `# …` ghosts are dropped. Trailing `# …` on an active line is still emitted;
# checks require complete end-anchored command lines so those tails cannot satisfy
# source / --version / assert. No shell comment parser.
install_step_run() {
  local wf="$1" step_name="$2"
  local job_fn="$3"
  "${job_fn}" "${wf}" | awk -v step="${step_name}" '
    function emit_active(line,    tmp) {
      tmp = line
      sub(/^[[:space:]]+/, "", tmp)
      if (tmp == "" || tmp ~ /^#/) return
      print line
    }
    $0 ~ ("^      - name: " step "[[:space:]]*$") { in_step=1; next }
    in_step && /^      - name:/ { exit }
    in_step && /^        run:[[:space:]]*\|[-+]?[[:space:]]*$/ { in_run=1; next }
    in_step && /^        run:[[:space:]]+/ {
      sub(/^        run:[[:space:]]*/, "")
      emit_active($0)
      exit
    }
    in_run && /^        [^[:space:]]/ { exit }
    in_run { emit_active($0) }
  '
}

active_source_line() {
  grep -qE '^[[:space:]]*source[[:space:]]+(\./scripts/ci/cargo-plugin-versions\.sh|"(\./)?scripts/ci/cargo-plugin-versions\.sh")[[:space:]]*$' <<<"$1"
}

# Count any active line containing `cargo` + optional one `+toolchain` token + `install`.
# No package/flag/shell parser — a second cargo install of any shape fails.
count_active_cargo_installs() {
  printf '%s\n' "$1" | grep -cE 'cargo[[:space:]]+(\+[^[:space:]]+[[:space:]]+)?install' || true
}

active_pinned_nextest_install() {
  grep -qE '^[[:space:]]*cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+--version[[:space:]]+"\$\{CARGO_NEXTEST_VERSION\}"[[:space:]]+cargo-nextest[[:space:]]*$' <<<"$1"
}

active_pinned_hack_install() {
  grep -qE '^[[:space:]]*cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+--version[[:space:]]+"\$\{CARGO_HACK_VERSION\}"[[:space:]]+cargo-hack[[:space:]]*$' <<<"$1"
}

active_pinned_semver_install() {
  grep -qE '^[[:space:]]*cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+--version[[:space:]]+"\$\{CARGO_SEMVER_CHECKS_VERSION\}"[[:space:]]+cargo-semver-checks[[:space:]]*$' <<<"$1"
}

active_literal_nextest_install() {
  grep -qE '^[[:space:]]*cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+--version[[:space:]]+"?'"${NEXTEST_PIN}"'"?[[:space:]]+cargo-nextest[[:space:]]*$' <<<"$1"
}

active_literal_hack_install() {
  grep -qE '^[[:space:]]*cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+--version[[:space:]]+"?'"${HACK_PIN}"'"?[[:space:]]+cargo-hack[[:space:]]*$' <<<"$1"
}

active_literal_semver_install() {
  grep -qE '^[[:space:]]*cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+--version[[:space:]]+"?'"${SEMVER_PIN}"'"?[[:space:]]+cargo-semver-checks[[:space:]]*$' <<<"$1"
}

active_assert_nextest() {
  grep -qE '^[[:space:]]*assert_cargo_plugin_version[[:space:]]+cargo-nextest[[:space:]]+"\$\{CARGO_NEXTEST_VERSION\}"[[:space:]]*$' <<<"$1"
}

active_assert_hack() {
  grep -qE '^[[:space:]]*assert_cargo_plugin_version[[:space:]]+cargo-hack[[:space:]]+"\$\{CARGO_HACK_VERSION\}"[[:space:]]*$' <<<"$1"
}

active_assert_semver() {
  grep -qE '^[[:space:]]*assert_cargo_plugin_version[[:space:]]+cargo-semver-checks[[:space:]]+"\$\{CARGO_SEMVER_CHECKS_VERSION\}"[[:space:]]*$' <<<"$1"
}

# Install step (or its job) must state RUSTUP_TOOLCHAIN=stable, or the job must call
# setup-rust before the install (selected toolchain is installed, not inferred from the image).
install_step_states_or_inherits_toolchain() {
  local wf="$1" step_name="$2" job_fn="$3"
  local job step_block
  job="$("${job_fn}" "${wf}")"
  # Step-scoped env: RUSTUP_TOOLCHAIN under the install step.
  step_block="$(awk -v step="${step_name}" '
    $0 ~ ("^      - name: " step "[[:space:]]*$") { in_step=1; print; next }
    in_step && /^      - name:/ { exit }
    in_step { print }
  ' <<<"${job}")"
  if grep -qE '^[[:space:]]*RUSTUP_TOOLCHAIN:[[:space:]]*stable[[:space:]]*$' <<<"${step_block}"; then
    return 0
  fi
  # Job-scoped env.
  if grep -qE '^[[:space:]]*RUSTUP_TOOLCHAIN:[[:space:]]*stable[[:space:]]*$' <<<"${job}"; then
    return 0
  fi
  # setup-rust before this install step in the same job (installs the selected toolchain).
  awk -v step="${step_name}" '
    /uses:[[:space:]]*\.\/\.github\/actions\/setup-rust/ { seen_setup=1 }
    $0 ~ ("^      - name: " step "[[:space:]]*$") {
      exit seen_setup ? 0 : 1
    }
  ' <<<"${job}"
}

check_feature_matrix_install() {
  local wf="$1"
  local install_run install_count

  install_run="$(install_step_run "${wf}" "Install cargo-nextest and cargo-hack" feature_matrix_job)"
  [[ -n "${install_run}" ]] || fail "could not find Install cargo-nextest and cargo-hack run block in ${wf##*/}"

  active_source_line "${install_run}" \
    || fail "feature-matrix install must have an active complete line sourcing scripts/ci/cargo-plugin-versions.sh; got:
${install_run}"

  install_count="$(count_active_cargo_installs "${install_run}")"
  [[ "${install_count}" -eq 2 ]] \
    || fail "feature-matrix install must have exactly two active lines containing cargo install; found ${install_count}:
${install_run}"

  active_pinned_nextest_install "${install_run}" \
    || fail "feature-matrix install must have active complete: cargo install --locked --version \"\${CARGO_NEXTEST_VERSION}\" cargo-nextest; got:
${install_run}"
  active_pinned_hack_install "${install_run}" \
    || fail "feature-matrix install must have active complete: cargo install --locked --version \"\${CARGO_HACK_VERSION}\" cargo-hack; got:
${install_run}"

  if active_literal_nextest_install "${install_run}"; then
    fail "feature-matrix restates nextest version literal ${NEXTEST_PIN} on an active complete install line"
  fi
  if active_literal_hack_install "${install_run}"; then
    fail "feature-matrix restates hack version literal ${HACK_PIN} on an active complete install line"
  fi

  active_assert_nextest "${install_run}" \
    || fail "feature-matrix install must assert cargo-nextest; got:
${install_run}"
  active_assert_hack "${install_run}" \
    || fail "feature-matrix install must assert cargo-hack; got:
${install_run}"

  install_step_states_or_inherits_toolchain "${wf}" "Install cargo-nextest and cargo-hack" feature_matrix_job \
    || fail "feature-matrix install must set RUSTUP_TOOLCHAIN: stable (step/job) or call setup-rust before the install"

  # Preserve feature-matrix invocations with per-command toolchain binding.
  local job
  job="$(feature_matrix_job "${wf}")"
  grep -qE 'RUSTUP_TOOLCHAIN=stable[[:space:]]+cargo[[:space:]]+nextest[[:space:]]+run[[:space:]]+-p[[:space:]]+assay-core' <<<"${job}" \
    || fail "feature-matrix lost bound cargo nextest run -p assay-core"
  grep -qE 'RUSTUP_TOOLCHAIN=stable[[:space:]]+cargo[[:space:]]+hack[[:space:]]+check[[:space:]]+-p[[:space:]]+assay-core[[:space:]]+--each-feature' <<<"${job}" \
    || fail "feature-matrix lost bound cargo hack check -p assay-core --each-feature"
  check_d2_plugin_invocation_bindings "${job}" "feature-matrix"

  ok "feature-matrix nextest/hack pin contract holds for ${wf##*/}"
}

check_semver_public_install() {
  local wf="$1"
  local install_run install_count job

  install_run="$(install_step_run "${wf}" "Install cargo-semver-checks" semver_public_job)"
  [[ -n "${install_run}" ]] || fail "could not find Install cargo-semver-checks run block in ${wf##*/}"

  active_source_line "${install_run}" \
    || fail "semver-public install must have an active complete line sourcing scripts/ci/cargo-plugin-versions.sh; got:
${install_run}"

  install_count="$(count_active_cargo_installs "${install_run}")"
  [[ "${install_count}" -eq 1 ]] \
    || fail "semver-public install must have exactly one active line containing cargo install; found ${install_count}:
${install_run}"

  active_pinned_semver_install "${install_run}" \
    || fail "semver-public install must have active complete: cargo install --locked --version \"\${CARGO_SEMVER_CHECKS_VERSION}\" cargo-semver-checks; got:
${install_run}"

  if active_literal_semver_install "${install_run}"; then
    fail "semver-public restates semver-checks version literal ${SEMVER_PIN} on an active complete install line"
  fi

  active_assert_semver "${install_run}" \
    || fail "semver-public install must assert cargo-semver-checks; got:
${install_run}"

  install_step_states_or_inherits_toolchain "${wf}" "Install cargo-semver-checks" semver_public_job \
    || fail "semver-public install must set RUSTUP_TOOLCHAIN: stable (step/job) or call setup-rust before the install"

  job="$(semver_public_job "${wf}")"
  # Preserve semver gate logic: baseline from last release tag, self-test, allowlist check-release.
  grep -q 'Resolve semver baseline from the last release tag' <<<"${job}" \
    || fail "semver-public lost baseline resolution step"
  grep -q 'scripts/ci/test-semver-gate.sh' <<<"${job}" \
    || fail "semver-public lost semver gate self-test"
  grep -qE 'RUSTUP_TOOLCHAIN=stable[[:space:]]+cargo[[:space:]]+semver-checks[[:space:]]+check-release' <<<"${job}" \
    || fail "semver-public lost bound cargo semver-checks check-release allowlist invocations"
  check_d2_plugin_invocation_bindings "${job}" "semver-public"

  ok "semver-public cargo-semver-checks pin contract holds for ${wf##*/}"
}

# Every active D2 plugin invocation (`cargo nextest|hack|semver-checks`, not install) must
# bind RUSTUP_TOOLCHAIN=stable on the command itself so root cargo check/test still follow
# rust-toolchain.toml. Summary `echo` lines are ignored.
check_d2_plugin_invocation_bindings() {
  local body="$1" label="$2"
  local line tmp
  while IFS= read -r line; do
    tmp="${line#"${line%%[![:space:]]*}"}"
    [[ -n "${tmp}" ]] || continue
    [[ "${tmp}" != \#* ]] || continue
    [[ "${tmp}" != echo\ * ]] || continue
    [[ "${tmp}" != echo\$* ]] || continue
    # Only cargo nextest / hack / semver-checks invocations (not cargo install / check / test).
    if ! grep -qE '(^|[[:space:]])cargo[[:space:]]+(nextest|hack|semver-checks)([[:space:]]|$)' <<<"${tmp}"; then
      continue
    fi
    if grep -qE '(^|[[:space:]])cargo[[:space:]]+install([[:space:]]|$)' <<<"${tmp}"; then
      continue
    fi
    grep -qE '^RUSTUP_TOOLCHAIN=stable[[:space:]]+cargo[[:space:]]+(nextest|hack|semver-checks)([[:space:]]|$)' <<<"${tmp}" \
      || fail "${label}: unbound D2 plugin invocation (need RUSTUP_TOOLCHAIN=stable prefix): ${tmp}"
  done <<<"${body}"
}

check_workflow() {
  local wf="$1"
  check_feature_matrix_install "${wf}"
  check_semver_public_install "${wf}"
}

# --- Optional helper: semver-checks pin only; public-api remains D3 --------------------

check_optional_helper() {
  local helper="$1"

  grep -qE 'source[[:space:]].*scripts/ci/cargo-plugin-versions\.sh' "${helper}" \
    || fail "optional-public-api-drift.sh must source scripts/ci/cargo-plugin-versions.sh for semver-checks"

  # Exact pinned install argv for semver-checks with RUSTUP_TOOLCHAIN defaulted (cd to ROOT).
  grep -qE '^[[:space:]]*RUSTUP_TOOLCHAIN="\$\{RUSTUP_TOOLCHAIN:-stable\}"[[:space:]]+cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+--version[[:space:]]+"\$\{CARGO_SEMVER_CHECKS_VERSION\}"[[:space:]]+cargo-semver-checks[[:space:]]*$' "${helper}" \
    || fail "optional-public-api-drift.sh must install cargo-semver-checks with RUSTUP_TOOLCHAIN=\"\${RUSTUP_TOOLCHAIN:-stable}\" cargo install --locked --version \"\${CARGO_SEMVER_CHECKS_VERSION}\" cargo-semver-checks"

  grep -qE '^[[:space:]]*assert_cargo_plugin_version[[:space:]]+cargo-semver-checks[[:space:]]+"\$\{CARGO_SEMVER_CHECKS_VERSION\}"[[:space:]]*$' "${helper}" \
    || fail "optional-public-api-drift.sh must assert cargo-semver-checks at install-root"

  # public-api remains unpinned for D3 — the generic install path must still exist and must
  # not gain a CARGO_PUBLIC_API_VERSION pin in this slice.
  grep -qE 'cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+"\$\{crate\}"' "${helper}" \
    || fail "optional-public-api-drift.sh lost the generic unpinned cargo install --locked \"\${crate}\" path (public-api D3)"
  if grep -qE 'CARGO_PUBLIC_API_VERSION' "${helper}"; then
    fail "optional-public-api-drift.sh must not pin cargo-public-api in CI-4D2 (D3 owns that)"
  fi

  # Preserve semver gate invocation shape with toolchain binding; public-api stays unbound (D3).
  grep -qE 'RUSTUP_TOOLCHAIN="\$\{RUSTUP_TOOLCHAIN:-stable\}"[[:space:]]+cargo[[:space:]]+semver-checks[[:space:]]+check-release' "${helper}" \
    || fail "optional-public-api-drift.sh lost bound cargo semver-checks check-release"
  # Initial/presence and post-install version probes for semver-checks must also bind.
  local semver_version_binds
  semver_version_binds="$(grep -cE 'RUSTUP_TOOLCHAIN="\$\{RUSTUP_TOOLCHAIN:-stable\}"[[:space:]]+cargo[[:space:]]+"\$\{subcommand\}"[[:space:]]+--version' "${helper}" || true)"
  [[ "${semver_version_binds}" -ge 2 ]] \
    || fail "optional-public-api-drift.sh must bind RUSTUP_TOOLCHAIN on semver-checks initial and post-install --version probes (found ${semver_version_binds})"
  grep -qE 'cargo[[:space:]]+public-api' "${helper}" \
    || fail "optional-public-api-drift.sh lost cargo public-api path"
  # public-api invocations must remain unbound in this slice (D3).
  if grep -qE 'RUSTUP_TOOLCHAIN=.*cargo[[:space:]]+public-api' "${helper}"; then
    fail "optional-public-api-drift.sh must not bind RUSTUP_TOOLCHAIN on public-api in CI-4D2"
  fi

  ok "optional-public-api-drift.sh semver-checks pin contract holds"
}

check_workflow "${WORKFLOW}"
check_optional_helper "${OPTIONAL}"

# --- Behavioral: assertion binds install location; nextest banner tolerated ------------

SCRATCH="$(mktemp -d)"
trap 'rm -rf "${SCRATCH}"' EXIT

expect_in_file() {
  local file="$1" needle="$2" what="$3"
  if grep -qF -- "${needle}" "${file}"; then
    ok "${what}"
  else
    echo "FAIL ${what}: '${needle}' not in" >&2
    sed 's/^/      /' "${file}" >&2
    fail "${what}"
  fi
}

echo "== assert_cargo_plugin_version accepts install-root nextest (measured banner) =="
# Measured cargo-nextest --version: first line "cargo-nextest <ver> (<hex> <YYYY-MM-DD>)"
# plus further banner lines. Only that tool may normalize that first-line suffix.
pass_dir="${SCRATCH}/assert_pass"
mkdir -p "${pass_dir}/bin" "${pass_dir}/early"
cat >"${pass_dir}/bin/cargo-nextest" <<STUB
#!/usr/bin/env bash
echo "cargo-nextest ${NEXTEST_PIN} (deadbeef01 2026-01-01)"
echo "release: ${NEXTEST_PIN}"
echo "host: test"
STUB
chmod +x "${pass_dir}/bin/cargo-nextest"
cat >"${pass_dir}/early/cargo-nextest" <<'STUB'
#!/usr/bin/env bash
echo "cargo-nextest 9.9.9-path-decoy"
STUB
chmod +x "${pass_dir}/early/cargo-nextest"
pass_out="${pass_dir}/out"
pass_exit=0
PATH="${pass_dir}/early:${PATH}" CARGO_HOME="${pass_dir}" \
  assert_cargo_plugin_version cargo-nextest "${NEXTEST_PIN}" >"${pass_out}" 2>&1 || pass_exit=$?
[[ "${pass_exit}" -eq 0 ]] || fail "assert_cargo_plugin_version failed against install-root nextest pin (exit ${pass_exit})"
expect_in_file "${pass_out}" "at ${pass_dir}/bin/cargo-nextest" "assertion names the install-root binary"

echo "== nextest arbitrary parenthetical suffix stays RED =="
nxt_evil_dir="${SCRATCH}/nextest_evil_paren"
mkdir -p "${nxt_evil_dir}/bin"
cat >"${nxt_evil_dir}/bin/cargo-nextest" <<STUB
#!/usr/bin/env bash
echo "cargo-nextest ${NEXTEST_PIN} (not-a-nextest-suffix)"
STUB
chmod +x "${nxt_evil_dir}/bin/cargo-nextest"
nxt_evil_out="${nxt_evil_dir}/out"
nxt_evil_exit=0
CARGO_HOME="${nxt_evil_dir}" \
  assert_cargo_plugin_version cargo-nextest "${NEXTEST_PIN}" >"${nxt_evil_out}" 2>&1 || nxt_evil_exit=$?
[[ "${nxt_evil_exit}" -ne 0 ]] || fail "nextest accepted a non-banner parenthetical suffix"

echo "== cargo-audit parenthetical suffix must stay RED (no global strip) =="
audit_dir="${SCRATCH}/audit_paren"
mkdir -p "${audit_dir}/bin"
cat >"${audit_dir}/bin/cargo-audit" <<STUB
#!/usr/bin/env bash
echo "cargo-audit ${AUDIT_PIN} (evil-suffix)"
STUB
chmod +x "${audit_dir}/bin/cargo-audit"
audit_out="${audit_dir}/out"
audit_exit=0
CARGO_HOME="${audit_dir}" \
  assert_cargo_plugin_version cargo-audit "${AUDIT_PIN}" >"${audit_out}" 2>&1 || audit_exit=$?
[[ "${audit_exit}" -ne 0 ]] || fail "cargo-audit parenthetical suffix was accepted (global strip weakens CI-4D1)"

echo "== cargo-hack parenthetical suffix must stay RED (no global strip) =="
hack_paren_dir="${SCRATCH}/hack_paren"
mkdir -p "${hack_paren_dir}/bin"
cat >"${hack_paren_dir}/bin/cargo-hack" <<STUB
#!/usr/bin/env bash
echo "cargo-hack ${HACK_PIN} (evil-suffix)"
STUB
chmod +x "${hack_paren_dir}/bin/cargo-hack"
hack_paren_out="${hack_paren_dir}/out"
hack_paren_exit=0
CARGO_HOME="${hack_paren_dir}" \
  assert_cargo_plugin_version cargo-hack "${HACK_PIN}" >"${hack_paren_out}" 2>&1 || hack_paren_exit=$?
[[ "${hack_paren_exit}" -ne 0 ]] || fail "cargo-hack parenthetical suffix was accepted (global strip weakens exactness)"

echo "== wrong installed hack version fails =="
wrong_dir="${SCRATCH}/assert_wrong"
mkdir -p "${wrong_dir}/bin"
cat >"${wrong_dir}/bin/cargo-hack" <<'STUB'
#!/usr/bin/env bash
echo "cargo-hack 0.0.0-not-the-pin"
STUB
chmod +x "${wrong_dir}/bin/cargo-hack"
wrong_out="${wrong_dir}/out"
wrong_exit=0
CARGO_HOME="${wrong_dir}" \
  assert_cargo_plugin_version cargo-hack "${HACK_PIN}" >"${wrong_out}" 2>&1 || wrong_exit=$?
[[ "${wrong_exit}" -ne 0 ]] || fail "wrong installed version left assertion green"
expect_in_file "${wrong_out}" "is not the pinned ${HACK_PIN}" "mismatch names the pin"
expect_in_file "${wrong_out}" "scripts/ci/cargo-plugin-versions.sh" "mismatch names the pin's location"

echo "== PATH decoy cannot satisfy semver-checks pin =="
decoy_dir="${SCRATCH}/path_decoy"
mkdir -p "${decoy_dir}/early" "${decoy_dir}/cargo-home"
cat >"${decoy_dir}/early/cargo-semver-checks" <<STUB
#!/usr/bin/env bash
echo "cargo-semver-checks ${SEMVER_PIN}"
STUB
chmod +x "${decoy_dir}/early/cargo-semver-checks"
decoy_out="${decoy_dir}/out"
decoy_exit=0
PATH="${decoy_dir}/early:${PATH}" CARGO_HOME="${decoy_dir}/cargo-home" \
  assert_cargo_plugin_version cargo-semver-checks "${SEMVER_PIN}" >"${decoy_out}" 2>&1 || decoy_exit=$?
[[ "${decoy_exit}" -ne 0 ]] || fail "PATH decoy satisfied assert_cargo_plugin_version"
expect_in_file "${decoy_out}" "cargo-semver-checks missing at install location" \
  "path decoy names the missing install-root binary"

# --- Mutations must bite ---------------------------------------------------------------

SANDBOX_ROOT="$(mktemp -d)"
trap 'rm -rf "${SCRATCH}" "${SANDBOX_ROOT}"' EXIT

echo "== mutation: remove --version from nextest install =="
mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${mutant}" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
new, n = re.subn(
    r'(cargo install --locked) --version "\$\{CARGO_NEXTEST_VERSION\}" (cargo-nextest)',
    r"\1 \2",
    text,
    count=1,
)
if n != 1:
    raise SystemExit(f"could not strip nextest --version (n={n})")
dst.write_text(new)
PY
if ( check_workflow "${mutant}" ) >/dev/null 2>&1; then
  fail "removing nextest --version left the contract green"
fi
ok "removing nextest --version turns the contract red"

echo "== mutation: duplicate workflow literal for hack =="
literal="$(mktemp "${SANDBOX_ROOT}/lit.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${literal}" "${HACK_PIN}" <<'PY'
import pathlib, sys
src, dst, pin = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
text = src.read_text()
old = 'cargo install --locked --version "${CARGO_HACK_VERSION}" cargo-hack'
new = f'cargo install --locked --version "{pin}" cargo-hack'
if old not in text:
    raise SystemExit("could not find versioned hack install line to literalize")
dst.write_text(text.replace(old, new, 1))
PY
if ( check_workflow "${literal}" ) >/dev/null 2>&1; then
  fail "restating the hack version literal in the workflow left the contract green"
fi
ok "duplicate hack workflow literal turns the contract red"

echo "== mutation: remove workflow source from feature-matrix install =="
nosource="$(mktemp "${SANDBOX_ROOT}/nosource.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${nosource}" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
# Remove only the first source block (feature-matrix nextest/hack install).
new, n = re.subn(
    r'^[ \t]*# shellcheck source=scripts/ci/cargo-plugin-versions\.sh[ \t]*\n'
    r'^[ \t]*source[ \t]+\./scripts/ci/cargo-plugin-versions\.sh[ \t]*\n',
    "",
    text,
    count=1,
    flags=re.M,
)
if n != 1:
    raise SystemExit(f"could not remove feature-matrix source block (n={n})")
dst.write_text(new)
PY
if ( check_workflow "${nosource}" ) >/dev/null 2>&1; then
  fail "removing feature-matrix workflow source left the contract green"
fi
ok "removing feature-matrix workflow source turns the contract red"

echo "== mutation: remove semver-checks version assertion =="
noassert="$(mktemp "${SANDBOX_ROOT}/noassert.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${noassert}" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
new, n = re.subn(
    r'^[ \t]*assert_cargo_plugin_version[ \t]+cargo-semver-checks[ \t]+"\$\{CARGO_SEMVER_CHECKS_VERSION\}"[ \t]*\n',
    "",
    text,
    count=1,
    flags=re.M,
)
if n != 1:
    raise SystemExit(f"could not remove semver assert line (n={n})")
dst.write_text(new)
PY
if ( check_workflow "${noassert}" ) >/dev/null 2>&1; then
  fail "removing semver-checks version assertion left the contract green"
fi
ok "removing semver-checks version assertion turns the contract red"

echo "== mutation: comment ghosts + active unpinned nextest/hack installs =="
ghost="$(mktemp "${SANDBOX_ROOT}/ghost.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${ghost}" <<'PY'
import pathlib, re, sys

src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
pat = re.compile(
    r"(      - name: Install cargo-nextest and cargo-hack\n"
    r"(?:        (?:#.*|shell:.*|env:)\n|          RUSTUP_TOOLCHAIN:.*\n)*"
    r"        run: \|[-+]?\n)"
    r"(?:          .*\n)+"
)
repl = (
    "\\1"
    "          set -euo pipefail\n"
    "          # shellcheck source=scripts/ci/cargo-plugin-versions.sh\n"
    "          # source ./scripts/ci/cargo-plugin-versions.sh\n"
    '          # cargo install --locked --version "${CARGO_NEXTEST_VERSION}" cargo-nextest\n'
    '          # assert_cargo_plugin_version cargo-nextest "${CARGO_NEXTEST_VERSION}"\n'
    '          # cargo install --locked --version "${CARGO_HACK_VERSION}" cargo-hack\n'
    '          # assert_cargo_plugin_version cargo-hack "${CARGO_HACK_VERSION}"\n'
    "          cargo install --locked cargo-nextest\n"
    "          cargo install --locked cargo-hack\n"
)
new, n = pat.subn(repl, text, count=1)
if n != 1:
    raise SystemExit(f"could not rewrite nextest/hack install block for ghost mutation (n={n})")
dst.write_text(new)
PY
if ( check_workflow "${ghost}" ) >/dev/null 2>&1; then
  fail "comment-ghost pin left the feature-matrix contract green"
fi
ok "comment-ghost pin + active unpinned installs turns the contract red"

echo "== mutation: second active cargo install in semver step (any shape) =="
expect_second_semver_cargo_install_red() {
  local name="$1" extra="$2"
  local mutant mutant_run
  mutant="$(mktemp "${SANDBOX_ROOT}/dual-${name}.XXXXXX.yml")"
  python3 - "${WORKFLOW}" "${mutant}" "${extra}" <<'PY'
import pathlib, sys

src, dst, extra = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
text = src.read_text()
old = '          assert_cargo_plugin_version cargo-semver-checks "${CARGO_SEMVER_CHECKS_VERSION}"\n'
new = old + f"          {extra}\n"
if old not in text:
    raise SystemExit("could not find semver assert line to append a second install after")
dst.write_text(text.replace(old, new, 1))
PY
  mutant_run="$(install_step_run "${mutant}" "Install cargo-semver-checks" semver_public_job)"
  [[ "$(count_active_cargo_installs "${mutant_run}")" -eq 2 ]] \
    || fail "${name}: expected two active cargo install lines; got:
${mutant_run}"
  if ( check_workflow "${mutant}" ) >/dev/null 2>&1; then
    fail "${name}: second cargo install left the contract green"
  fi
  ok "second cargo install turns red (${name})"
}

expect_second_semver_cargo_install_red bare 'cargo install --locked cargo-semver-checks'
expect_second_semver_cargo_install_red atversion 'cargo install --locked cargo-semver-checks@0.99.0'
expect_second_semver_cargo_install_red plus_stable 'cargo +stable install --locked cargo-semver-checks'
expect_second_semver_cargo_install_red plus_nightly 'cargo +nightly install --locked cargo-semver-checks'
expect_second_semver_cargo_install_red other_pkg 'cargo install --locked cargo-deny'

echo "== mutation: optional helper drops semver pin =="
opt_mut="$(mktemp "${SANDBOX_ROOT}/opt.XXXXXX.sh")"
python3 - "${OPTIONAL}" "${opt_mut}" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
# Replace pinned semver install with the old floating form.
new, n = re.subn(
    r'RUSTUP_TOOLCHAIN="\$\{RUSTUP_TOOLCHAIN:-stable\}" cargo install --locked --version "\$\{CARGO_SEMVER_CHECKS_VERSION\}" cargo-semver-checks',
    'cargo install --locked cargo-semver-checks',
    text,
    count=1,
)
if n != 1:
    raise SystemExit(f"could not unpin optional semver install (n={n})")
dst.write_text(new)
PY
if ( check_optional_helper "${opt_mut}" ) >/dev/null 2>&1; then
  fail "unpinning optional semver-checks install left the helper contract green"
fi
ok "unpinning optional semver-checks install turns the helper contract red"

echo "== mutation: optional helper drops install-root assert =="
opt_noassert="$(mktemp "${SANDBOX_ROOT}/opt-na.XXXXXX.sh")"
python3 - "${OPTIONAL}" "${opt_noassert}" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
new, n = re.subn(
    r'^[ \t]*assert_cargo_plugin_version[ \t]+cargo-semver-checks[ \t]+"\$\{CARGO_SEMVER_CHECKS_VERSION\}"[ \t]*\n',
    "",
    text,
    count=1,
    flags=re.M,
)
if n != 1:
    raise SystemExit(f"could not remove optional assert (n={n})")
dst.write_text(new)
PY
if ( check_optional_helper "${opt_noassert}" ) >/dev/null 2>&1; then
  fail "removing optional semver assert left the helper contract green"
fi
ok "removing optional semver assert turns the helper contract red"

echo "== regression: feature-matrix install run: |- stays green =="
chomp="$(mktemp "${SANDBOX_ROOT}/chomp.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${chomp}" <<'PY'
import pathlib, re, sys

src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
pat = re.compile(
    r"(      - name: Install cargo-nextest and cargo-hack\n"
    r"(?:        (?:#.*|shell:.*|env:)\n|          RUSTUP_TOOLCHAIN:.*\n)*)"
    r"        run: \|\n"
)
new, n = pat.subn(r"\1        run: |-\n", text, count=1)
if n != 1:
    raise SystemExit(f"could not switch feature-matrix install to run: |- (n={n})")
dst.write_text(new)
PY
if ! ( check_workflow "${chomp}" ) >/dev/null 2>&1; then
  fail "correctly pinned feature-matrix run: |- turned the contract red"
fi
ok "correctly pinned feature-matrix run: |- stays green"

echo "== mutation: remove nextest invocation toolchain binding =="
unbound_nextest="$(mktemp "${SANDBOX_ROOT}/unbound-nextest.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${unbound_nextest}" <<'PY'
import pathlib, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
old = "RUSTUP_TOOLCHAIN=stable cargo nextest run -p assay-core --all-features"
new = "cargo nextest run -p assay-core --all-features"
if old not in text:
    raise SystemExit("could not find bound nextest invocation to unbind")
dst.write_text(text.replace(old, new, 1))
PY
if ( check_workflow "${unbound_nextest}" ) >/dev/null 2>&1; then
  fail "removing nextest invocation RUSTUP_TOOLCHAIN binding left the contract green"
fi
ok "removing nextest invocation binding turns the contract red"

echo "== mutation: remove hack invocation toolchain binding =="
unbound_hack="$(mktemp "${SANDBOX_ROOT}/unbound-hack.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${unbound_hack}" <<'PY'
import pathlib, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
old = "RUSTUP_TOOLCHAIN=stable cargo hack check -p assay-core --each-feature"
new = "cargo hack check -p assay-core --each-feature"
if old not in text:
    raise SystemExit("could not find bound hack invocation to unbind")
dst.write_text(text.replace(old, new, 1))
PY
if ( check_workflow "${unbound_hack}" ) >/dev/null 2>&1; then
  fail "removing hack invocation RUSTUP_TOOLCHAIN binding left the contract green"
fi
ok "removing hack invocation binding turns the contract red"

echo "== mutation: remove semver-checks invocation toolchain binding =="
unbound_semver="$(mktemp "${SANDBOX_ROOT}/unbound-semver.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${unbound_semver}" <<'PY'
import pathlib, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
old = 'RUSTUP_TOOLCHAIN=stable cargo semver-checks check-release -p "${crate}" --baseline-rev "${BASELINE_TAG}"'
new = 'cargo semver-checks check-release -p "${crate}" --baseline-rev "${BASELINE_TAG}"'
if old not in text:
    raise SystemExit("could not find bound semver-checks invocation to unbind")
dst.write_text(text.replace(old, new, 1))
PY
if ( check_workflow "${unbound_semver}" ) >/dev/null 2>&1; then
  fail "removing semver-checks invocation RUSTUP_TOOLCHAIN binding left the contract green"
fi
ok "removing semver-checks invocation binding turns the contract red"

echo "== mutation: optional helper drops semver check-release binding =="
opt_unbound="$(mktemp "${SANDBOX_ROOT}/opt-unbound.XXXXXX.sh")"
python3 - "${OPTIONAL}" "${opt_unbound}" <<'PY'
import pathlib, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
old = 'RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}" cargo semver-checks check-release'
new = "cargo semver-checks check-release"
if old not in text:
    raise SystemExit("could not find bound optional semver check-release to unbind")
dst.write_text(text.replace(old, new, 1))
PY
if ( check_optional_helper "${opt_unbound}" ) >/dev/null 2>&1; then
  fail "removing optional semver check-release binding left the helper contract green"
fi
ok "removing optional semver check-release binding turns the helper contract red"

ok "split-wave plugin-versions contract mutations bite"
echo "PASS: split-wave plugin-versions contract"
