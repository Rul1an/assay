#!/usr/bin/env bash
set -euo pipefail

# Contract assertions for the fuzz lane's lock-staleness and executed-units guards.
#
# The lock guard wraps `cargo metadata --locked`, and a wrapper that names one cause for every failure
# is a diagnosis the command did not make. `--locked` fails on a stale lock, but equally on an
# unparsable manifest, an unavailable dependency, a registry or network fault, or a broken
# toolchain — and the first version of this wrapper reported all of them as "fuzz/Cargo.lock is
# stale" and told the reader to run `cargo update --workspace`. On a registry outage that advice
# is wrong and, followed, would rewrite a lock that was never the problem.
#
# So the wrapper must add exit-code discipline and nothing else: Cargo's own stderr stays visible
# and stays the diagnosis.
#
# The executed-units guard captures libFuzzer output and asserts that units were actually executed,
# failing if the run executed 0 units or emitted no summary line (-print_final_stats=1).

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/fuzz-smoke.yml"
CHECK_SCRIPT="${ROOT}/scripts/ci/check-fuzz-executed-units.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f "$WORKFLOW" ]] || fail "missing ${WORKFLOW#"$ROOT"/}"
[[ -f "$CHECK_SCRIPT" ]] || fail "missing ${CHECK_SCRIPT#"$ROOT"/}"
[[ -x "$CHECK_SCRIPT" ]] || fail "not executable: ${CHECK_SCRIPT#"$ROOT"/}"

# One root for every temporary the test builds -- mutant workflows and behavioural sandboxes alike.
# Each mutant run aborts partway through check_workflow by design, so a cleanup line placed after
# the assertions is never reached. A single trap owns this: `trap ... EXIT` *replaces* the previous
# handler rather than adding to it, so a chain of re-traps quietly drops whatever the earlier ones
# covered -- which is how the sandboxes were leaking one per failed mutant.
SANDBOX_ROOT="$(mktemp -d)"
trap 'rm -rf "$SANDBOX_ROOT"' EXIT

# Every assertion takes the workflow path, so the same logic can be run against a deliberately
# broken copy below. A control that lives only in someone's shell history is not a control.
check_workflow() {
  local wf="$1"

  # The guard is the `||` arm attached to the `cargo metadata --locked` invocation. Anchor on the
  # invocation *line number*, not on any line mentioning a lock: the seed-corpus check also emits
  # `::error::`, and the workflow's own comments name the command in prose — a plain grep picked a
  # comment block and every assertion below then reported on the wrong two lines.
  local guard_ln guard_block guard_line guard_msg pin
  guard_ln="$(grep -nE '^[^#]*cargo[^#]*metadata --locked' "$wf" | head -1 | cut -d: -f1)"
  [[ -n "$guard_ln" ]] || fail "no \`cargo metadata --locked\` invocation found"
  guard_block="$(sed -n "${guard_ln},$((guard_ln + 1))p" "$wf")"

  guard_line="$(grep '::error::' <<<"$guard_block" | head -1 || true)"
  [[ -n "$guard_line" ]] || fail "no error message attached to the \`cargo metadata --locked\` guard"
  guard_msg="${guard_line#*::error::}"

  # 1. The message must not name a cause the command did not establish.
  for claim in stale "cargo update" "out of date" outdated regenerate; do
    if grep -qi -- "$claim" <<<"$guard_msg"; then
      fail "the lock-guard message claims '$claim', but \`cargo metadata --locked\` fails for \
several unrelated reasons (bad manifest, unavailable dependency, registry or network fault, \
toolchain configuration). Report the failure and let Cargo's stderr say why:\n  $guard_msg"
    fi
  done

  # 2. It must point the reader at the real diagnosis rather than replacing it.
  grep -qiE 'cargo error|error above|output above' <<<"$guard_msg" \
    || fail "the lock-guard message should send the reader to Cargo's own error, got:\n  $guard_msg"

  # 3. Cargo's stderr must stay visible. Only stdout is noise here — the metadata JSON.
  #    Scoped to the entire `Fuzz smoke` run block, not to the guard's own two lines. fd2 is step
  #    state, not line state: `exec 2>cargo-error.log` anywhere above the guard redirects it just
  #    as effectively, and a two-line window cannot see that. The property being promised is that
  #    the step's stderr reaches the log, so the check has to cover the step.
  local run_block
  run_block="$(awk '
    /^      - name: Fuzz smoke[[:space:]]*$/ { in_step=1; next }
    in_step && /^      - / { in_step=0 }
    in_step && /^        run: \|[[:space:]]*$/ { in_run=1; next }
    in_run {
      if ($0 !~ /^          / && $0 !~ /^[[:space:]]*$/) { in_run=0 } else { print }
    }
  ' "$wf")"
  [[ -n "$run_block" ]] || fail "could not read the \`Fuzz smoke\` run block from ${wf##*/}"
  grep -qF 'metadata --locked' <<<"$run_block" \
    || fail "the extracted \`Fuzz smoke\` run block does not contain the guard, so scanning it \
proves nothing"

  local shell_line
  shell_line="$(awk '
    /^      - name: Fuzz smoke[[:space:]]*$/ { in_step=1; next }
    in_step && /^      - / { in_step=0 }
    in_step && /^        shell:/ { print; exit }
  ' "$wf")"
  [[ "$shell_line" == "        shell: bash" ]] \
    || fail "the fuzz step needs a plain \`shell: bash\`; a custom shell line redirects the whole \
script's stderr from outside the script, where nothing in the run block can see it. Got:\n  \
${shell_line:-<none>}"

  # `BASH_ENV` check
  local bash_env_line
  local bash_env_key=$'^[[:space:]]*("BASH_ENV"|\'BASH_ENV\'|BASH_ENV)[[:space:]]*:'
  bash_env_line="$(grep -nE "$bash_env_key" "$wf" | head -1 || true)"
  [[ -z "$bash_env_line" ]] \
    || fail "this lane must not set \`BASH_ENV\`: bash sources it before the step's own script, so \
a preamble can redirect the step's stderr without appearing in the script at all. Found:\n  \
$bash_env_line"

  # Reject GitHub expressions inside raw script block
  if grep -qF '${{' <<<"$run_block"; then
    fail "the \`Fuzz smoke\` run script uses a GitHub \`\${{ }}\` expression; pass the value \
through the step's \`env:\` instead, so the contract harness executes the same script"
  fi

  # 4. The failure must still be a failure — asserted on the guard's own `||` arm, not on the file.
  grep -q 'exit 1' <<<"$guard_line" \
    || fail "the lock guard reports but does not exit nonzero:\n  $guard_line"

  # 5. Check that the step redirects fuzz run output to a log without piping through tee/tail/head,
  #    prints the log, checks exit code, and invokes check-fuzz-executed-units.sh.
  grep -q 'scripts/ci/check-fuzz-executed-units.sh' <<<"$run_block" \
    || fail "the fuzz step does not invoke scripts/ci/check-fuzz-executed-units.sh"

  if grep -E '\|[[:space:]]*(tee|tail|head)' <<<"$run_block"; then
    fail "the fuzz step pipes through a filter (tee/tail/head), which can mask the exit code; \
redirect to a file instead"
  fi

  grep -qE 'cat[[:space:]]+"\$?\{?fuzz_log\}?"' <<<"$run_block" \
    || fail "the fuzz step must print the fuzz log via cat before checking or exiting"

  local sandbox rc=0
  local sentinel="CARGO-STDERR-SENTINEL-4d1f9a"
  sandbox="$(mktemp -d "${SANDBOX_ROOT}/wf.XXXXXX")"
  mkdir -p "$sandbox/bin" "$sandbox/fuzz" "$sandbox/tmp" "$sandbox/scripts/ci"
  awk '{ sub(/^          /, ""); print }' <<<"$run_block" > "$sandbox/step.sh"
  cp "$CHECK_SCRIPT" "$sandbox/scripts/ci/check-fuzz-executed-units.sh"
  chmod +x "$sandbox/scripts/ci/check-fuzz-executed-units.sh"

  {
    echo '#!/usr/bin/env bash'
    echo 'if [[ "$*" == *"metadata --locked"* ]]; then'
    echo '  if [[ "${MOCK_CARGO_SCENARIO:-}" == "lock_failure" ]]; then'
    echo "    echo \"error: the lock file needs to be updated -- ${sentinel}\" >&2"
    echo '    exit 1'
    echo '  fi'
    echo '  exit 0'
    echo 'fi'
    echo 'if [[ "$*" == *"fuzz run"* ]]; then'
    echo '  case "${MOCK_CARGO_SCENARIO:-}" in'
    echo '    fuzz_crash)'
    echo '      echo "FATAL: fuzz target crashed with SIGSEGV" >&2'
    echo '      exit 77'
    echo '      ;;'
    echo '    fuzz_zero_units)'
    echo '      echo "Running: 0 units"'
    echo '      echo "stat::number_of_executed_units: 0"'
    echo '      exit 0'
    echo '      ;;'
    echo '    fuzz_no_summary)'
    echo '      echo "Fuzzer started and hung"'
    echo '      exit 0'
    echo '      ;;'
    echo '    fuzz_success | *)'
    echo '      echo "Running: 20000 units"'
    echo '      echo "stat::number_of_executed_units: 20000"'
    echo '      echo "stat::average_exec_per_sec: 1428"'
    echo '      echo "stat::peak_rss_mb: 412"'
    echo '      exit 0'
    echo '      ;;'
    echo '  esac'
    echo 'fi'
    echo 'exit 0'
  } > "$sandbox/bin/cargo"
  printf '#!/usr/bin/env bash\necho "rustc 0.0.0-mock"\n' > "$sandbox/bin/rustc"
  chmod +x "$sandbox/bin/cargo" "$sandbox/bin/rustc"

  # The mocks only mean something if they are the ones that run.
  local tool resolved
  for tool in cargo rustc; do
    resolved="$(PATH="$sandbox/bin:$PATH" command -v "$tool" || true)"
    [[ "$resolved" == "$sandbox/bin/$tool" ]] \
      || fail "the sandbox PATH does not win for \`$tool\`: it resolved to ${resolved:-<none>}, so \
the run below would exercise the real toolchain instead of the mock"
  done

  # Scenario 1: lock validation failure
  ( cd "$sandbox" && env -u BASH_ENV PATH="$sandbox/bin:$PATH" RUNNER_TEMP="$sandbox/tmp" \
      FUZZ_TOOLCHAIN="nightly-mock" RUNS=1 MAX_TOTAL_TIME=1 TARGET="bundle_reader" \
      MOCK_CARGO_SCENARIO="lock_failure" \
      bash step.sh ) >"$sandbox/out.lock" 2>"$sandbox/err.lock" || rc=$?

  [[ "$rc" -ne 0 ]] \
    || fail "the fuzz step exited 0 even though \`cargo metadata --locked\` failed"
  grep -q '::error::locked fuzz metadata validation failed' "$sandbox/out.lock" \
    || fail "the guard's wrapper message never printed:\n$(cat "$sandbox/out.lock")"
  grep -q "$sentinel" "$sandbox/err.lock" \
    || fail "Cargo's stderr did not reach the step's stderr. Step stderr was:\n$(cat "$sandbox/err.lock")"

  # Scenario 2: fuzz run crash (must preserve exit code 77 and print log)
  rc=0
  ( cd "$sandbox" && env -u BASH_ENV PATH="$sandbox/bin:$PATH" RUNNER_TEMP="$sandbox/tmp" \
      FUZZ_TOOLCHAIN="nightly-mock" RUNS=1 MAX_TOTAL_TIME=1 TARGET="bundle_reader" \
      MOCK_CARGO_SCENARIO="fuzz_crash" \
      bash step.sh ) >"$sandbox/out.crash" 2>"$sandbox/err.crash" || rc=$?

  [[ "$rc" -eq 77 ]] \
    || fail "a fuzz crash did not preserve exit code 77 (got exit code ${rc})"
  grep -q 'FATAL: fuzz target crashed' "$sandbox/out.crash" \
    || fail "fuzz crash output was not displayed on stdout"

  # Scenario 3: fuzz run succeeds with 0 executed units (must fail with ::error::)
  rc=0
  ( cd "$sandbox" && env -u BASH_ENV PATH="$sandbox/bin:$PATH" RUNNER_TEMP="$sandbox/tmp" \
      FUZZ_TOOLCHAIN="nightly-mock" RUNS=1 MAX_TOTAL_TIME=1 TARGET="bundle_reader" \
      MOCK_CARGO_SCENARIO="fuzz_zero_units" \
      bash step.sh ) >"$sandbox/out.zero" 2>"$sandbox/err.zero" || rc=$?

  [[ "$rc" -ne 0 ]] \
    || fail "a fuzz run with 0 executed units unexpectedly exited 0"
  grep -q '::error::fuzz target bundle_reader executed 0 units' "$sandbox/err.zero" \
    || grep -q '::error::fuzz target bundle_reader executed 0 units' "$sandbox/out.zero" \
    || fail "0 executed units did not report expected ::error:: message:\n$(cat "$sandbox/out.zero" "$sandbox/err.zero")"

  # Scenario 4: fuzz run succeeds with missing summary (must fail with ::error::)
  rc=0
  ( cd "$sandbox" && env -u BASH_ENV PATH="$sandbox/bin:$PATH" RUNNER_TEMP="$sandbox/tmp" \
      FUZZ_TOOLCHAIN="nightly-mock" RUNS=1 MAX_TOTAL_TIME=1 TARGET="bundle_reader" \
      MOCK_CARGO_SCENARIO="fuzz_no_summary" \
      bash step.sh ) >"$sandbox/out.nosummary" 2>"$sandbox/err.nosummary" || rc=$?

  [[ "$rc" -ne 0 ]] \
    || fail "a fuzz run with no summary line unexpectedly exited 0"
  grep -q '::error::no stat::number_of_executed_units line found' "$sandbox/err.nosummary" \
    || grep -q '::error::no stat::number_of_executed_units line found' "$sandbox/out.nosummary" \
    || fail "missing summary line did not report expected ::error:: message:\n$(cat "$sandbox/out.nosummary" "$sandbox/err.nosummary")"

  # Scenario 5: fuzz run succeeds with valid units (must succeed)
  rc=0
  ( cd "$sandbox" && env -u BASH_ENV PATH="$sandbox/bin:$PATH" RUNNER_TEMP="$sandbox/tmp" \
      FUZZ_TOOLCHAIN="nightly-mock" RUNS=1 MAX_TOTAL_TIME=1 TARGET="bundle_reader" \
      MOCK_CARGO_SCENARIO="fuzz_success" \
      bash step.sh ) >"$sandbox/out.success" 2>"$sandbox/err.success" || rc=$?

  [[ "$rc" -eq 0 ]] \
    || fail "a valid fuzz run with 20000 units failed with exit code ${rc}:\n$(cat "$sandbox/out.success" "$sandbox/err.success")"
  grep -q 'ok: fuzz target bundle_reader executed 20000 units' "$sandbox/out.success" \
    || fail "successful fuzz run did not report success message:\n$(cat "$sandbox/out.success")"

  # 6. The toolchain pin stays dated.
  pin="$(grep -E '^\s*FUZZ_TOOLCHAIN:' "$wf" | head -1 | sed 's/.*: *//')"
  [[ "$pin" =~ ^nightly-[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] \
    || fail "FUZZ_TOOLCHAIN must be a dated nightly, got: ${pin:-<empty>}"
  PIN="$pin"

  # 7. Every local crate the fuzz graph resolves must trigger this lane.
  local paths_active locals missing=""
  paths_active="$(awk '
    /^  pull_request:[[:space:]]*$/ { in_pr=1; next }
    /^  [^[:space:]]/              { in_pr=0; in_paths=0 }
    in_pr && /^    paths:[[:space:]]*$/ { in_paths=1; next }
    in_pr && /^    [^[:space:]]/  { in_paths=0 }
    in_paths && /^      -[[:space:]]/ { print }
  ' "$wf")"
  [[ -n "$paths_active" ]] \
    || fail "could not read the \`on.pull_request.paths\` sequence from ${wf##*/}"

  locals="$(awk '/^\[\[package\]\]/{name="";src=0} /^name = /{gsub(/[",]/,"",$3); name=$3} \
                 /^source = /{src=1} /^$/{if(name!="" && !src) print name} \
                 END{if(name!="" && !src) print name}' \
            "${ROOT}/fuzz/Cargo.lock" | grep -v '^assay-fuzz$' | sort -u)"
  [[ -n "$locals" ]] || fail "could not derive local path dependencies from fuzz/Cargo.lock"
  while read -r crate; do
    [[ -n "$crate" ]] || continue
    grep -qF "\"crates/${crate}/**\"" <<<"$paths_active" || missing="${missing} ${crate}"
  done <<<"$locals"
  [[ -z "$missing" ]] || fail "the fuzz lane resolves these local crates but does not trigger on \
them, so a change there can go untested:${missing}"
}

