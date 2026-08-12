#!/usr/bin/env bash
# Contract for optional public-api + mutants plugin pins (CI-4D3 / #2224).
#
# Before this gate, optional-public-api-drift.sh installed cargo-public-api with
# `cargo install --locked "${crate}"` (no shared pin, no toolchain bind on probes or
# diffs), and mutation-smoke-pure-modules.sh installed cargo-mutants the same way.
# `--locked` pins each tool's own dependencies, not the tool version. Pins and
# install-root assertions must read one checked-in value from
# scripts/ci/cargo-plugin-versions.sh (no second installer abstraction).
#
# Discipline matches CI-4D1/D2: exact argv, exact install counts, no shell/package
# parser. Opt-in / opt-out skip semantics stay exit 0.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSIONS="${ROOT}/scripts/ci/cargo-plugin-versions.sh"
OPTIONAL="${ROOT}/scripts/ci/optional-public-api-drift.sh"
MUTATION="${ROOT}/scripts/ci/mutation-smoke-pure-modules.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

ok() { echo "ok   $*"; }

abort_is_failure() {
  local rc="$1"
  [[ "${rc}" -eq 0 ]] || echo "optional plugin-versions contract aborted (exit ${rc}); treat as failure" >&2
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

[[ -f "${OPTIONAL}" ]] || fail "missing ${OPTIONAL#"${ROOT}"/}"
[[ -f "${MUTATION}" ]] || fail "missing ${MUTATION#"${ROOT}"/}"
[[ -f "${VERSIONS}" ]] || fail "missing shared version source ${VERSIONS#"${ROOT}"/}"

grep -q 'BASH_SOURCE\[0\]' "${VERSIONS}" \
  || fail "cargo-plugin-versions.sh is not source-safe (missing BASH_SOURCE execute guard)"
grep -qE '^(export[[:space:]]+)?CARGO_PUBLIC_API_VERSION=' "${VERSIONS}" \
  || fail "cargo-plugin-versions.sh must define CARGO_PUBLIC_API_VERSION"
grep -qE '^(export[[:space:]]+)?CARGO_MUTANTS_VERSION=' "${VERSIONS}" \
  || fail "cargo-plugin-versions.sh must define CARGO_MUTANTS_VERSION"
grep -qE '^cargo_plugin_bin_path[[:space:]]*\(\)' "${VERSIONS}" \
  || fail "cargo-plugin-versions.sh must define cargo_plugin_bin_path()"
grep -qE '^assert_cargo_plugin_version[[:space:]]*\(\)' "${VERSIONS}" \
  || fail "cargo-plugin-versions.sh must define assert_cargo_plugin_version()"

# shellcheck source=scripts/ci/cargo-plugin-versions.sh
source "${VERSIONS}"

PUBLIC_API_PIN="${CARGO_PUBLIC_API_VERSION:-}"
MUTANTS_PIN="${CARGO_MUTANTS_VERSION:-}"
SEMVER_PIN="${CARGO_SEMVER_CHECKS_VERSION:-}"
AUDIT_PIN="${CARGO_AUDIT_VERSION:-}"
[[ -n "${PUBLIC_API_PIN}" ]] || fail "CARGO_PUBLIC_API_VERSION is empty after sourcing ${VERSIONS#"${ROOT}"/}"
[[ -n "${MUTANTS_PIN}" ]] || fail "CARGO_MUTANTS_VERSION is empty after sourcing ${VERSIONS#"${ROOT}"/}"
[[ -n "${SEMVER_PIN}" ]] || fail "CARGO_SEMVER_CHECKS_VERSION is empty after sourcing ${VERSIONS#"${ROOT}"/}"
[[ -n "${AUDIT_PIN}" ]] || fail "CARGO_AUDIT_VERSION is empty after sourcing ${VERSIONS#"${ROOT}"/}"

# Measured pins for this slice (crates.io max_stable / unyanked at authoring).
[[ "${PUBLIC_API_PIN}" == "0.52.0" ]] \
  || fail "CARGO_PUBLIC_API_VERSION must be 0.52.0 for CI-4D3 (got ${PUBLIC_API_PIN})"
[[ "${MUTANTS_PIN}" == "27.1.0" ]] \
  || fail "CARGO_MUTANTS_VERSION must be 27.1.0 for CI-4D3 (got ${MUTANTS_PIN})"

# Refuse a restated literal in the shared source that differs from the export name's value
# only by being duplicated as a second assignment — both tools get exactly one export.
public_api_export_count="$(grep -cE '^(export[[:space:]]+)?CARGO_PUBLIC_API_VERSION=' "${VERSIONS}" || true)"
mutants_export_count="$(grep -cE '^(export[[:space:]]+)?CARGO_MUTANTS_VERSION=' "${VERSIONS}" || true)"
[[ "${public_api_export_count}" -eq 1 ]] \
  || fail "CARGO_PUBLIC_API_VERSION must appear exactly once as an assignment (found ${public_api_export_count})"
[[ "${mutants_export_count}" -eq 1 ]] \
  || fail "CARGO_MUTANTS_VERSION must appear exactly once as an assignment (found ${mutants_export_count})"

# --- Extractors: active (non-comment, non-blank) lines; no shell comment parser ------------

active_lines() {
  awk '
    function emit_active(line,    tmp) {
      tmp = line
      sub(/^[[:space:]]+/, "", tmp)
      if (tmp == "" || tmp ~ /^#/) return
      print line
    }
    { emit_active($0) }
  ' "$1"
}

count_active_cargo_installs() {
  printf '%s\n' "$1" | grep -cE 'cargo[[:space:]]+(\+[^[:space:]]+[[:space:]]+)?install' || true
}

active_source_line() {
  # Helpers source "${ROOT}/scripts/ci/cargo-plugin-versions.sh"; workflow steps use ./path.
  grep -qE '^[[:space:]]*source[[:space:]]+("\$\{ROOT\}/|\./|"(\./)?)scripts/ci/cargo-plugin-versions\.sh"[[:space:]]*$' <<<"$1" \
    || grep -qE '^[[:space:]]*source[[:space:]]+\./scripts/ci/cargo-plugin-versions\.sh[[:space:]]*$' <<<"$1"
}

active_pinned_public_api_install() {
  grep -qE '^[[:space:]]*RUSTUP_TOOLCHAIN="\$\{RUSTUP_TOOLCHAIN:-stable\}"[[:space:]]+cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+--version[[:space:]]+"\$\{CARGO_PUBLIC_API_VERSION\}"[[:space:]]+cargo-public-api[[:space:]]*$' <<<"$1"
}

active_pinned_mutants_install() {
  grep -qE '^[[:space:]]*RUSTUP_TOOLCHAIN="\$\{RUSTUP_TOOLCHAIN:-stable\}"[[:space:]]+cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+--version[[:space:]]+"\$\{CARGO_MUTANTS_VERSION\}"[[:space:]]+cargo-mutants[[:space:]]*$' <<<"$1"
}

active_literal_public_api_install() {
  grep -qE '^[[:space:]]*RUSTUP_TOOLCHAIN="\$\{RUSTUP_TOOLCHAIN:-stable\}"[[:space:]]+cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+--version[[:space:]]+"?'"${PUBLIC_API_PIN}"'"?[[:space:]]+cargo-public-api[[:space:]]*$' <<<"$1"
}

active_literal_mutants_install() {
  grep -qE '^[[:space:]]*RUSTUP_TOOLCHAIN="\$\{RUSTUP_TOOLCHAIN:-stable\}"[[:space:]]+cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+--version[[:space:]]+"?'"${MUTANTS_PIN}"'"?[[:space:]]+cargo-mutants[[:space:]]*$' <<<"$1"
}

active_assert_public_api() {
  grep -qE '^[[:space:]]*assert_cargo_plugin_version[[:space:]]+cargo-public-api[[:space:]]+"\$\{CARGO_PUBLIC_API_VERSION\}"[[:space:]]*$' <<<"$1"
}

active_assert_mutants() {
  grep -qE '^[[:space:]]*assert_cargo_plugin_version[[:space:]]+cargo-mutants[[:space:]]+"\$\{CARGO_MUTANTS_VERSION\}"[[:space:]]*$' <<<"$1"
}

# Every active cargo public-api / mutants invocation (not install) must bind
# RUSTUP_TOOLCHAIN immediately before cargo. Allows a leading `if ` for help probes.
check_bound_plugin_invocations() {
  local body="$1" label="$2" plugin="$3"
  local line tmp
  while IFS= read -r line; do
    tmp="${line#"${line%%[![:space:]]*}"}"
    [[ -n "${tmp}" ]] || continue
    [[ "${tmp}" != \#* ]] || continue
    [[ "${tmp}" != echo\ * ]] || continue
    [[ "${tmp}" != echo\$* ]] || continue
    if ! grep -qE '(^|[[:space:]])cargo[[:space:]]+'"${plugin}"'([[:space:]]|$)' <<<"${tmp}"; then
      continue
    fi
    if grep -qE '(^|[[:space:]])cargo[[:space:]]+(\+[^[:space:]]+[[:space:]]+)?install([[:space:]]|$)' <<<"${tmp}"; then
      continue
    fi
    # Bound forms: optional if/elif/if! before RUSTUP_TOOLCHAIN=... cargo <plugin> ...
    if grep -qE '^((if|elif)[[:space:]]+(![[:space:]]+)?)?RUSTUP_TOOLCHAIN=("\$\{RUSTUP_TOOLCHAIN:-stable\}"|stable)[[:space:]]+cargo[[:space:]]+'"${plugin}"'([[:space:]]|$)' <<<"${tmp}"; then
      continue
    fi
    fail "${label}: unbound ${plugin} invocation (need RUSTUP_TOOLCHAIN prefix): ${tmp}"
  done <<<"${body}"
}

# --- optional-public-api-drift.sh --------------------------------------------------------

check_optional_helper() {
  local helper="$1"
  local active install_count public_api_version_binds

  active="$(active_lines "${helper}")"

  active_source_line "${active}" \
    || fail "optional-public-api-drift.sh must source scripts/ci/cargo-plugin-versions.sh; active lines:
${active}"

  # Preserve D2 semver-checks pin + assert + bound check-release.
  grep -qE '^[[:space:]]*RUSTUP_TOOLCHAIN="\$\{RUSTUP_TOOLCHAIN:-stable\}"[[:space:]]+cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+--version[[:space:]]+"\$\{CARGO_SEMVER_CHECKS_VERSION\}"[[:space:]]+cargo-semver-checks[[:space:]]*$' "${helper}" \
    || fail "optional-public-api-drift.sh must keep D2 pinned cargo-semver-checks install"
  grep -qE '^[[:space:]]*assert_cargo_plugin_version[[:space:]]+cargo-semver-checks[[:space:]]+"\$\{CARGO_SEMVER_CHECKS_VERSION\}"[[:space:]]*$' "${helper}" \
    || fail "optional-public-api-drift.sh must keep D2 cargo-semver-checks assert"
  grep -qE 'RUSTUP_TOOLCHAIN="\$\{RUSTUP_TOOLCHAIN:-stable\}"[[:space:]]+cargo[[:space:]]+semver-checks[[:space:]]+check-release' "${helper}" \
    || fail "optional-public-api-drift.sh must keep bound cargo semver-checks check-release"

  # public-api opt-in install: exact pinned argv + install-root assert.
  active_pinned_public_api_install "${active}" \
    || fail "optional-public-api-drift.sh must install cargo-public-api with RUSTUP_TOOLCHAIN=\"\${RUSTUP_TOOLCHAIN:-stable}\" cargo install --locked --version \"\${CARGO_PUBLIC_API_VERSION}\" cargo-public-api; got:
${active}"

  if active_literal_public_api_install "${active}"; then
    fail "optional-public-api-drift.sh restates public-api version literal ${PUBLIC_API_PIN} on an active complete install line"
  fi

  active_assert_public_api "${active}" \
    || fail "optional-public-api-drift.sh must assert cargo-public-api at install-root; got:
${active}"

  # No floating generic install path for public-api, and no cargo +toolchain install forms.
  if grep -qE 'cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+"\$\{crate\}"' "${helper}"; then
    fail "optional-public-api-drift.sh still has generic unpinned cargo install --locked \"\${crate}\" (D3 pins public-api)"
  fi
  if grep -qE 'cargo[[:space:]]+\+[^[:space:]]+[[:space:]]+install' "${helper}"; then
    fail "optional-public-api-drift.sh must not use cargo +toolchain install; bind via RUSTUP_TOOLCHAIN="
  fi

  # Exactly two active cargo install lines in the helper (semver-checks + public-api).
  install_count="$(count_active_cargo_installs "${active}")"
  [[ "${install_count}" -eq 2 ]] \
    || fail "optional-public-api-drift.sh must have exactly two active cargo install lines (semver + public-api); found ${install_count}:
${active}"

  # Every public-api probe/help/diff is bound stable (presence, post-install, help, diff).
  public_api_version_binds="$(grep -cE 'RUSTUP_TOOLCHAIN="\$\{RUSTUP_TOOLCHAIN:-stable\}"[[:space:]]+cargo[[:space:]]+"\$\{subcommand\}"[[:space:]]+--version' "${helper}" || true)"
  # Both crates (semver-checks + public-api) share the helper; require ≥2 binds for the
  # public-api branch alone is enforced below by scanning every public-api invocation.
  [[ "${public_api_version_binds}" -ge 2 ]] \
    || fail "optional-public-api-drift.sh must bind RUSTUP_TOOLCHAIN on subcommand --version probes (found ${public_api_version_binds})"

  check_bound_plugin_invocations "${active}" "optional-public-api-drift.sh" "public-api"

  # Skip semantics: missing tool + INSTALL_TOOLS!=1 still skips (not fail-closed install).
  grep -qE 'skip cargo-public-api: cargo subcommand not installed' "${helper}" \
    || fail "optional-public-api-drift.sh lost missing-tool skip message for public-api"

  ok "optional-public-api-drift.sh public-api pin contract holds"
}

# --- mutation-smoke-pure-modules.sh ------------------------------------------------------

check_mutation_smoke() {
  local helper="$1"
  local active install_count

  active="$(active_lines "${helper}")"

  # Opt-out: ASSAY_RUN_MUTATION_SMOKE != 1 exits 0 before any install.
  grep -qE 'ASSAY_RUN_MUTATION_SMOKE:-0' "${helper}" \
    || fail "mutation-smoke-pure-modules.sh must gate on ASSAY_RUN_MUTATION_SMOKE"
  grep -qE 'skipped: set ASSAY_RUN_MUTATION_SMOKE=1' "${helper}" \
    || fail "mutation-smoke-pure-modules.sh lost opt-out skip message"

  active_source_line "${active}" \
    || fail "mutation-smoke-pure-modules.sh must source scripts/ci/cargo-plugin-versions.sh on the opt-in install path; active lines:
${active}"

  active_pinned_mutants_install "${active}" \
    || fail "mutation-smoke-pure-modules.sh must install with RUSTUP_TOOLCHAIN=\"\${RUSTUP_TOOLCHAIN:-stable}\" cargo install --locked --version \"\${CARGO_MUTANTS_VERSION}\" cargo-mutants; got:
${active}"

  if active_literal_mutants_install "${active}"; then
    fail "mutation-smoke-pure-modules.sh restates mutants version literal ${MUTANTS_PIN} on an active complete install line"
  fi

  active_assert_mutants "${active}" \
    || fail "mutation-smoke-pure-modules.sh must assert cargo-mutants at install-root; got:
${active}"

  install_count="$(count_active_cargo_installs "${active}")"
  [[ "${install_count}" -eq 1 ]] \
    || fail "mutation-smoke-pure-modules.sh must have exactly one active cargo install; found ${install_count}:
${active}"

  if grep -qE 'cargo[[:space:]]+\+[^[:space:]]+[[:space:]]+install' "${helper}"; then
    fail "mutation-smoke-pure-modules.sh must not use cargo +toolchain install; bind via RUSTUP_TOOLCHAIN="
  fi

  # Presence probe and every mutants run bound.
  grep -qE 'RUSTUP_TOOLCHAIN="\$\{RUSTUP_TOOLCHAIN:-stable\}"[[:space:]]+cargo[[:space:]]+mutants[[:space:]]+--version' "${helper}" \
    || fail "mutation-smoke-pure-modules.sh must bind RUSTUP_TOOLCHAIN on cargo mutants --version"
  check_bound_plugin_invocations "${active}" "mutation-smoke-pure-modules.sh" "mutants"

  # Missing tool + install=0 still skips (exit 0 path).
  grep -qE 'ASSAY_INSTALL_MUTATION_TOOLS:-0' "${helper}" \
    || fail "mutation-smoke-pure-modules.sh must honor ASSAY_INSTALL_MUTATION_TOOLS"
  grep -qE 'skipped: cargo-mutants is not installed' "${helper}" \
    || fail "mutation-smoke-pure-modules.sh lost missing-tool skip message"

  # Preserve mutation target arguments (package/file/-- separators).
  grep -qE 'cargo[[:space:]]+mutants[[:space:]]+--package[[:space:]]+assay-evidence[[:space:]]+--file[[:space:]]+crates/assay-evidence/src/trust_basis/diff\.rs' "${helper}" \
    || fail "mutation-smoke lost trust_basis/diff.rs target"
  grep -qE 'cargo[[:space:]]+mutants[[:space:]]+--package[[:space:]]+assay-evidence[[:space:]]+--file[[:space:]]+crates/assay-evidence/src/trust_basis/classifiers\.rs' "${helper}" \
    || fail "mutation-smoke lost trust_basis/classifiers.rs target"
  grep -qE 'cargo[[:space:]]+mutants[[:space:]]+--package[[:space:]]+assay-cli[[:space:]]+--file[[:space:]]+crates/assay-cli/src/cli/commands/sandbox/degradation\.rs' "${helper}" \
    || fail "mutation-smoke lost sandbox/degradation.rs target"
  grep -qE -- '-- trust_basis' "${helper}" \
    || fail "mutation-smoke lost -- trust_basis filter"
  grep -qE -- '-- sandbox' "${helper}" \
    || fail "mutation-smoke lost -- sandbox filter"

  ok "mutation-smoke-pure-modules.sh mutants pin contract holds"
}

check_optional_helper "${OPTIONAL}"
check_mutation_smoke "${MUTATION}"

# --- Behavioral: assertion binds install location; exactness for new tools ---------------

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

echo "== assert_cargo_plugin_version accepts install-root cargo-public-api =="
pass_dir="${SCRATCH}/assert_pass"
mkdir -p "${pass_dir}/bin" "${pass_dir}/early"
cat >"${pass_dir}/bin/cargo-public-api" <<STUB
#!/usr/bin/env bash
echo "cargo-public-api ${PUBLIC_API_PIN}"
STUB
chmod +x "${pass_dir}/bin/cargo-public-api"
cat >"${pass_dir}/early/cargo-public-api" <<'STUB'
#!/usr/bin/env bash
echo "cargo-public-api 9.9.9-path-decoy"
STUB
chmod +x "${pass_dir}/early/cargo-public-api"
pass_out="${pass_dir}/out"
pass_exit=0
PATH="${pass_dir}/early:${PATH}" CARGO_HOME="${pass_dir}" \
  assert_cargo_plugin_version cargo-public-api "${PUBLIC_API_PIN}" >"${pass_out}" 2>&1 || pass_exit=$?
[[ "${pass_exit}" -eq 0 ]] || fail "assert_cargo_plugin_version failed against install-root public-api pin (exit ${pass_exit})"
expect_in_file "${pass_out}" "at ${pass_dir}/bin/cargo-public-api" "assertion names the install-root binary"

echo "== cargo-public-api parenthetical suffix must stay RED (no global strip) =="
api_paren_dir="${SCRATCH}/api_paren"
mkdir -p "${api_paren_dir}/bin"
cat >"${api_paren_dir}/bin/cargo-public-api" <<STUB
#!/usr/bin/env bash
echo "cargo-public-api ${PUBLIC_API_PIN} (evil-suffix)"
STUB
chmod +x "${api_paren_dir}/bin/cargo-public-api"
api_paren_out="${api_paren_dir}/out"
api_paren_exit=0
CARGO_HOME="${api_paren_dir}" \
  assert_cargo_plugin_version cargo-public-api "${PUBLIC_API_PIN}" >"${api_paren_out}" 2>&1 || api_paren_exit=$?
[[ "${api_paren_exit}" -ne 0 ]] || fail "cargo-public-api parenthetical suffix was accepted (weakens exactness)"

echo "== PATH decoy cannot satisfy cargo-mutants pin =="
decoy_dir="${SCRATCH}/path_decoy"
mkdir -p "${decoy_dir}/early" "${decoy_dir}/cargo-home"
cat >"${decoy_dir}/early/cargo-mutants" <<STUB
#!/usr/bin/env bash
echo "cargo-mutants ${MUTANTS_PIN}"
STUB
chmod +x "${decoy_dir}/early/cargo-mutants"
decoy_out="${decoy_dir}/out"
decoy_exit=0
PATH="${decoy_dir}/early:${PATH}" CARGO_HOME="${decoy_dir}/cargo-home" \
  assert_cargo_plugin_version cargo-mutants "${MUTANTS_PIN}" >"${decoy_out}" 2>&1 || decoy_exit=$?
[[ "${decoy_exit}" -ne 0 ]] || fail "PATH decoy satisfied assert_cargo_plugin_version for mutants"
expect_in_file "${decoy_out}" "cargo-mutants missing at install location" \
  "path decoy names the missing install-root binary"

echo "== cargo-mutants receives its Cargo subcommand argv =="
mutants_argv_dir="${SCRATCH}/mutants_argv"
mkdir -p "${mutants_argv_dir}/bin"
cat >"${mutants_argv_dir}/bin/cargo-mutants" <<STUB
#!/usr/bin/env bash
if [[ "\${1:-}" != "mutants" || "\${2:-}" != "--version" || "\${#}" -ne 2 ]]; then
  echo "expected cargo-mutants argv: mutants --version; got: \$*" >&2
  exit 64
fi
echo "cargo-mutants ${MUTANTS_PIN}"
STUB
chmod +x "${mutants_argv_dir}/bin/cargo-mutants"
mutants_argv_out="${mutants_argv_dir}/out"
mutants_argv_exit=0
CARGO_HOME="${mutants_argv_dir}" \
  assert_cargo_plugin_version cargo-mutants "${MUTANTS_PIN}" >"${mutants_argv_out}" 2>&1 || mutants_argv_exit=$?
[[ "${mutants_argv_exit}" -eq 0 ]] \
  || fail "cargo-mutants version assertion did not use Cargo subcommand argv (exit ${mutants_argv_exit}): $(cat "${mutants_argv_out}")"

echo "== wrong installed mutants version fails =="
wrong_dir="${SCRATCH}/assert_wrong"
mkdir -p "${wrong_dir}/bin"
cat >"${wrong_dir}/bin/cargo-mutants" <<'STUB'
#!/usr/bin/env bash
echo "cargo-mutants 0.0.0-not-the-pin"
STUB
chmod +x "${wrong_dir}/bin/cargo-mutants"
wrong_out="${wrong_dir}/out"
wrong_exit=0
CARGO_HOME="${wrong_dir}" \
  assert_cargo_plugin_version cargo-mutants "${MUTANTS_PIN}" >"${wrong_out}" 2>&1 || wrong_exit=$?
[[ "${wrong_exit}" -ne 0 ]] || fail "wrong installed mutants version left assertion green"
expect_in_file "${wrong_out}" "is not the pinned ${MUTANTS_PIN}" "mismatch names the pin"
expect_in_file "${wrong_out}" "scripts/ci/cargo-plugin-versions.sh" "mismatch names the pin's location"

echo "== cargo-audit parenthetical suffix must stay RED (D1 exactness preserved) =="
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

# --- Behavioral skip semantics (opt-out / missing tool) ----------------------------------

echo "== mutation-smoke opt-out skips with exit 0 =="
opt_out_dir="${SCRATCH}/mut_opt_out"
mkdir -p "${opt_out_dir}"
opt_out_exit=0
(
  cd "${ROOT}"
  ASSAY_RUN_MUTATION_SMOKE=0 ASSAY_INSTALL_MUTATION_TOOLS=0 \
    bash "${MUTATION}" >"${opt_out_dir}/out" 2>&1
) || opt_out_exit=$?
[[ "${opt_out_exit}" -eq 0 ]] || fail "mutation-smoke opt-out exited ${opt_out_exit}, expected 0"
expect_in_file "${opt_out_dir}/out" "ASSAY_RUN_MUTATION_SMOKE=1" "opt-out names the enable flag"

echo "== mutation-smoke missing tool + install=0 skips with exit 0 =="
# Force a PATH with no cargo-mutants and a fake cargo that rejects the subcommand.
miss_dir="${SCRATCH}/mut_miss"
mkdir -p "${miss_dir}/bin"
cat >"${miss_dir}/bin/cargo" <<'STUB'
#!/usr/bin/env bash
if [[ "$*" == *mutants* ]]; then
  echo "error: no such command: \`mutants\`" >&2
  exit 101
fi
# Delegate unrelated cargo uses (should not be hit in this skip path).
exit 0
STUB
chmod +x "${miss_dir}/bin/cargo"
miss_exit=0
(
  cd "${ROOT}"
  PATH="${miss_dir}/bin:${PATH}" \
  ASSAY_RUN_MUTATION_SMOKE=1 ASSAY_INSTALL_MUTATION_TOOLS=0 \
    bash "${MUTATION}" >"${miss_dir}/out" 2>&1
) || miss_exit=$?
[[ "${miss_exit}" -eq 0 ]] || fail "mutation-smoke missing-tool install=0 exited ${miss_exit}, expected 0"
expect_in_file "${miss_dir}/out" "cargo-mutants is not installed" "missing-tool skip names cargo-mutants"

echo "== optional-public-api-drift missing tools + install=0 skips with exit 0 =="
# Real sandbox: both optional subcommands absent; install opt-out must exit 0 with skip lines.
api_miss_dir="${SCRATCH}/api_miss"
mkdir -p "${api_miss_dir}/bin"
cat >"${api_miss_dir}/bin/cargo" <<'STUB'
#!/usr/bin/env bash
if [[ "$*" == *semver-checks* ]] || [[ "$*" == *public-api* ]]; then
  echo "error: no such command" >&2
  exit 101
fi
exit 0
STUB
chmod +x "${api_miss_dir}/bin/cargo"
api_miss_exit=0
(
  cd "${ROOT}"
  PATH="${api_miss_dir}/bin:${PATH}" \
  BASE_REV=HEAD \
  ASSAY_INSTALL_API_DRIFT_TOOLS=0 \
    bash "${OPTIONAL}" >"${api_miss_dir}/out" 2>&1
) || api_miss_exit=$?
[[ "${api_miss_exit}" -eq 0 ]] || fail "optional-public-api-drift missing-tool install=0 exited ${api_miss_exit}, expected 0"
expect_in_file "${api_miss_dir}/out" "skip cargo-semver-checks: cargo subcommand not installed" \
  "api-drift skip names cargo-semver-checks"
expect_in_file "${api_miss_dir}/out" "skip cargo-public-api: cargo subcommand not installed" \
  "api-drift skip names cargo-public-api"
expect_in_file "${api_miss_dir}/out" "no optional public API drift tools installed" \
  "api-drift reports no optional tools installed"

# --- Mutations must bite -----------------------------------------------------------------

SANDBOX_ROOT="$(mktemp -d)"
trap 'rm -rf "${SCRATCH}" "${SANDBOX_ROOT}"' EXIT

echo "== mutation: remove CARGO_PUBLIC_API_VERSION pin =="
no_pin="$(mktemp "${SANDBOX_ROOT}/no-pin.XXXXXX.sh")"
python3 - "${VERSIONS}" "${no_pin}" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
new, n = re.subn(
    r'^(export[ \t]+)?CARGO_PUBLIC_API_VERSION=.*\n',
    "",
    text,
    count=1,
    flags=re.M,
)
if n != 1:
    raise SystemExit(f"could not remove CARGO_PUBLIC_API_VERSION (n={n})")
dst.write_text(new)
PY
if (
  VERSIONS="${no_pin}" bash -c '
    set -euo pipefail
    ROOT="'"${ROOT}"'"
    VERSIONS="'"${no_pin}"'"
    OPTIONAL="'"${OPTIONAL}"'"
    MUTATION="'"${MUTATION}"'"
    # Re-run only the pin-presence portion by sourcing the mutant versions file.
    grep -qE "^(export[[:space:]]+)?CARGO_PUBLIC_API_VERSION=" "${VERSIONS}"
  '
) >/dev/null 2>&1; then
  fail "removing CARGO_PUBLIC_API_VERSION left pin presence green"
fi
ok "removing CARGO_PUBLIC_API_VERSION turns pin presence red"

echo "== mutation: optional helper unpins public-api install =="
opt_unpin="$(mktemp "${SANDBOX_ROOT}/opt-unpin.XXXXXX.sh")"
python3 - "${OPTIONAL}" "${opt_unpin}" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
new, n = re.subn(
    r'RUSTUP_TOOLCHAIN="\$\{RUSTUP_TOOLCHAIN:-stable\}" cargo install --locked --version "\$\{CARGO_PUBLIC_API_VERSION\}" cargo-public-api',
    'cargo install --locked cargo-public-api',
    text,
    count=1,
)
if n != 1:
    # Also try replacing the pre-D3 generic form if GREEN not yet applied.
    new2, n2 = re.subn(
        r'cargo install --locked "\$\{crate\}"',
        'cargo install --locked cargo-public-api',
        text,
        count=1,
    )
    if n2 != 1 and 'CARGO_PUBLIC_API_VERSION' not in text:
        # Pre-GREEN: helper has no pin yet — contract already red via check_optional_helper.
        dst.write_text(text)
        raise SystemExit(0)
    if n != 1 and n2 != 1:
        raise SystemExit(f"could not unpin public-api install (n={n}, n2={n2})")
    dst.write_text(new2)
else:
    dst.write_text(new)
PY
if ( check_optional_helper "${opt_unpin}" ) >/dev/null 2>&1; then
  fail "unpinning optional public-api install left the helper contract green"
fi
ok "unpinning optional public-api install turns the helper contract red"

echo "== mutation: optional helper drops public-api assert =="
opt_noassert="$(mktemp "${SANDBOX_ROOT}/opt-na.XXXXXX.sh")"
python3 - "${OPTIONAL}" "${opt_noassert}" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
new, n = re.subn(
    r'^[ \t]*assert_cargo_plugin_version[ \t]+cargo-public-api[ \t]+"\$\{CARGO_PUBLIC_API_VERSION\}"[ \t]*\n',
    "",
    text,
    count=1,
    flags=re.M,
)
if n != 1:
    # Pre-GREEN: no assert line yet.
    dst.write_text(text)
    raise SystemExit(0)
dst.write_text(new)
PY
if ( check_optional_helper "${opt_noassert}" ) >/dev/null 2>&1; then
  fail "removing optional public-api assert left the helper contract green"
fi
ok "removing optional public-api assert turns the helper contract red"

echo "== mutation: optional helper drops public-api invocation binding =="
opt_unbound="$(mktemp "${SANDBOX_ROOT}/opt-unbound.XXXXXX.sh")"
python3 - "${OPTIONAL}" "${opt_unbound}" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
# Prefer unbinding a bound public-api diff/help; fall back to leaving pre-GREEN unbound.
patterns = [
    (
        'RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}" cargo public-api diff --help',
        "cargo public-api diff --help",
    ),
    (
        'RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}" cargo public-api --help',
        "cargo public-api --help",
    ),
    (
        'RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}" cargo public-api diff',
        "cargo public-api diff",
    ),
]
for old, new in patterns:
    if old in text:
        dst.write_text(text.replace(old, new, 1))
        break
else:
    dst.write_text(text)
PY
if ( check_optional_helper "${opt_unbound}" ) >/dev/null 2>&1; then
  fail "removing optional public-api toolchain binding left the helper contract green"
fi
ok "removing optional public-api binding turns the helper contract red"

echo "== mutation: optional helper restates public-api version literal =="
opt_lit="$(mktemp "${SANDBOX_ROOT}/opt-lit.XXXXXX.sh")"
python3 - "${OPTIONAL}" "${opt_lit}" "${PUBLIC_API_PIN}" <<'PY'
import pathlib, sys
src, dst, pin = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
text = src.read_text()
old = 'cargo install --locked --version "${CARGO_PUBLIC_API_VERSION}" cargo-public-api'
new = f'cargo install --locked --version "{pin}" cargo-public-api'
if old not in text:
    dst.write_text(text)
    raise SystemExit(0)
dst.write_text(text.replace(old, new, 1))
PY
if ( check_optional_helper "${opt_lit}" ) >/dev/null 2>&1; then
  fail "restating public-api version literal left the helper contract green"
fi
ok "restating public-api version literal turns the helper contract red"

echo "== mutation: optional helper uses cargo +stable install =="
opt_plus="$(mktemp "${SANDBOX_ROOT}/opt-plus.XXXXXX.sh")"
python3 - "${OPTIONAL}" "${opt_plus}" <<'PY'
import pathlib, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
old = 'RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}" cargo install --locked --version "${CARGO_PUBLIC_API_VERSION}" cargo-public-api'
new = 'cargo +stable install --locked --version "${CARGO_PUBLIC_API_VERSION}" cargo-public-api'
if old not in text:
    dst.write_text(text)
    raise SystemExit(0)
dst.write_text(text.replace(old, new, 1))
PY
if ( check_optional_helper "${opt_plus}" ) >/dev/null 2>&1; then
  fail "cargo +stable install left the helper contract green"
fi
ok "cargo +stable install turns the helper contract red"

echo "== mutation: mutation-smoke unpins mutants install =="
mut_unpin="$(mktemp "${SANDBOX_ROOT}/mut-unpin.XXXXXX.sh")"
python3 - "${MUTATION}" "${mut_unpin}" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
new, n = re.subn(
    r'RUSTUP_TOOLCHAIN="\$\{RUSTUP_TOOLCHAIN:-stable\}" cargo install --locked --version "\$\{CARGO_MUTANTS_VERSION\}" cargo-mutants',
    "cargo install --locked cargo-mutants",
    text,
    count=1,
)
if n != 1:
    new2, n2 = re.subn(
        r'cargo install --locked cargo-mutants',
        "cargo install --locked cargo-mutants",
        text,
        count=1,
    )
    dst.write_text(text if n2 == 1 else text)
    raise SystemExit(0)
dst.write_text(new)
PY
if ( check_mutation_smoke "${mut_unpin}" ) >/dev/null 2>&1; then
  fail "unpinning mutants install left the mutation-smoke contract green"
fi
ok "unpinning mutants install turns the mutation-smoke contract red"

echo "== mutation: mutation-smoke drops assert =="
mut_noassert="$(mktemp "${SANDBOX_ROOT}/mut-na.XXXXXX.sh")"
python3 - "${MUTATION}" "${mut_noassert}" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
new, n = re.subn(
    r'^[ \t]*assert_cargo_plugin_version[ \t]+cargo-mutants[ \t]+"\$\{CARGO_MUTANTS_VERSION\}"[ \t]*\n',
    "",
    text,
    count=1,
    flags=re.M,
)
if n != 1:
    dst.write_text(text)
    raise SystemExit(0)
dst.write_text(new)
PY
if ( check_mutation_smoke "${mut_noassert}" ) >/dev/null 2>&1; then
  fail "removing mutants assert left the mutation-smoke contract green"
fi
ok "removing mutants assert turns the mutation-smoke contract red"

echo "== mutation: mutation-smoke drops invocation binding =="
mut_unbound="$(mktemp "${SANDBOX_ROOT}/mut-unbound.XXXXXX.sh")"
python3 - "${MUTATION}" "${mut_unbound}" <<'PY'
import pathlib, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
old = 'RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}" cargo mutants --package assay-evidence --file crates/assay-evidence/src/trust_basis/diff.rs'
new = "cargo mutants --package assay-evidence --file crates/assay-evidence/src/trust_basis/diff.rs"
if old not in text:
    # Pre-GREEN unbound form — contract already red.
    dst.write_text(text)
    raise SystemExit(0)
dst.write_text(text.replace(old, new, 1))
PY
if ( check_mutation_smoke "${mut_unbound}" ) >/dev/null 2>&1; then
  fail "removing mutants invocation binding left the mutation-smoke contract green"
fi
ok "removing mutants invocation binding turns the mutation-smoke contract red"

echo "== mutation: mutation-smoke drops source =="
mut_nosource="$(mktemp "${SANDBOX_ROOT}/mut-nosource.XXXXXX.sh")"
python3 - "${MUTATION}" "${mut_nosource}" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
new, n = re.subn(
    r'^[ \t]*# shellcheck source=scripts/ci/cargo-plugin-versions\.sh[ \t]*\n'
    r'^[ \t]*source[ \t]+.*"\$\{ROOT\}/scripts/ci/cargo-plugin-versions\.sh"[ \t]*\n',
    "",
    text,
    count=1,
    flags=re.M,
)
if n != 1:
    new2, n2 = re.subn(
        r'^[ \t]*source[ \t]+.*cargo-plugin-versions\.sh[ \t]*\n',
        "",
        text,
        count=1,
        flags=re.M,
    )
    if n2 != 1:
        dst.write_text(text)
        raise SystemExit(0)
    dst.write_text(new2)
else:
    dst.write_text(new)
PY
if ( check_mutation_smoke "${mut_nosource}" ) >/dev/null 2>&1; then
  fail "removing mutation-smoke source left the contract green"
fi
ok "removing mutation-smoke source turns the contract red"

echo "== mutation: mutation-smoke loses trust_basis target args =="
mut_target="$(mktemp "${SANDBOX_ROOT}/mut-target.XXXXXX.sh")"
python3 - "${MUTATION}" "${mut_target}" <<'PY'
import pathlib, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
old = "crates/assay-evidence/src/trust_basis/diff.rs"
new = "crates/assay-evidence/src/trust_basis/REMOVED.rs"
if old not in text:
    raise SystemExit("could not find trust_basis/diff.rs target to remove")
dst.write_text(text.replace(old, new, 1))
PY
if ( check_mutation_smoke "${mut_target}" ) >/dev/null 2>&1; then
  fail "losing trust_basis/diff.rs target left the mutation-smoke contract green"
fi
ok "losing trust_basis target turns the mutation-smoke contract red"

echo "== mutation: optional helper drops semver D2 pin (preserve D2) =="
opt_semver="$(mktemp "${SANDBOX_ROOT}/opt-semver.XXXXXX.sh")"
python3 - "${OPTIONAL}" "${opt_semver}" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
new, n = re.subn(
    r'RUSTUP_TOOLCHAIN="\$\{RUSTUP_TOOLCHAIN:-stable\}" cargo install --locked --version "\$\{CARGO_SEMVER_CHECKS_VERSION\}" cargo-semver-checks',
    "cargo install --locked cargo-semver-checks",
    text,
    count=1,
)
if n != 1:
    raise SystemExit(f"could not unpin optional semver install (n={n})")
dst.write_text(new)
PY
if ( check_optional_helper "${opt_semver}" ) >/dev/null 2>&1; then
  fail "unpinning optional semver-checks (D2) left the D3 helper contract green"
fi
ok "unpinning optional semver-checks turns the helper contract red"

ok "optional plugin-versions contract mutations bite"
echo "PASS: optional plugin-versions contract"
