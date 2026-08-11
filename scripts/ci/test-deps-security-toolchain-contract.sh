#!/usr/bin/env bash
# Contract for ci.yml's deps-security toolchain statement and its accuracy claims.
#
# install-cargo-deny.sh already self-tests the shell half of the pin. Nothing read the workflow:
# deleting `RUSTUP_TOOLCHAIN: stable` from the job, or restoring the false "never one that compiles
# a crate" claim, stayed green across actionlint (when YAML stayed valid), the install self-test,
# and the CI gate expectations harness. This gate reads the job the way those mutations edit it.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/ci.yml"
INSTALL="${ROOT}/scripts/ci/install-cargo-deny.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

ok() { echo "ok   $*"; }

abort_is_failure() {
  local rc="$1"
  [[ "${rc}" -eq 0 ]] || echo "deps-security toolchain contract aborted (exit ${rc}); treat as failure" >&2
}
trap 'abort_is_failure "$?"' ERR

[[ -f "${WORKFLOW}" ]] || fail "missing ${WORKFLOW#"${ROOT}"/}"
[[ -f "${INSTALL}" ]] || fail "missing ${INSTALL#"${ROOT}"/}"

SANDBOX_ROOT="$(mktemp -d)"
trap 'rm -rf "${SANDBOX_ROOT}"' EXIT

# Extract the deps-security job body (from its key through the next top-level job key).
deps_security_job() {
  local wf="$1"
  awk '
    /^  deps-security:[[:space:]]*$/ { in_job=1; print; next }
    in_job && /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { exit }
    in_job { print }
  ' "${wf}"
}