check_workflow "$WORKFLOW"

# --- Unit Controls for check-fuzz-executed-units.sh ---

# Negative control: log with 0 executed units
zero_summary="$(mktemp "${SANDBOX_ROOT}/zero.XXXXXX")"
cat <<'EOF' > "$zero_summary"
INFO: Running with entropic power schedule (0xFF, 100).
INFO: Seed: 123456789
stat::number_of_executed_units: 0
stat::average_exec_per_sec:     0
stat::new_units_added:          0
stat::slowest_unit_time_sec:    0
stat::peak_rss_mb:              120
EOF
rc=0
out="$(bash "$CHECK_SCRIPT" "$zero_summary" "bundle_reader" 2>&1)" || rc=$?
[[ "$rc" -ne 0 ]] || fail "check-fuzz-executed-units passed on 0 executed units"
grep -q '::error::fuzz target bundle_reader executed 0 units' <<<"$out" \
  || fail "check-fuzz-executed-units did not emit 0 units error message on 0 units, got:\n$out"
echo "ok: negative control on 0 executed units correctly fails"

# Negative control: log with no stat line (e.g. hung process or missing -print_final_stats)
missing_summary="$(mktemp "${SANDBOX_ROOT}/missing.XXXXXX")"
cat <<'EOF' > "$missing_summary"
INFO: Seed: 123456789
INFO: Loaded 14 seeds
# Process exited without final stats
EOF
rc=0
out="$(bash "$CHECK_SCRIPT" "$missing_summary" "bundle_reader" 2>&1)" || rc=$?
[[ "$rc" -ne 0 ]] || fail "check-fuzz-executed-units passed on missing summary line"
grep -q '::error::no stat::number_of_executed_units line found in fuzz output' <<<"$out" \
  || fail "check-fuzz-executed-units did not emit missing summary error message, got:\n$out"
