#!/usr/bin/env bash
set -euo pipefail

# Contract assertions for the fuzz lane's lock-staleness guard.
#
# The guard wraps `cargo metadata --locked`, and a wrapper that names one cause for every failure
# is a diagnosis the command did not make. `--locked` fails on a stale lock, but equally on an
# unparsable manifest, an unavailable dependency, a registry or network fault, or a broken
# toolchain — and the first version of this wrapper reported all of them as "fuzz/Cargo.lock is
# stale" and told the reader to run `cargo update --workspace`. On a registry outage that advice
# is wrong and, followed, would rewrite a lock that was never the problem.
#
# So the wrapper must add exit-code discipline and nothing else: Cargo's own stderr stays visible
# and stays the diagnosis. These assertions pin that, because prose in a comment is what went
# stale last time.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/fuzz-smoke.yml"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f "$WORKFLOW" ]] || fail "missing ${WORKFLOW#"$ROOT"/}"

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
  #    Any `2>` form, not an enumeration of them: `2>/dev/null` and `2>&1` were listed, so
  #    `2>cargo-error.log` passed both this test and actionlint while putting the only real
  #    diagnosis in a file nobody reads.
  #
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
  #    Proved by running the step rather than by pattern-matching it. Matching text was wrong in
  #    both directions at once: a step-level `shell: bash {0} 2>cargo-error.log` redirects the whole
  #    script from outside the block a regex can see, and a comment saying `2>cargo-error.log`
  #    turned the check red while changing nothing. Each new operator spelling only moved the line.
  #
  #    So: pin the shell, then execute the extracted script against a `cargo` that fails the way
  #    `--locked` fails, and require the diagnosis to arrive on the step's own stderr.
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

  # `BASH_ENV` names a file bash sources before the script, and `--noprofile --norc` does not
  # suppress it -- confirmed by running a preamble that does `exec 2>file`, which empties the outer
  # stderr capture while leaving the script itself untouched. The harness below executes the script,
  # so it can never see a preamble the runner would have sourced first. Matched as an active mapping
  # key, one name, no env parsing: a line that is commented out sources nothing and stays green.
  local bash_env_line
  bash_env_line="$(grep -nE '^[[:space:]]*BASH_ENV[[:space:]]*:' "$wf" | head -1 || true)"
  [[ -z "$bash_env_line" ]] \
    || fail "this lane must not set \`BASH_ENV\`: bash sources it before the step's own script, so
