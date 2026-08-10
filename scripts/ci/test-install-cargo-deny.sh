#!/usr/bin/env bash
# Self-test for scripts/ci/install-cargo-deny.sh.
#
# The install script's guarantees were verified once, by hand, in a pull-request body: that the argv
# carries `--version`, that a mismatching version fails, and that an inherited RUSTUP_TOOLCHAIN wins
# over the default. Evidence in a PR body runs once and never again, so removing `--version` stayed
# green -- and would stay green until upstream published a new release, at which point the failure
# would point at the install rather than at the deletion that caused it.
#
# A stub `cargo` and `cargo-deny` on PATH make each guarantee cheap to check for real. No network, no
# toolchain, no minutes-long install.
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

SCRATCH="$(mktemp -d)"
trap 'rm -rf "${SCRATCH}"' EXIT
failures=0

# Build a stub PATH. `cargo` records its argv and the toolchain it saw; `cargo-deny` reports whatever
# version the case asks for, which is how a mismatching install is simulated without one.
make_stubs() {
  local dir="$1" reported="$2"
  mkdir -p "${dir}"
  cat >"${dir}/cargo" <<'STUB'
#!/usr/bin/env bash
echo "argv: $*" >>"${STUB_LOG}"
echo "toolchain: ${RUSTUP_TOOLCHAIN:-<unset>}" >>"${STUB_LOG}"
STUB
  cat >"${dir}/cargo-deny" <<STUB
#!/usr/bin/env bash
echo "cargo-deny ${reported}"
STUB
  chmod +x "${dir}/cargo" "${dir}/cargo-deny"
}

# Each case runs the script under stubs and reports the exit code plus the recorded argv.
run_case() {
  local name="$1" reported="$2" expected_exit="$3"
  shift 3
  local dir="${SCRATCH}/${name}"
  make_stubs "${dir}" "${reported}"
  local log="${dir}/log"
  : >"${log}"

  # Published before the exit-code check, not after. When a case failed, an early `return` left these
  # unset, the next `expect_in_file` hit `set -u`, and the script aborted before it could count the
  # failure -- which is how a FAIL could coexist with exit 0.
  CASE_LOG="${log}"
  CASE_OUT="${dir}/out"

  local actual_exit=0
  # `env -i` would drop too much; the point is a controlled PATH and a controlled toolchain value.
  STUB_LOG="${log}" PATH="${dir}:${PATH}" "$@" bash "${UNDER_TEST}" >"${dir}/out" 2>&1 || actual_exit=$?

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
    echo "FAIL ${what}: ${needle@Q} not in"
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

run_case inherited_toolchain "${PIN}" 0 env RUSTUP_TOOLCHAIN=1.2.3-inherited
expect_in_file "${CASE_LOG}" "toolchain: 1.2.3-inherited" "an inherited RUSTUP_TOOLCHAIN is honoured"

echo "== the log says which binary answered, not only which version =="
# Without the path, a version line is not evidence about the install: an unrelated cargo-deny on
# PATH reporting the pinned version would produce an identical log.
expect_in_file "${CASE_OUT}" "at ${SCRATCH}/inherited_toolchain/cargo-deny" \
  "the resolved path is reported"

if [[ "${failures}" -ne 0 ]]; then
  echo "install-cargo-deny self-test: ${failures} failure(s)" >&2
  exit 1
fi
echo "install-cargo-deny self-test: all cases passed (pin ${PIN})"