echo "ok: negative control on missing summary line correctly fails"

# Negative control: log with malformed stat line
malformed_summary="$(mktemp "${SANDBOX_ROOT}/malformed.XXXXXX")"
cat <<'EOF' > "$malformed_summary"
stat::number_of_executed_units: not_a_number
EOF
rc=0
out="$(bash "$CHECK_SCRIPT" "$malformed_summary" "bundle_reader" 2>&1)" || rc=$?
[[ "$rc" -ne 0 ]] || fail "check-fuzz-executed-units passed on malformed summary line"
grep -q '::error::malformed stat::number_of_executed_units line' <<<"$out" \
  || fail "check-fuzz-executed-units did not emit malformed line error message, got:\n$out"
echo "ok: negative control on malformed stat line correctly fails"

# Positive control: real-shaped libFuzzer summary
# Exact format cited from LLVM libFuzzer compiler-rt/lib/fuzzer/FuzzerLoop.cpp PrintFinalStats():
# Printf("stat::number_of_executed_units: %zd\n", TotalNumberOfRuns);
# Printf("stat::average_exec_per_sec:     %zd\n", ExecsPerSec);
# Printf("stat::new_units_added:          %zd\n", NumberOfNewUnitsAdded);
# Printf("stat::slowest_unit_time_sec:    %zd\n", SlowestUnitStartTime);
# Printf("stat::peak_rss_mb:              %zd\n", GetPeakRSSMb());
real_summary="$(mktemp "${SANDBOX_ROOT}/real.XXXXXX")"
cat <<'EOF' > "$real_summary"
INFO: 20000 runs completed in 14 seconds.
stat::number_of_executed_units: 20000
stat::average_exec_per_sec:     1428
stat::new_units_added:          776
stat::slowest_unit_time_sec:    0
stat::peak_rss_mb:              412
EOF
rc=0
out="$(bash "$CHECK_SCRIPT" "$real_summary" "bundle_reader" 2>&1)" || rc=$?
[[ "$rc" -eq 0 ]] || fail "check-fuzz-executed-units failed on real libFuzzer summary, got:\n$out"
grep -q 'ok: fuzz target bundle_reader executed 20000 units' <<<"$out" \
  || fail "check-fuzz-executed-units did not print expected ok line on real summary, got:\n$out"
