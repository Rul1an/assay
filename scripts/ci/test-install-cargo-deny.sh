#!/usr/bin/env bash
# Self-test for scripts/ci/install-cargo-deny.sh.
#
# The install script's guarantees were verified once, by hand, in a pull-request body: that the argv
# carries `--version`, that a mismatching version fails, and that an inherited RUSTUP_TOOLCHAIN wins
# over the default. Evidence in a PR body runs once and never again, so removing `--version` stayed
# green -- and would stay green until upstream published a new release, at which point the failure
# would point at the install rather than at the deletion that caused it.
#
# A stub `cargo`, `cargo-deny`, and `rustc` on PATH make each guarantee cheap to check for real. No
# network, no toolchain, no minutes-long install. The stub `cargo` writes the binary into Cargo's
# install location so an unrelated `cargo-deny` earlier on PATH cannot satisfy the pin check.
set -euo pipefail

# Any abort is a failure, never a pass. Found the hard way: a mutation that removed the install
# script's `RUSTUP_TOOLCHAIN` export made it die on an unbound variable, this script printed `FAIL`,
# then died itself on an unset case variable -- and exited 0. A gate that reports a failure and
# returns success is worse than no gate, so the abort path says so and keeps the non-zero status.
abort_is_failure() {
  local rc="$1"
  if [[ "${rc}" -ne 0 ]]; then
    echo "install-cargo-deny self-test aborted (exit ${rc}); treat as failure" >&2
  fi
}
trap 'abort_is_failure "$?"' ERR

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
UNDER_TEST="${SCRIPT_DIR}/install-cargo-deny.sh"
PIN="$(sed -n 's/^CARGO_DENY_VERSION="\(.*\)"$/\1/p' "${UNDER_TEST}")"

if [[ -z "${PIN}" ]]; then
  echo "could not read CARGO_DENY_VERSION from ${UNDER_TEST}; the assignment's shape moved" >&2
  exit 1
fi

# macOS ships /bin/bash 3.2.57. The @Q parameter transformation is bash 4.4+ and turns the
# failure path into `bad substitution` instead of naming the assertion (#2250). Refuse that
# spelling in active code (comments may mention it).
if awk '
  /^[[:space:]]*#/ { next }
  /\$\{[^}]+@Q\}/ { found=1; print NR ":" $0 }
  END { exit found ? 0 : 1 }
' "${BASH_SOURCE[0]}"; then
  echo "FAIL self-test uses bash-4.4 @Q quoting; macOS bash 3.2 aborts with bad substitution" >&2
  exit 1
fi

# One rule, one function: the install script owns the bin-path formula. Source it (it does not run
# its body when sourced) and call the same helper the production check uses.
if ! grep -qE '^cargo_deny_bin_path[[:space:]]*\(\)' "${UNDER_TEST}"; then
  echo "FAIL install-cargo-deny.sh does not define cargo_deny_bin_path()" >&2
  exit 1
fi
if ! grep -qE '^resolved_toolchain_label[[:space:]]*\(\)' "${UNDER_TEST}"; then
  echo "FAIL install-cargo-deny.sh does not define resolved_toolchain_label()" >&2
  exit 1
fi
# Refuse to source a script that still runs its body on source — that would invoke a real install.
if ! grep -q 'BASH_SOURCE\[0\]' "${UNDER_TEST}"; then
  echo "FAIL install-cargo-deny.sh is not source-safe (missing BASH_SOURCE execute guard)" >&2
  exit 1
fi
# shellcheck source=scripts/ci/install-cargo-deny.sh
source "${UNDER_TEST}"

SCRATCH="$(mktemp -d)"
trap 'rm -rf "${SCRATCH}"' EXIT
failures=0

