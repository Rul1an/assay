#!/usr/bin/env bash
# Contract for the shared cargo-plugin version source and ci.yml's cargo-audit pin (CI-4D1 / #2224).
#
# Before this gate, deps-security installed cargo-audit with `cargo install --locked cargo-audit`
# and a comment that floating the scanner was deliberate. `--locked` pins the tool's own
# dependencies, not the tool version, so a new cargo-audit release could redden an unrelated PR.
# Advisory freshness comes from refreshing the advisory DB, not from resolving a new scanner on
# every run. The pin and the runtime assertion must read one checked-in value.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSIONS="${ROOT}/scripts/ci/cargo-plugin-versions.sh"
WORKFLOW="${ROOT}/.github/workflows/ci.yml"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

ok() { echo "ok   $*"; }

abort_is_failure() {
  local rc="$1"
  [[ "${rc}" -eq 0 ]] || echo "cargo-plugin-versions contract aborted (exit ${rc}); treat as failure" >&2
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
[[ -f "${VERSIONS}" ]] || fail "missing shared version source ${VERSIONS#"${ROOT}"/} (unpinned cargo-audit install has no single owner)"

# Refuse to source a script that still runs install-like work on source.
grep -q 'BASH_SOURCE\[0\]' "${VERSIONS}" \
  || fail "cargo-plugin-versions.sh is not source-safe (missing BASH_SOURCE execute guard)"
grep -qE '^(export[[:space:]]+)?CARGO_AUDIT_VERSION=' "${VERSIONS}" \
  || fail "cargo-plugin-versions.sh must define CARGO_AUDIT_VERSION"
grep -qE '^cargo_plugin_bin_path[[:space:]]*\(\)' "${VERSIONS}" \
  || fail "cargo-plugin-versions.sh must define cargo_plugin_bin_path()"
grep -qE '^assert_cargo_plugin_version[[:space:]]*\(\)' "${VERSIONS}" \
  || fail "cargo-plugin-versions.sh must define assert_cargo_plugin_version()"

# shellcheck source=scripts/ci/cargo-plugin-versions.sh
source "${VERSIONS}"

PIN="${CARGO_AUDIT_VERSION:-}"
[[ -n "${PIN}" ]] || fail "CARGO_AUDIT_VERSION is empty after sourcing ${VERSIONS#"${ROOT}"/}"

# Extract the deps-security job body.
deps_security_job() {
  local wf="$1"
  awk '
    /^  deps-security:[[:space:]]*$/ { in_job=1; print; next }
    in_job && /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { exit }
    in_job { print }
  ' "${wf}"
}

# Active (non-comment, non-blank) lines of the Install cargo-audit step run block.
# Whole-line `# …` ghosts are dropped here. Trailing `# …` on an active line is still emitted;
# check_workflow therefore requires complete anchored command lines so those tails cannot satisfy
# source / --version / assert (#2317 review 4914059003). No shell comment parser.
install_cargo_audit_run() {
  local wf="$1"
  deps_security_job "${wf}" | awk '
    function emit_active(line,    tmp) {
      tmp = line
      sub(/^[[:space:]]+/, "", tmp)
      if (tmp == "" || tmp ~ /^#/) return
      print line
    }
    /^      - name: Install cargo-audit[[:space:]]*$/ { in_step=1; next }
    in_step && /^      - name:/ { exit }
    # Block scalars: run: | / |- / |+ with optional spaces (#2317 review).
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

# Complete active command lines only (optional leading/trailing blank). Trailing `# ghosts`
# on the same line cannot match these end-anchored patterns.
active_source_line() {
  grep -qE '^[[:space:]]*source[[:space:]]+(\./scripts/ci/cargo-plugin-versions\.sh|"(\./)?scripts/ci/cargo-plugin-versions\.sh")[[:space:]]*$' <<<"$1"
}

count_active_cargo_audit_installs() {
  # grep -c exits 1 on zero matches; keep zero under set -e.
  printf '%s\n' "$1" | grep -cE '^[[:space:]]*cargo[[:space:]]+install[[:space:]].*[[:space:]]cargo-audit[[:space:]]*$' || true
}

active_pinned_install_line() {
  grep -qE '^[[:space:]]*cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+--version[[:space:]]+"\$\{CARGO_AUDIT_VERSION\}"[[:space:]]+cargo-audit[[:space:]]*$' <<<"$1"
}

active_literal_install_line() {
  # Restated pin as a complete install argv (not a # tail after an unpinned install).
  grep -qE '^[[:space:]]*cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+--version[[:space:]]+"?'"${PIN}"'"?[[:space:]]+cargo-audit[[:space:]]*$' <<<"$1"
}

active_assert_line() {
  grep -qE '^[[:space:]]*assert_cargo_plugin_version[[:space:]]+cargo-audit[[:space:]]+"\$\{CARGO_AUDIT_VERSION\}"[[:space:]]*$' <<<"$1"
}

check_workflow() {
  local wf="$1"
  local job install_run install_count

  job="$(deps_security_job "${wf}")"
  [[ -n "${job}" ]] || fail "could not find deps-security job in ${wf##*/}"

  install_run="$(install_cargo_audit_run "${wf}")"
  [[ -n "${install_run}" ]] || fail "could not find Install cargo-audit run block in ${wf##*/}"

  # Must source the shared pin — both install --version and the runtime assertion read it.
  active_source_line "${install_run}" \
    || fail "Install cargo-audit must have an active complete line sourcing scripts/ci/cargo-plugin-versions.sh; got:
${install_run}"

  # Exactly one active cargo install of cargo-audit, and it must be the pinned argv.
  # A second install (pinned or unpinned) after assert would overwrite the pin while a
  # presence-only check stayed green (#2317 review 4914399777).
  install_count="$(count_active_cargo_audit_installs "${install_run}")"
  [[ "${install_count}" -eq 1 ]] \
    || fail "Install cargo-audit must have exactly one active cargo install of cargo-audit; found ${install_count}:
${install_run}"

  active_pinned_install_line "${install_run}" \
    || fail "Install cargo-audit must have an active complete line: cargo install --locked --version \"\${CARGO_AUDIT_VERSION}\" cargo-audit; got:
${install_run}"

  # A restated pin in the workflow can drift from the shared source (AGENTS.md pinning rule).
  if active_literal_install_line "${install_run}"; then
    fail "Install cargo-audit restates version literal ${PIN} on an active complete install line; both install and assertion must read CARGO_AUDIT_VERSION"
  fi

  active_assert_line "${install_run}" \
    || fail "Install cargo-audit must have an active complete assert_cargo_plugin_version line; got:
${install_run}"

  # The former floating-scanner rationale must not return; it conflicts with the pin contract.
  if grep -qiE 'floating is arguably right|Version deliberately unpinned|scanner should float' <<<"${job}"; then
    fail "deps-security still carries the floating cargo-audit rationale; pin + advisory-DB refresh is the contract"
  fi

  # Preserve CI-4C: no RUSTSEC ignore on the audit invocations in this job.
  if grep -qE -- '--ignore[[:space:]]+RUSTSEC-' <<<"${job}"; then
    fail "deps-security reintroduced a RUSTSEC --ignore; CI-4C removed those exceptions"
  fi

  ok "deps-security cargo-audit pin contract holds for ${wf##*/}"
}

check_workflow "${WORKFLOW}"

# --- Behavioral: assertion binds install location, not PATH --------------------

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

echo "== assert_cargo_plugin_version accepts the install-root binary =="
pass_dir="${SCRATCH}/assert_pass"
mkdir -p "${pass_dir}/bin" "${pass_dir}/early"
cat >"${pass_dir}/bin/cargo-audit" <<STUB
#!/usr/bin/env bash
echo "cargo-audit ${PIN}"
STUB
chmod +x "${pass_dir}/bin/cargo-audit"
cat >"${pass_dir}/early/cargo-audit" <<'STUB'
#!/usr/bin/env bash
echo "cargo-audit 9.9.9-path-decoy"
STUB
chmod +x "${pass_dir}/early/cargo-audit"
pass_out="${pass_dir}/out"
pass_exit=0
PATH="${pass_dir}/early:${PATH}" CARGO_HOME="${pass_dir}" \
  assert_cargo_plugin_version cargo-audit "${PIN}" >"${pass_out}" 2>&1 || pass_exit=$?
[[ "${pass_exit}" -eq 0 ]] || fail "assert_cargo_plugin_version failed against install-root pin (exit ${pass_exit})"
expect_in_file "${pass_out}" "at ${pass_dir}/bin/cargo-audit" "assertion names the install-root binary"

echo "== wrong installed version fails =="
wrong_dir="${SCRATCH}/assert_wrong"
mkdir -p "${wrong_dir}/bin"
cat >"${wrong_dir}/bin/cargo-audit" <<'STUB'
#!/usr/bin/env bash
echo "cargo-audit 0.0.0-not-the-pin"
STUB
chmod +x "${wrong_dir}/bin/cargo-audit"
wrong_out="${wrong_dir}/out"
wrong_exit=0
CARGO_HOME="${wrong_dir}" \
  assert_cargo_plugin_version cargo-audit "${PIN}" >"${wrong_out}" 2>&1 || wrong_exit=$?
[[ "${wrong_exit}" -ne 0 ]] || fail "wrong installed version left assertion green"
expect_in_file "${wrong_out}" "is not the pinned ${PIN}" "mismatch names the pin"
expect_in_file "${wrong_out}" "scripts/ci/cargo-plugin-versions.sh" "mismatch names the pin's location"

echo "== PATH decoy cannot satisfy the pin =="
decoy_dir="${SCRATCH}/path_decoy"
mkdir -p "${decoy_dir}/early" "${decoy_dir}/cargo-home"
cat >"${decoy_dir}/early/cargo-audit" <<STUB
#!/usr/bin/env bash
echo "cargo-audit ${PIN}"
STUB
chmod +x "${decoy_dir}/early/cargo-audit"
decoy_out="${decoy_dir}/out"
decoy_exit=0
PATH="${decoy_dir}/early:${PATH}" CARGO_HOME="${decoy_dir}/cargo-home" \
  assert_cargo_plugin_version cargo-audit "${PIN}" >"${decoy_out}" 2>&1 || decoy_exit=$?
[[ "${decoy_exit}" -ne 0 ]] || fail "PATH decoy satisfied assert_cargo_plugin_version"
expect_in_file "${decoy_out}" "cargo-audit missing at install location" \
  "path decoy names the missing install-root binary"

# --- Mutations must bite -------------------------------------------------------

SANDBOX_ROOT="$(mktemp -d)"
trap 'rm -rf "${SCRATCH}" "${SANDBOX_ROOT}"' EXIT

mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX.yml")"

echo "== mutation: remove --version =="
# Drop --version and its argument from the install line while keeping source + assert.
python3 - "${WORKFLOW}" "${mutant}" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
# Only mutate the cargo-audit install line that uses the shared variable.
new, n = re.subn(
    r'(cargo install --locked) --version "\$\{CARGO_AUDIT_VERSION\}" (cargo-audit)',
    r"\1 \2",
    text,
    count=1,
)
if n != 1:
    raise SystemExit(f"could not strip --version (n={n})")
dst.write_text(new)
PY
grep -qE '^[[:space:]]*cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+cargo-audit[[:space:]]*$' \
  <<<"$(install_cargo_audit_run "${mutant}")" \
  || fail "--version removal mutation did not leave an active unpinned install line"
if active_pinned_install_line "$(install_cargo_audit_run "${mutant}")"; then
  fail "--version removal mutation still leaves a complete pinned install line"
fi
if ( check_workflow "${mutant}" ) >/dev/null 2>&1; then
  fail "removing --version left the contract green"
fi
ok "removing --version turns the contract red"

echo "== mutation: duplicate workflow literal =="
literal="$(mktemp "${SANDBOX_ROOT}/lit.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${literal}" "${PIN}" <<'PY'
import pathlib, sys
src, dst, pin = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
text = src.read_text()
old = 'cargo install --locked --version "${CARGO_AUDIT_VERSION}" cargo-audit'
new = f'cargo install --locked --version "{pin}" cargo-audit'
if old not in text:
    raise SystemExit("could not find versioned install line to literalize")
dst.write_text(text.replace(old, new, 1))
PY
grep -qF -- "--version \"${PIN}\"" "${literal}" \
  || fail "duplicate-literal mutation did not apply"
if ( check_workflow "${literal}" ) >/dev/null 2>&1; then
  fail "restating the version literal in the workflow left the contract green"
fi
ok "duplicate workflow literal turns the contract red"

echo "== mutation: remove workflow source =="
nosource="$(mktemp "${SANDBOX_ROOT}/nosource.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${nosource}" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
# Drop the active source and its shellcheck directive; leave other mentions untouched.
new, n = re.subn(
    r'^[ \t]*# shellcheck source=scripts/ci/cargo-plugin-versions\.sh[ \t]*\n'
    r'^[ \t]*source[ \t]+\./scripts/ci/cargo-plugin-versions\.sh[ \t]*\n',
    "",
    text,
    count=1,
    flags=re.M,
)
if n != 1:
    raise SystemExit(f"could not remove source block (n={n})")
dst.write_text(new)
PY
install_after="$(install_cargo_audit_run "${nosource}")"
if active_source_line "${install_after}"; then
  fail "source-removal mutation still leaves a complete active source line"
fi
if ( check_workflow "${nosource}" ) >/dev/null 2>&1; then
  fail "removing workflow source left the contract green"
fi
ok "removing workflow source turns the contract red"

echo "== mutation: remove version assertion =="
noassert="$(mktemp "${SANDBOX_ROOT}/noassert.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${noassert}" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
new, n = re.subn(
    r'^[ \t]*assert_cargo_plugin_version[ \t]+cargo-audit[ \t]+"\$\{CARGO_AUDIT_VERSION\}"[ \t]*\n',
    "",
    text,
    count=1,
    flags=re.M,
)
if n != 1:
    raise SystemExit(f"could not remove assert line (n={n})")
dst.write_text(new)
PY
if active_assert_line "$(install_cargo_audit_run "${noassert}")"; then
  fail "assertion-removal mutation still leaves a complete active assert line"
fi
if ( check_workflow "${noassert}" ) >/dev/null 2>&1; then
  fail "removing version assertion left the contract green"
fi
ok "removing version assertion turns the contract red"

echo "== mutation: comment ghosts + active unpinned install =="
# source / pinned install / assert survive only as comments; active argv is unpinned.
# Before active-line filtering this left check_workflow green (#2317 review).
ghost="$(mktemp "${SANDBOX_ROOT}/ghost.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${ghost}" <<'PY'
import pathlib, re, sys

src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
pat = re.compile(
    r"(      - name: Install cargo-audit\n"
    r"(?:        #.*\n)*"
    r"        run: \|\n)"
    r"(?:          .*\n)+"
)
repl = (
    "\\1"
    "          set -euo pipefail\n"
    "          # shellcheck source=scripts/ci/cargo-plugin-versions.sh\n"
    "          # source ./scripts/ci/cargo-plugin-versions.sh\n"
    '          # echo "installing cargo-audit ${CARGO_AUDIT_VERSION}"\n'
    '          # cargo install --locked --version "${CARGO_AUDIT_VERSION}" cargo-audit\n'
    '          # assert_cargo_plugin_version cargo-audit "${CARGO_AUDIT_VERSION}"\n'
    "          cargo install --locked cargo-audit\n"
)
new, n = pat.subn(repl, text, count=1)
if n != 1:
    raise SystemExit(f"could not rewrite install block for ghost mutation (n={n})")
dst.write_text(new)
PY
install_ghost="$(install_cargo_audit_run "${ghost}")"
grep -qE '^[[:space:]]*cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+cargo-audit[[:space:]]*$' \
  <<<"${install_ghost}" \
  || fail "ghost mutation did not leave an active unpinned cargo install"
if active_source_line "${install_ghost}"; then
  fail "ghost mutation still leaves a complete active source line"
fi
if active_pinned_install_line "${install_ghost}"; then
  fail "ghost mutation still leaves a complete pinned install line"
fi
if active_assert_line "${install_ghost}"; then
  fail "ghost mutation still leaves a complete active assert line"
fi
# Comment text must still contain the ghosts — otherwise we tested a different defect.
grep -q 'source ./scripts/ci/cargo-plugin-versions.sh' "${ghost}" \
  || fail "ghost mutation did not retain commented source"
grep -q 'assert_cargo_plugin_version cargo-audit' "${ghost}" \
  || fail "ghost mutation did not retain commented assert"
if ( check_workflow "${ghost}" ) >/dev/null 2>&1; then
  fail "comment-ghost pin left the contract green"
fi
ok "comment-ghost pin + active unpinned install turns the contract red"

echo "== mutation: trailing inline # ghosts + active unpinned install =="
# Pin markers survive only as trailing comments on otherwise-active lines (#2317 review 4914059003).
inline_ghost="$(mktemp "${SANDBOX_ROOT}/inline.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${inline_ghost}" <<'PY'
import pathlib, re, sys

src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
pat = re.compile(
    r"(      - name: Install cargo-audit\n"
    r"(?:        #.*\n)*"
    r"        run: \|[-+]?\n)"
    r"(?:          .*\n)+"
)
repl = (
    "\\1"
    "          set -euo pipefail\n"
    "          true # source ./scripts/ci/cargo-plugin-versions.sh\n"
    '          cargo install --locked cargo-audit # --version "${CARGO_AUDIT_VERSION}"\n'
    '          true # assert_cargo_plugin_version cargo-audit "${CARGO_AUDIT_VERSION}"\n'
)
new, n = pat.subn(repl, text, count=1)
if n != 1:
    raise SystemExit(f"could not rewrite install block for inline-ghost mutation (n={n})")
dst.write_text(new)
PY
install_inline="$(install_cargo_audit_run "${inline_ghost}")"
grep -qF 'source ./scripts/ci/cargo-plugin-versions.sh' <<<"${install_inline}" \
  || fail "inline-ghost mutation did not retain trailing source text on an emitted line"
# Needle is the literal workflow text `--version "${CARGO_AUDIT_VERSION}"`, not an expanded pin.
# shellcheck disable=SC2016
grep -qF -- '--version "${CARGO_AUDIT_VERSION}"' <<<"${install_inline}" \
  || fail "inline-ghost mutation did not retain trailing --version text on an emitted line"
if active_source_line "${install_inline}"; then
  fail "inline-ghost mutation unexpectedly matches a complete active source line"
fi
if active_pinned_install_line "${install_inline}"; then
  fail "inline-ghost mutation unexpectedly matches a complete pinned install line"
fi
if active_assert_line "${install_inline}"; then
  fail "inline-ghost mutation unexpectedly matches a complete active assert line"
fi
if ( check_workflow "${inline_ghost}" ) >/dev/null 2>&1; then
  fail "trailing inline # ghosts left the contract green"
fi
ok "trailing inline # ghosts + active unpinned install turns the contract red"

echo "== regression: correctly pinned run: |- stays green =="
chomp="$(mktemp "${SANDBOX_ROOT}/chomp.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${chomp}" <<'PY'
import pathlib, re, sys

src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
pat = re.compile(
    r"(      - name: Install cargo-audit\n(?:        #.*\n)*)"
    r"        run: \|\n"
)
new, n = pat.subn(r"\1        run: |-\n", text, count=1)
if n != 1:
    raise SystemExit(f"could not switch Install cargo-audit to run: |- (n={n})")
dst.write_text(new)
PY
grep -qE '^[[:space:]]*run:[[:space:]]*\|-[[:space:]]*$' <<<"$(deps_security_job "${chomp}")" \
  || fail "run: |- regression did not apply"
if ! ( check_workflow "${chomp}" ) >/dev/null 2>&1; then
  fail "correctly pinned run: |- turned the contract red"
fi
ok "correctly pinned run: |- stays green"

echo "== regression: correctly pinned run: |+ stays green =="
keep="$(mktemp "${SANDBOX_ROOT}/keep.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${keep}" <<'PY'
import pathlib, re, sys

src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
pat = re.compile(
    r"(      - name: Install cargo-audit\n(?:        #.*\n)*)"
    r"        run: \|\n"
)
new, n = pat.subn(r"\1        run: |+\n", text, count=1)
if n != 1:
    raise SystemExit(f"could not switch Install cargo-audit to run: |+ (n={n})")
dst.write_text(new)
PY
grep -qE '^[[:space:]]*run:[[:space:]]*\|\+[[:space:]]*$' <<<"$(deps_security_job "${keep}")" \
  || fail "run: |+ regression did not apply"
if ! ( check_workflow "${keep}" ) >/dev/null 2>&1; then
  fail "correctly pinned run: |+ turned the contract red"
fi
ok "correctly pinned run: |+ stays green"

echo "== mutation: second active unpinned cargo-audit install =="
# Keep source + pinned install + assert, then append an unpinned install that would overwrite
# the pin at runtime while a presence-only check stayed green (#2317 review 4914399777).
dual="$(mktemp "${SANDBOX_ROOT}/dual.XXXXXX.yml")"
python3 - "${WORKFLOW}" "${dual}" <<'PY'
import pathlib, sys

src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
text = src.read_text()
old = '          assert_cargo_plugin_version cargo-audit "${CARGO_AUDIT_VERSION}"\n'
new = old + "          cargo install --locked cargo-audit\n"
if old not in text:
    raise SystemExit("could not find assert line to append a second install after")
dst.write_text(text.replace(old, new, 1))
PY
dual_run="$(install_cargo_audit_run "${dual}")"
[[ "$(count_active_cargo_audit_installs "${dual_run}")" -eq 2 ]] \
  || fail "dual-install mutation did not leave two active cargo-audit installs"
active_pinned_install_line "${dual_run}" \
  || fail "dual-install mutation lost the pinned install line"
active_source_line "${dual_run}" \
  || fail "dual-install mutation lost the source line"
active_assert_line "${dual_run}" \
  || fail "dual-install mutation lost the assert line"
grep -qE '^[[:space:]]*cargo[[:space:]]+install[[:space:]]+--locked[[:space:]]+cargo-audit[[:space:]]*$' \
  <<<"${dual_run}" \
  || fail "dual-install mutation did not append an active unpinned install"
if ( check_workflow "${dual}" ) >/dev/null 2>&1; then
  fail "second active unpinned cargo-audit install left the contract green"
fi
ok "second active unpinned cargo-audit install turns the contract red"

ok "cargo-plugin-versions contract mutations bite"
echo "PASS: cargo-plugin-versions contract"