echo "ok: positive control on real libFuzzer summary passes"

# Positive control: multi-worker libFuzzer summary (summing per-worker outputs)
multi_summary="$(mktemp "${SANDBOX_ROOT}/multi.XXXXXX")"
cat <<'EOF' > "$multi_summary"
=== Worker 0 ===
stat::number_of_executed_units: 10000
stat::average_exec_per_sec:     1000
stat::peak_rss_mb:              200
=== Worker 1 ===
stat::number_of_executed_units: 15000
stat::average_exec_per_sec:     1500
stat::peak_rss_mb:              250
EOF
rc=0
out="$(bash "$CHECK_SCRIPT" "$multi_summary" "mcp_jsonrpc" 2>&1)" || rc=$?
[[ "$rc" -eq 0 ]] || fail "check-fuzz-executed-units failed on multi-worker summary, got:\n$out"
grep -q 'ok: fuzz target mcp_jsonrpc executed 25000 units' <<<"$out" \
  || fail "check-fuzz-executed-units did not sum multi-worker units correctly, got:\n$out"
echo "ok: positive control on multi-worker summary sums units correctly"

# --- Parser Mutation Controls (RED tests) ---

# Negative control mutant: bypass 0-units check in check-fuzz-executed-units.sh
zero_mutant_script="$(mktemp "${SANDBOX_ROOT}/zero_mut.XXXXXX")"
sed 's|if \[\[ "\$total_units" -le 0 \]\];* *then|if false; then|' "$CHECK_SCRIPT" > "$zero_mutant_script"
if ! grep -q 'if false; then' "$zero_mutant_script"; then
  fail "the 0-units mutation did not apply to check-fuzz-executed-units.sh"