# Build a stub PATH. `cargo` records argv/toolchain and writes `cargo-deny` into Cargo's install
# location. A separate early PATH entry can hold a decoy binary that must not satisfy the pin.
make_stubs() {
  local dir="$1" reported="$2"
  mkdir -p "${dir}"
  cat >"${dir}/cargo" <<'STUB'
#!/usr/bin/env bash
echo "argv: $*" >>"${STUB_LOG}"
echo "toolchain: ${RUSTUP_TOOLCHAIN:-<unset>}" >>"${STUB_LOG}"
root="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-${HOME}/.cargo}}"
mkdir -p "${root}/bin"
cat >"${root}/bin/cargo-deny" <<INNER
#!/usr/bin/env bash
echo "cargo-deny ${STUB_DENY_VERSION}"
INNER
chmod +x "${root}/bin/cargo-deny"
STUB
  cat >"${dir}/rustc" <<'STUB'
#!/usr/bin/env bash
if [[ "${1:-}" == "-vV" ]]; then
  cat <<EOF
rustc 1.77.0-stub
binary: rustc
commit-hash: stub
commit-date: stub
host: stub
release: 1.77.0-stub
EOF
  exit 0
fi
echo "rustc stub: unexpected args: $*" >&2
exit 1
STUB
  chmod +x "${dir}/cargo" "${dir}/rustc"
  # Keep a PATH-visible cargo-deny that reports a *different* version so a PATH-first check cannot
  # accidentally pass. The install must read the binary cargo wrote into the install root.
  cat >"${dir}/cargo-deny" <<STUB
#!/usr/bin/env bash
echo "cargo-deny 9.9.9-path-decoy"
STUB
  chmod +x "${dir}/cargo-deny"
  # Exported for the cargo stub's here-doc expansion at install time.
  export STUB_DENY_VERSION="${reported}"
}

# Each case runs the script under stubs and reports the exit code plus the recorded argv.
run_case() {
  local name="$1" reported="$2" expected_exit="$3"
  shift 3
  local dir="${SCRATCH}/${name}"
  local cargo_home="${dir}/cargo-home"
  mkdir -p "${cargo_home}"
  make_stubs "${dir}" "${reported}"
  local log="${dir}/log"
  : >"${log}"

  # Published before the exit-code check, not after. When a case failed, an early `return` left these
  # unset, the next `expect_in_file` hit `set -u`, and the script aborted before it could count the
  # failure -- which is how a FAIL could coexist with exit 0.
  CASE_LOG="${log}"
  CASE_OUT="${dir}/out"
  CASE_HOME="${dir}/case-home"
  CASE_CARGO_HOME="${cargo_home}"
  mkdir -p "${CASE_HOME}"

  local actual_exit=0
  # Controlled PATH + toolchain + Cargo home. HOME is isolated so a missing CARGO_HOME still
  # resolves under this case rather than the operator's real ~/.cargo.
  STUB_LOG="${log}" STUB_DENY_VERSION="${reported}" \
    HOME="${CASE_HOME}" CARGO_HOME="${cargo_home}" \
    PATH="${dir}:${PATH}" "$@" bash "${UNDER_TEST}" >"${dir}/out" 2>&1 || actual_exit=$?

  if [[ "${actual_exit}" -ne "${expected_exit}" ]]; then
    echo "FAIL ${name}: exit ${actual_exit}, expected ${expected_exit}"
    sed 's/^/      /' "${dir}/out"
    failures=$((failures + 1))
    return
  fi
  echo "ok   ${name} (exit ${actual_exit})"
}

expect_in_file() {
  local file="$1" needle="$2" what="$3"
  if grep -qF -- "${needle}" "${file}"; then
    echo "ok   ${what}"
  else
    # Plain quoting only — bash 4.4 @Q aborts macOS bash 3.2 (#2250).
    echo "FAIL ${what}: '${needle}' not in"
    sed 's/^/      /' "${file}"
    failures=$((failures + 1))
  fi
}

echo "== the install names the pinned version explicitly =="
# This is the case that matters most. `--locked` reads like a pin and is not one: it pins
# cargo-deny's own dependencies, not cargo-deny. Deleting `--version` was silent before this test.
run_case pinned "${PIN}" 0
expect_in_file "${CASE_LOG}" "argv: install --locked --version ${PIN} cargo-deny" \
  "argv carries --version ${PIN}"

echo "== a version other than the pin fails, and says where the pin lives =="
run_case mismatch "0.0.0-not-the-pin" 1
expect_in_file "${CASE_OUT}" "is not the pinned ${PIN}" "mismatch names the pin"
expect_in_file "${CASE_OUT}" "scripts/ci/install-cargo-deny.sh" "mismatch names the pin's location"

echo "== the toolchain is stated, and an inherited value wins =="
run_case default_toolchain "${PIN}" 0
expect_in_file "${CASE_LOG}" "toolchain: stable" "defaults to stable when the caller states nothing"
expect_in_file "${CASE_OUT}" "using toolchain stable (resolved 1.77.0-stub)" \
  "requested alias and resolved toolchain are both logged"