a preamble can redirect the step's stderr without appearing in the script at all. Found:\n  \
$bash_env_line"

  local sandbox rc=0
  local sentinel="CARGO-STDERR-SENTINEL-4d1f9a"
  sandbox="$(mktemp -d "${SANDBOX_ROOT}/wf.XXXXXX")"
  mkdir -p "$sandbox/bin" "$sandbox/fuzz" "$sandbox/tmp"
  awk '{ sub(/^          /, ""); print }' <<<"$run_block" > "$sandbox/step.sh"

  # `cargo` fails only for the guard, the way a `--locked` violation does: a message on stderr and
  # a nonzero exit. Everything else succeeds, so a green run means the guard did its job.
  {
    echo '#!/usr/bin/env bash'
    echo 'if [[ "$*" == *"metadata --locked"* ]]; then'
    echo "  echo \"error: the lock file needs to be updated -- ${sentinel}\" >&2"
    echo '  exit 1'
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

  # `env -u BASH_ENV`: whatever the caller's environment carries must not decide whether this proof
  # holds. The workflow is checked for it above; here the harness itself is put beyond its reach.
  ( cd "$sandbox" && env -u BASH_ENV PATH="$sandbox/bin:$PATH" RUNNER_TEMP="$sandbox/tmp" \
      FUZZ_TOOLCHAIN="nightly-mock" RUNS=1 MAX_TOTAL_TIME=1 \
      bash step.sh ) >"$sandbox/out" 2>"$sandbox/err" || rc=$?

  [[ "$rc" -ne 0 ]] \
    || fail "the fuzz step exited 0 even though \`cargo metadata --locked\` failed"
  grep -q '::error::locked fuzz metadata validation failed' "$sandbox/out" \
    || fail "the guard's wrapper message never printed:\n$(cat "$sandbox/out")"
  grep -q "$sentinel" "$sandbox/err" \
    || fail "Cargo's stderr did not reach the step's stderr, so the only real diagnosis is \
invisible to a reviewer. Step stderr was:\n$(cat "$sandbox/err")"

  # 4. The failure must still be a failure — asserted on the guard's own `||` arm, not on the file.
  #    Searching the whole workflow passed on the seed-corpus check's unrelated `exit 1`, so
  #    removing only this guard's exit left the contract green. An assertion scoped wider than its
  #    subject reports on something else.
  grep -q 'exit 1' <<<"$guard_line" \
    || fail "the lock guard reports but does not exit nonzero:\n  $guard_line"

  # 5. The toolchain pin stays dated. A channel alias would hide both a break and its fix, which is
  #    exactly what happened when nightly-2026-07-24 began ICEing on tokio under sanitizer coverage.
  pin="$(grep -E '^\s*FUZZ_TOOLCHAIN:' "$wf" | head -1 | sed 's/.*: *//')"
  [[ "$pin" =~ ^nightly-[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] \
    || fail "FUZZ_TOOLCHAIN must be a dated nightly, got: ${pin:-<empty>}"
  PIN="$pin"

  # 6. Every local crate the fuzz graph resolves must trigger this lane.
  #
  #    Derived from `fuzz/Cargo.lock` rather than listed: a package with no `source` is a path
  #    dependency, so the set comes from the same file the lane pins. Listing them by hand is what
  #    left `assay-adapter-api`, `assay-canonical` and `assay-common` uncovered while
  #    `assay-core` and `assay-evidence` were named -- a change in any of the three could alter
  #    code, manifest or lock without this lane ever running, which is the staleness this whole
  #    branch exists to stop. Offline and cheap: no cargo invocation.
  #    Read from the `on.pull_request.paths` sequence itself, not from the file. A crate name
  #    appears in this workflow's own prose too, so a whole-file grep answered "covered" for
  #    `# - "crates/assay-common/**"` -- a commented-out entry that triggers nothing, and that
  #    actionlint passes as valid YAML. The extractor takes only `- ` items inside that one block.
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

  local locals
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

# Negative control: strip the guard's exit and nothing else. The seed-corpus check keeps its own
# `exit 1`, which is exactly the state that used to pass.
mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
sed 's|\(inspect the Cargo error above"\); exit 1; }|\1; }|' "$WORKFLOW" > "$mutant"
if ! grep -q 'exit 1' "$mutant"; then
  fail "the mutation removed every exit, so it does not isolate the guard"
fi
if ( check_workflow "$mutant" ) >/dev/null 2>&1; then
  fail "removing only the guard's exit left the contract green — the fail-closed assertion is not \
bound to the guard"
fi
echo "ok: removing only the guard's exit turns the contract red"

# Negative control: send stderr to a file. It is not `/dev/null` and not `2>&1`, so the enumerated
# form of this check passed it -- and actionlint passes it too, since the shell is valid. The
# diagnosis simply lands somewhere no reviewer looks.
redirect_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
sed 's|--format-version 1 >/dev/null )|--format-version 1 >/dev/null 2>cargo-error.log )|' \
  "$WORKFLOW" > "$redirect_mutant"
grep -q '2>cargo-error.log' "$redirect_mutant" \
  || fail "the redirect mutation did not apply, so it proves nothing"
if ( check_workflow "$redirect_mutant" ) >/dev/null 2>&1; then
  fail "sending Cargo's stderr to a file left the contract green — the check enumerates redirect \
forms instead of rejecting them"
fi
echo "ok: redirecting Cargo's stderr to a file turns the contract red"

# Negative control: drop one crate from the path filter. This is the state the branch shipped in --
# three of the five local crates were simply absent -- so the assertion that catches it has to be
# shown catching it.
paths_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
grep -v '"crates/assay-common/\*\*"' "$WORKFLOW" > "$paths_mutant"
if ( check_workflow "$paths_mutant" ) >/dev/null 2>&1; then
  fail "dropping a local crate from the path filter left the contract green — the coverage \
assertion is not bound to the lockfile"
fi
echo "ok: dropping a local crate from the path filter turns the contract red"

# Negative control: comment the entry out instead of deleting it. The line is still in the file and
# still says the crate's name, so a whole-file grep called it covered — while `pull_request.paths`
# no longer carries it and actionlint sees nothing wrong.
comment_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
sed 's|^      - "crates/assay-common/\*\*"|      # - "crates/assay-common/**"|' \
  "$WORKFLOW" > "$comment_mutant"
grep -q '^      # - "crates/assay-common/\*\*"' "$comment_mutant" \
  || fail "the comment mutation did not apply, so it proves nothing"
if ( check_workflow "$comment_mutant" ) >/dev/null 2>&1; then
  fail "commenting out a path entry left the contract green — the coverage assertion reads the \
file rather than the active \`pull_request.paths\` sequence"
fi
echo "ok: commenting out a path entry turns the contract red"

# Negative control: move the redirect off the guard line. `exec 2>` sets fd2 for the rest of the
# step, so Cargo's diagnosis is gone just the same -- but a check windowed on the guard's own two
# lines never sees it, and actionlint has no opinion. Same promised property, one scope wider.
exec_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
sed 's|^          # Assert the checked-in lock is current before fuzzing.|          exec 2>cargo-error.log\
&|' "$WORKFLOW" > "$exec_mutant"
grep -q '^          exec 2>cargo-error.log$' "$exec_mutant" \
  || fail "the exec-redirect mutation did not apply, so it proves nothing"
if ( check_workflow "$exec_mutant" ) >/dev/null 2>&1; then
  fail "an \`exec 2>\` earlier in the step left the contract green — the stderr check is \
windowed on the guard rather than on the step"
fi
echo "ok: an \`exec 2>\` earlier in the step turns the contract red"

# Negative control: `2<>` opens fd2 read/write on a file. It is a different operator, not a
# different target, so a check spelled `2>` never saw it -- and the step's stderr is just as gone.
readwrite_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
sed 's|^          # Assert the checked-in lock is current before fuzzing.|          exec 2<>cargo-error.log\
&|' "$WORKFLOW" > "$readwrite_mutant"
grep -q '^          exec 2<>cargo-error.log$' "$readwrite_mutant" \
  || fail "the read/write redirect mutation did not apply, so it proves nothing"
if ( check_workflow "$readwrite_mutant" ) >/dev/null 2>&1; then
  fail "an \`exec 2<>\` left the contract green — the stderr check matches one spelling rather \
than the redirection operators that hide fd2"
fi
echo "ok: an \`exec 2<>\` turns the contract red"

# Negative control: redirect from the step's `shell:` line. It is outside the run block entirely,
# so no amount of scanning the script can see it -- only pinning the shell can.
shell_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
awk '
  /^      - name: Fuzz smoke[[:space:]]*$/ { in_step=1 }
  in_step && /^        shell: bash$/ { print "        shell: bash {0} 2>cargo-error.log"; in_step=0; next }
  { print }
' "$WORKFLOW" > "$shell_mutant"
grep -q 'shell: bash {0} 2>cargo-error.log' "$shell_mutant" \
  || fail "the custom-shell mutation did not apply, so it proves nothing"
if ( check_workflow "$shell_mutant" ) >/dev/null 2>&1; then
  fail "a step-level \`shell:\` redirect left the contract green — the stderr proof does not \
cover how the script is invoked"
fi
echo "ok: a step-level \`shell:\` redirect turns the contract red"

# Positive control: a comment that merely names a redirect changes nothing and must stay green.
# The previous static check matched the text and went red on it, which is a false alarm on a line
# warning against the very thing the test exists to catch.
comment_ok="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
sed 's|^          # Assert the checked-in lock is current before fuzzing.|          # Never add 2>cargo-error.log here, and never set BASH_ENV: anything\
&|' "$WORKFLOW" > "$comment_ok"
grep -q '# Never add 2>cargo-error.log here' "$comment_ok" \
  || fail "the comment control did not apply, so it proves nothing"
if ! ( check_workflow "$comment_ok" ) >/dev/null 2>&1; then
  fail "a comment naming a redirect turned the contract red — the check reacts to text rather \
than to behaviour"
fi
echo "ok: a comment naming a redirect or BASH_ENV leaves the contract green"

# Negative control: set `BASH_ENV` on the step. The script is untouched, so extracting and running
# it proves nothing -- the runner would have sourced a preamble before the first line ever ran.
bash_env_mutant="$(mktemp "${SANDBOX_ROOT}/mut.XXXXXX")"
awk '
  /^          RUNS: / && !done { print "          BASH_ENV: .github/fuzz-preamble.sh"; done=1 }
  { print }
' "$WORKFLOW" > "$bash_env_mutant"
grep -q '^          BASH_ENV: ' "$bash_env_mutant" \
  || fail "the BASH_ENV mutation did not apply, so it proves nothing"
if ( check_workflow "$bash_env_mutant" ) >/dev/null 2>&1; then
  fail "a step-level \`BASH_ENV\` left the contract green — bash sources it before the script, \
which is where the harness starts looking"
fi
echo "ok: a step-level \`BASH_ENV\` turns the contract red"

echo "ok: the lock guard reports without diagnosing, keeps Cargo's stderr, and fails closed"
echo "ok: FUZZ_TOOLCHAIN is dated (${PIN})"
echo "PASS: fuzz lane contract"