fi
if ! bash "$zero_mutant_script" "$zero_summary" "bundle_reader" >/dev/null 2>&1; then
  fail "bypassing the 0-units check still caused a failure on 0 units"
fi
echo "ok: removing the 0-units check in the checker turns the 0-units control false-green (isolated)"

# Negative control mutant: bypass missing-summary check in check-fuzz-executed-units.sh
missing_mutant_script="$(mktemp "${SANDBOX_ROOT}/missing_mut.XXXXXX")"
sed 's|if \[\[ -z "\$matched_lines" \]\];* *then|if false; then|' "$CHECK_SCRIPT" > "$missing_mutant_script"
if ! grep -q 'if false; then' "$missing_mutant_script"; then
  fail "the missing-summary mutation did not apply to check-fuzz-executed-units.sh"
fi
mut_out="$(bash "$missing_mutant_script" "$missing_summary" "bundle_reader" 2>&1 || true)"
if grep -q '::error::no stat::number_of_executed_units line found in fuzz output' <<<"$mut_out"; then
  fail "bypassing the missing-summary check still emitted the missing-summary error"
fi
echo "ok: removing the missing-summary check fails to emit the distinct missing-summary error"

# --- Workflow Mutation Controls ---

# Negative control: strip the lock guard's exit and nothing else.
mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
sed 's|\(inspect the Cargo error above"\); exit 1; }|\1; }|' "$WORKFLOW" > "$mutant"
if ! grep -q 'exit 1' "$mutant"; then
  fail "the mutation removed every exit, so it does not isolate the guard"