run_case inherited_toolchain "${PIN}" 0 env RUSTUP_TOOLCHAIN=1.2.3-inherited
expect_in_file "${CASE_LOG}" "toolchain: 1.2.3-inherited" "an inherited RUSTUP_TOOLCHAIN is honoured"
expect_in_file "${CASE_OUT}" "using toolchain 1.2.3-inherited (resolved 1.77.0-stub)" \
  "inherited alias is logged beside the resolved toolchain"

echo "== the pin check binds the install-root binary, not PATH =="
# Same formula the install uses. An earlier PATH decoy reports 9.9.9; only the install-root binary
# carries the pin. Passing here means PATH did not answer the version check.
expect_in_file "${CASE_OUT}" "at $(CARGO_HOME="${CASE_CARGO_HOME}" cargo_deny_bin_path)" \
  "the resolved path is the install-root binary"

# Decoy-only: cargo writes nothing useful; PATH still has the pin. Must fail.
echo "== an unrelated PATH cargo-deny cannot satisfy the pin =="
decoy_dir="${SCRATCH}/path_decoy"
mkdir -p "${decoy_dir}/early" "${decoy_dir}/tools" "${decoy_dir}/cargo-home" "${decoy_dir}/home"
cat >"${decoy_dir}/early/cargo-deny" <<STUB
#!/usr/bin/env bash
echo "cargo-deny ${PIN}"
STUB
chmod +x "${decoy_dir}/early/cargo-deny"
cat >"${decoy_dir}/tools/cargo" <<'STUB'
#!/usr/bin/env bash
echo "argv: $*" >>"${STUB_LOG}"
echo "toolchain: ${RUSTUP_TOOLCHAIN:-<unset>}" >>"${STUB_LOG}"
# Deliberately do not write an install-root binary.
STUB
cat >"${decoy_dir}/tools/rustc" <<'STUB'
#!/usr/bin/env bash
if [[ "${1:-}" == "-vV" ]]; then
  echo "release: 1.77.0-stub"
  exit 0
fi
exit 1
STUB
chmod +x "${decoy_dir}/tools/cargo" "${decoy_dir}/tools/rustc"
decoy_log="${decoy_dir}/log"
decoy_out="${decoy_dir}/out"
: >"${decoy_log}"
decoy_exit=0
STUB_LOG="${decoy_log}" HOME="${decoy_dir}/home" CARGO_HOME="${decoy_dir}/cargo-home" \
  PATH="${decoy_dir}/early:${decoy_dir}/tools:${PATH}" \
  bash "${UNDER_TEST}" >"${decoy_out}" 2>&1 || decoy_exit=$?
if [[ "${decoy_exit}" -eq 0 ]]; then
  echo "FAIL path decoy: install exited 0 while only PATH carried the pin"
  sed 's/^/      /' "${decoy_out}"
  failures=$((failures + 1))
else
  echo "ok   path decoy refused (exit ${decoy_exit})"
fi
expect_in_file "${decoy_out}" "cargo-deny missing at install location" \
  "path decoy names the missing install-root binary"

echo "== CARGO_INSTALL_ROOT wins over CARGO_HOME for the bound binary =="
root_dir="${SCRATCH}/install_root_case"
mkdir -p "${root_dir}/tools" "${root_dir}/cargo-home" "${root_dir}/install-root" "${root_dir}/home"
make_stubs "${root_dir}/tools" "${PIN}"
# Override make_stubs' PATH decoy — keep it, but point install root elsewhere.
root_log="${root_dir}/log"
root_out="${root_dir}/out"
: >"${root_log}"
root_exit=0
STUB_LOG="${root_log}" STUB_DENY_VERSION="${PIN}" \
  HOME="${root_dir}/home" CARGO_HOME="${root_dir}/cargo-home" \
  CARGO_INSTALL_ROOT="${root_dir}/install-root" \
  PATH="${root_dir}/tools:${PATH}" \
  bash "${UNDER_TEST}" >"${root_out}" 2>&1 || root_exit=$?
if [[ "${root_exit}" -ne 0 ]]; then
  echo "FAIL CARGO_INSTALL_ROOT: exit ${root_exit}, expected 0"
  sed 's/^/      /' "${root_out}"
  failures=$((failures + 1))
else
  echo "ok   CARGO_INSTALL_ROOT (exit 0)"
fi
expect_in_file "${root_out}" "at ${root_dir}/install-root/bin/cargo-deny" \
  "CARGO_INSTALL_ROOT selects the bound binary"

if [[ "${failures}" -ne 0 ]]; then
  echo "install-cargo-deny self-test: ${failures} failure(s)" >&2
  exit 1
fi
echo "install-cargo-deny self-test: all cases passed (pin ${PIN})"