# Active (non-comment) job-level env lines under deps-security.
# Job-level `env:` sits at 4 spaces; its entries at 6. Step env blocks are deeper and ignored.
deps_security_job_env_lines() {
  local wf="$1"
  deps_security_job "${wf}" | awk '
    /^    env:[[:space:]]*$/ { in_env=1; next }
    in_env && /^    [A-Za-z0-9_-]+:/ { in_env=0 }
    in_env && /^    [A-Za-z]/ { in_env=0 }
    in_env && /^      [^#[:space:]]/ { print }
  '
}

check_workflow() {
  local wf="$1"
  local job env_lines

  job="$(deps_security_job "${wf}")"
  [[ -n "${job}" ]] || fail "could not find deps-security job in ${wf##*/}"

  env_lines="$(deps_security_job_env_lines "${wf}")"
  # Trim leading spaces; YAML indent is not part of the key/value contract.
  trimmed_env="$(sed 's/^[[:space:]]*//' <<<"${env_lines}")"
  grep -qx 'RUSTUP_TOOLCHAIN: stable' <<<"${trimmed_env}" \
    || fail "deps-security job env must state \`RUSTUP_TOOLCHAIN: stable\` as an active line; got:
${env_lines:-<empty>}"

  # #2235: the job compiles the dependency tools from source. Claiming it never compiles a crate
  # is false and teaches the next editor the wrong scope for RUSTUP_TOOLCHAIN.
  if grep -qF 'never one that compiles a crate' <<<"${job}"; then
    fail "deps-security comment still claims the job never compiles a crate; cargo install builds tools from source"
  fi

  grep -qF 'dependency tools' <<<"${job}" \
    || fail "deps-security comment must state that the job compiles the dependency tools from source"

  grep -qF 'rust-toolchain.toml' <<<"${job}" \
    || fail "deps-security comment must keep rust-toolchain.toml as the source for workspace compile jobs"

  # Resolved-toolchain echo lives in the install script (one place for both workflows that install).
  grep -qE '^resolved_toolchain_label[[:space:]]*\(\)' "${INSTALL}" \
    || fail "install-cargo-deny.sh must define resolved_toolchain_label()"
  grep -qF '(resolved ' "${INSTALL}" \
    || fail "install-cargo-deny.sh must echo the resolved toolchain beside the requested alias"
  grep -qF 'rustc -vV' "${INSTALL}" \
    || fail "install-cargo-deny.sh must resolve the toolchain via rustc -vV"

  # Bound binary: PATH must not answer the pin check.
  grep -qE '^cargo_deny_bin_path[[:space:]]*\(\)' "${INSTALL}" \
    || fail "install-cargo-deny.sh must define cargo_deny_bin_path for the pin check"
  grep -qF '"$(cargo_deny_bin_path)"' "${INSTALL}" || grep -qF '$(cargo_deny_bin_path)' "${INSTALL}" \
    || fail "install-cargo-deny.sh must call cargo_deny_bin_path for the version assertion"

  ok "deps-security toolchain contract holds for ${wf##*/}"
}

check_workflow "${WORKFLOW}"

# --- Mutations must bite -------------------------------------------------------

mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX.yml")"

# R2: delete the active toolchain statement but keep a dummy env entry so YAML/actionlint stay valid.
awk '
  /^  deps-security:[[:space:]]*$/ { in_job=1 }
  in_job && /^  [A-Za-z0-9_-]+:[[:space:]]*$/ && !/^  deps-security:/ { in_job=0 }
  in_job && /^    env:[[:space:]]*$/ { in_env=1; print; print "      CI_PLACEHOLDER: 1"; next }
  in_env && /^      RUSTUP_TOOLCHAIN:[[:space:]]*stable[[:space:]]*$/ { next }
  in_env && /^    [A-Za-z0-9_-]+:/ { in_env=0 }
  in_env && /^    [A-Za-z]/ { in_env=0 }
  { print }
' "${WORKFLOW}" >"${mutant}"
grep -q 'CI_PLACEHOLDER: 1' "${mutant}" \
  || fail "RUSTUP_TOOLCHAIN deletion mutation did not apply"
if grep -q '^[[:space:]]*RUSTUP_TOOLCHAIN:[[:space:]]*stable[[:space:]]*$' "${mutant}"; then
  # Only accept absence inside deps-security env; a mention in a comment is fine.
  env_after="$(deps_security_job_env_lines "${mutant}")"
  if grep -qx 'RUSTUP_TOOLCHAIN: stable' <<<"$(sed 's/^[[:space:]]*//' <<<"${env_after}")"; then
    fail "RUSTUP_TOOLCHAIN deletion mutation still leaves an active env line"
  fi
fi
if ( check_workflow "${mutant}" ) >/dev/null 2>&1; then
  fail "deleting deps-security RUSTUP_TOOLCHAIN left the contract green"
fi
ok "deleting deps-security RUSTUP_TOOLCHAIN turns the contract red"

# Weakening: name a toolchain the job does not install (the original incident value).
weak="$(mktemp "${SANDBOX_ROOT}/weak.XXXXXX.yml")"
sed 's/^      RUSTUP_TOOLCHAIN: stable$/      RUSTUP_TOOLCHAIN: 1.96.0/' "${WORKFLOW}" >"${weak}"
grep -q '^      RUSTUP_TOOLCHAIN: 1.96.0$' "${weak}" \
  || fail "RUSTUP_TOOLCHAIN weakening mutation did not apply"
if ( check_workflow "${weak}" ) >/dev/null 2>&1; then
  fail "changing deps-security RUSTUP_TOOLCHAIN to 1.96.0 left the contract green"
fi
ok "weakening deps-security RUSTUP_TOOLCHAIN to 1.96.0 turns the contract red"

# R4: restore the false compiles-nothing claim.
false_comment="$(mktemp "${SANDBOX_ROOT}/false.XXXXXX.yml")"
sed 's/dependency tools themselves, installed from source/dependency tooling, never one that compiles a crate/' \
  "${WORKFLOW}" >"${false_comment}"
# If the green comment is not yet in place, seed the false phrase explicitly for the mutation proof.
if ! grep -qF 'never one that compiles a crate' "${false_comment}"; then
  sed 's/rust-toolchain.toml stays the source/never one that compiles a crate, and rust-toolchain.toml stays the source/' \
    "${WORKFLOW}" >"${false_comment}"
fi
grep -qF 'never one that compiles a crate' "${false_comment}" \
  || fail "false-comment mutation did not apply"
if ( check_workflow "${false_comment}" ) >/dev/null 2>&1; then
  fail "restoring the false compiles-nothing claim left the contract green"
fi
ok "restoring the false compiles-nothing claim turns the contract red"

# R3: strip resolved-toolchain echo from the install script. check_workflow reads global INSTALL.
check_install_resolved() {
  local install="$1"
  grep -qE '^resolved_toolchain_label[[:space:]]*\(\)' "${install}" || return 1
  grep -qF '(resolved ' "${install}" || return 1
  grep -qF 'rustc -vV' "${install}" || return 1
  return 0
}

if ! check_install_resolved "${INSTALL}"; then
  fail "install-cargo-deny.sh does not yet echo a resolved toolchain beside the requested alias"
fi

install_mutant="${SANDBOX_ROOT}/install-mutant.sh"
cp "${INSTALL}" "${install_mutant}"
sed -i.bak \
  -e 's/resolved_toolchain_label/disabled_resolved_toolchain_label/g' \
  -e 's/(resolved /(/g' \
  -e '/rustc -vV/d' \
  "${install_mutant}"
rm -f "${install_mutant}.bak"
if check_install_resolved "${install_mutant}"; then
  fail "resolved-toolchain echo mutation left reporting intact — mutation did not apply"
fi
INSTALL_SAVE="${INSTALL}"
INSTALL="${install_mutant}"
if ( check_workflow "${WORKFLOW}" ) >/dev/null 2>&1; then
  INSTALL="${INSTALL_SAVE}"
  fail "removing resolved-toolchain echo left the contract green"
fi
INSTALL="${INSTALL_SAVE}"
ok "removing resolved-toolchain echo turns the contract red"

ok "deps-security toolchain contract mutations bite"
echo "PASS: deps-security toolchain contract"