fi
if ( check_workflow "$mutant" ) >/dev/null 2>&1; then
  fail "removing only the guard's exit left the contract green"
fi
echo "ok: removing only the guard's exit turns the contract red"

# Negative control: remove check-fuzz-executed-units.sh invocation from workflow
no_check_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
sed 's|bash scripts/ci/check-fuzz-executed-units.sh.*||' "$WORKFLOW" > "$no_check_mutant"
if ( check_workflow "$no_check_mutant" ) >/dev/null 2>&1; then
  fail "removing the check-fuzz-executed-units invocation left the contract green"
fi
echo "ok: removing check-fuzz-executed-units invocation turns the contract red"

# Negative control: pipe cargo fuzz run through tee in workflow
tee_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
sed 's|-print_final_stats=1 > "${fuzz_log}" 2>&1|-print_final_stats=1 2>\&1 \| tee "${fuzz_log}"|' "$WORKFLOW" > "$tee_mutant"
if ( check_workflow "$tee_mutant" ) >/dev/null 2>&1; then
  fail "piping cargo fuzz run through tee left the contract green"
fi
echo "ok: piping cargo fuzz run through tee turns the contract red"

# Negative control: swallow fuzz crash exit code in workflow
swallow_crash_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
sed 's|exit "${fuzz_status}"|# exit "${fuzz_status}"|' "$WORKFLOW" > "$swallow_crash_mutant"
grep -q '# exit "${fuzz_status}"' "$swallow_crash_mutant" \
  || fail "the swallow-crash mutation did not apply"
if ( check_workflow "$swallow_crash_mutant" ) >/dev/null 2>&1; then
  fail "swallowing fuzz crash exit code left the contract green"
fi
echo "ok: swallowing fuzz crash exit code turns the contract red"

# Negative control: send Cargo stderr to a file.
redirect_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
sed 's|--format-version 1 >/dev/null )|--format-version 1 >/dev/null 2>cargo-error.log )|' \
  "$WORKFLOW" > "$redirect_mutant"
grep -q '2>cargo-error.log' "$redirect_mutant" \
  || fail "the redirect mutation did not apply, so it proves nothing"
if ( check_workflow "$redirect_mutant" ) >/dev/null 2>&1; then
  fail "sending Cargo's stderr to a file left the contract green"
fi
echo "ok: redirecting Cargo's stderr to a file turns the contract red"

# Negative control: drop one crate from the path filter.
paths_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
grep -v '"crates/assay-common/\*\*"' "$WORKFLOW" > "$paths_mutant"
if ( check_workflow "$paths_mutant" ) >/dev/null 2>&1; then
  fail "dropping a local crate from the path filter left the contract green"
fi
echo "ok: dropping a local crate from the path filter turns the contract red"

# Negative control: comment the entry out instead of deleting it.
comment_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
sed 's|^      - "crates/assay-common/\*\*"|      # - "crates/assay-common/**"|' \
  "$WORKFLOW" > "$comment_mutant"
grep -q '^      # - "crates/assay-common/\*\*"' "$comment_mutant" \
  || fail "the comment mutation did not apply, so it proves nothing"
if ( check_workflow "$comment_mutant" ) >/dev/null 2>&1; then
  fail "commenting out a path entry left the contract green"
fi
echo "ok: commenting out a path entry turns the contract red"

# Negative control: move the redirect off the guard line.
exec_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
sed 's|^          # Assert the checked-in lock is current before fuzzing.|          exec 2>cargo-error.log\
&|' "$WORKFLOW" > "$exec_mutant"
grep -q '^          exec 2>cargo-error.log$' "$exec_mutant" \
  || fail "the exec-redirect mutation did not apply, so it proves nothing"
if ( check_workflow "$exec_mutant" ) >/dev/null 2>&1; then
  fail "an \`exec 2>\` earlier in the step left the contract green"
fi
echo "ok: an \`exec 2>\` earlier in the step turns the contract red"

# Negative control: `2<>` opens fd2 read/write on a file.
readwrite_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
sed 's|^          # Assert the checked-in lock is current before fuzzing.|          exec 2<>cargo-error.log\
&|' "$WORKFLOW" > "$readwrite_mutant"
grep -q '^          exec 2<>cargo-error.log$' "$readwrite_mutant" \
  || fail "the read/write redirect mutation did not apply, so it proves nothing"
if ( check_workflow "$readwrite_mutant" ) >/dev/null 2>&1; then
  fail "an \`exec 2<>\` left the contract green"
fi
echo "ok: an \`exec 2<>\` turns the contract red"

# Negative control: redirect from the step's `shell:` line.
shell_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
awk '
  /^      - name: Fuzz smoke[[:space:]]*$/ { in_step=1 }
  in_step && /^        shell: bash$/ { print "        shell: bash {0} 2>cargo-error.log"; in_step=0; next }
  { print }
' "$WORKFLOW" > "$shell_mutant"
grep -q 'shell: bash {0} 2>cargo-error.log' "$shell_mutant" \
  || fail "the custom-shell mutation did not apply, so it proves nothing"
if ( check_workflow "$shell_mutant" ) >/dev/null 2>&1; then
  fail "a step-level \`shell:\` redirect left the contract green"
fi
echo "ok: a step-level \`shell:\` redirect turns the contract red"

# Positive control: a comment that merely names a redirect changes nothing and must stay green.
comment_ok="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
sed 's|^          # Assert the checked-in lock is current before fuzzing.|          # Never add 2>cargo-error.log here, and never set BASH_ENV: anything\
&|' "$WORKFLOW" > "$comment_ok"
grep -q '# Never add 2>cargo-error.log here' "$comment_ok" \
  || fail "the comment control did not apply, so it proves nothing"
if ! ( check_workflow "$comment_ok" ) >/dev/null 2>&1; then
  fail "a comment naming a redirect turned the contract red"
fi
echo "ok: a comment naming a redirect or BASH_ENV leaves the contract green"

# Negative control: set `BASH_ENV` on the step.
bash_env_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
awk '
  /^          RUNS: / && !done { print "          BASH_ENV: .github/fuzz-preamble.sh"; done=1 }
  { print }
' "$WORKFLOW" > "$bash_env_mutant"
grep -q '^          BASH_ENV: ' "$bash_env_mutant" \
  || fail "the BASH_ENV mutation did not apply, so it proves nothing"
if ( check_workflow "$bash_env_mutant" ) >/dev/null 2>&1; then
  fail "a step-level \`BASH_ENV\` left the contract green"
fi
echo "ok: a step-level \`BASH_ENV\` turns the contract red"

# Negative control: quote the key.
quoted_env_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
awk '
  /^          RUNS: / && !done { print "          \"BASH_ENV\": .github/fuzz-preamble.sh"; done=1 }
  { print }
' "$WORKFLOW" > "$quoted_env_mutant"
grep -q '^          "BASH_ENV": ' "$quoted_env_mutant" \
  || fail "the quoted-key mutation did not apply, so it proves nothing"
if ( check_workflow "$quoted_env_mutant" ) >/dev/null 2>&1; then
  fail "a quoted \`\"BASH_ENV\"\` key left the contract green"
fi
echo "ok: a quoted \`BASH_ENV\` key turns the contract red"

# Negative control: use a GitHub expression in the run script instead of the step's `env:`.
expr_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
sed 's|^          case "${TARGET}" in|          case "${{ matrix.target }}" in|' \
  "$WORKFLOW" > "$expr_mutant"
grep -q 'matrix.target' "$expr_mutant" \
  || fail "the expression mutation did not apply, so it proves nothing"
if ( check_workflow "$expr_mutant" ) >/dev/null 2>&1; then
  fail "a GitHub expression in the run script left the contract green"
fi
echo "ok: a GitHub expression in the run script turns the contract red"

echo "ok: the lock guard reports without diagnosing, keeps Cargo's stderr, and fails closed"
echo "ok: the executed-units guard fails on 0 units and missing summary, preserves crashes, and sums multi-worker outputs"
echo "ok: FUZZ_TOOLCHAIN is dated (${PIN})"
echo "PASS: fuzz lane contract"
