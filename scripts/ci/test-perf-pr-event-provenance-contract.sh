#!/usr/bin/env bash
# Contract for perf_pr.yml event-commit provenance on non-closed PR events (CI-5D / #2326).
#
# A delayed non-closed event after squash-merge can leave the symbolic PR head ref gone while
# the event still names exact base/head SHAs. Default actions/checkout may then land on current
# main; git diff of the event SHAs fails. This contract pins explicit head checkout, object
# proofs, HEAD identity, fail-closed diagnostics, and a byte-identical closed archive job.
#
# Literal workflow needles intentionally keep their dollar signs unexpanded.
# shellcheck disable=SC2016
set -euo pipefail

# Git hooks export repository-local state. This test creates disposable repositories, so carrying
# that state across the boundary can make fixture commands mutate the caller's shared .git config.
# Keep this static rather than asking `git rev-parse --local-env-vars`: invoking Git is unsafe until
# the inherited repository selection has been removed.
unset GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_CONFIG GIT_CONFIG_PARAMETERS GIT_CONFIG_COUNT \
  GIT_OBJECT_DIRECTORY GIT_DIR GIT_WORK_TREE GIT_IMPLICIT_WORK_TREE GIT_GRAFT_FILE \
  GIT_INDEX_FILE GIT_NO_REPLACE_OBJECTS GIT_REPLACE_REF_BASE GIT_PREFIX GIT_SHALLOW_FILE \
  GIT_COMMON_DIR

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${WORKFLOW:-${ROOT}/.github/workflows/perf_pr.yml}"
# archive_pr_branch job body on origin/main 1137af34e9ca4f7f1655fc422ff6b0e441a3e066
ARCHIVE_SHA256_PIN="1d82054d8ccba584450348453d12a027a819a13394323bb44aeab457ca482872"
EVENT_HEAD_REF_LINE='ref: ${{ github.event.pull_request.head.sha }}'

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f "$WORKFLOW" ]] || fail "missing ${WORKFLOW#"$ROOT"/}"

SANDBOX_ROOT="$(mktemp -d)"
trap 'rm -rf "$SANDBOX_ROOT"' EXIT

archive_job_sha256() {
  local wf="$1"
  python3 - "$wf" <<'PY'
import hashlib
import re
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
match = re.search(r"(?ms)^  archive_pr_branch:\n.*\Z", text)
if not match:
    raise SystemExit("missing archive_pr_branch job")
sys.stdout.write(hashlib.sha256(match.group(0).encode("utf-8")).hexdigest())
PY
}

detect_checkout_with_block() {
  local wf="$1"
  python3 - "$wf" <<'PY'
import re
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
job = re.search(
    r"(?ms)^  detect-benchmark-relevance:\n(.*?)(?=^  [A-Za-z]|\Z)",
    text,
)
if not job:
    raise SystemExit("missing detect-benchmark-relevance job")
block = job.group(1)
# First checkout step's with: map only.
m = re.search(
    r"(?ms)^      - uses: actions/checkout@[0-9a-f]+[^\n]*\n"
    r"        with:\n"
    r"((?:          [^\n]*\n)*)",
    block,
)
if not m:
    raise SystemExit("missing detect-benchmark-relevance checkout with: map")
sys.stdout.write(m.group(1))
PY
}

detect_run_script() {
  local wf="$1"
  python3 - "$wf" <<'PY'
import re
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
job = re.search(
    r"(?ms)^  detect-benchmark-relevance:\n(.*?)(?=^  [A-Za-z]|\Z)",
    text,
)
if not job:
    raise SystemExit("missing detect-benchmark-relevance job")
step = re.search(
    r"(?ms)^      - name: Detect benchmark-relevant paths\n"
    r"(.*?)(?=^      - |\Z)",
    job.group(1),
)
if not step:
    raise SystemExit("missing Detect benchmark-relevant paths step")
run = re.search(r"(?ms)^        run: \|\n((?:          .*?\n)*)", step.group(1))
if not run:
    raise SystemExit("missing detect run: block")
for line in run.group(1).splitlines(keepends=True):
    if line.startswith("          "):
        sys.stdout.write(line[10:])
    else:
        sys.stdout.write(line)
PY
}

checkout_pins_event_head() {
  local with_block="$1"
  grep -Fq "$EVENT_HEAD_REF_LINE" <<<"$with_block"
}

checkout_uses_main_or_default() {
  local with_block="$1"
  # Default = no ref key. Explicit main/github.sha/github.ref are also forbidden substitutes.
  if ! grep -Eq '^[[:space:]]*ref:' <<<"$with_block"; then
    return 0
  fi
  grep -Eq '^[[:space:]]*ref:[[:space:]]*(main|\$\{\{\s*github\.sha\s*\}\}|\$\{\{\s*github\.ref\s*\}\})' \
    <<<"$with_block"
}

run_has_fatal_base_object_check() {
  local script="$1"
  grep -Fq 'ensure_event_commit "${BASE_SHA}" "base"' <<<"$script" \
    && grep -Eq 'cat-file -e "\$\{sha\}\^\{commit\}"' <<<"$script" \
    && grep -Eq 'exit 1' <<<"$script" \
    && ! grep -Eq 'cat-file -e "\$\{sha\}[^"]*"[[:space:]]*\|\|[[:space:]]*true' <<<"$script"
}

run_has_fatal_head_object_check() {
  local script="$1"
  grep -Fq 'ensure_event_commit "${HEAD_SHA}" "head"' <<<"$script" \
    && grep -Eq 'cat-file -e "\$\{sha\}\^\{commit\}"' <<<"$script" \
    && grep -Eq 'exit 1' <<<"$script"
}

run_has_head_identity_assert() {
  local script="$1"
  grep -Fq 'actual_head="$(git rev-parse HEAD)"' <<<"$script" \
    && grep -Fq 'if [[ "${actual_head}" != "${HEAD_SHA}" ]]; then' <<<"$script" \
    && grep -Fq 'exit 1' <<<"$script"
}

run_fetch_failures_are_fatal() {
  local script="$1"
  # Any fetch of event SHAs must not be soft-pedaled with || true.
  if grep -Eq 'git fetch[^\n]*\$\{(BASE|HEAD)_SHA\}[^\n]*\|\|[[:space:]]*true' <<<"$script"; then
    return 1
  fi
  if grep -Eq 'git fetch[^\n]*\$(BASE|HEAD)_SHA[^\n]*\|\|[[:space:]]*true' <<<"$script"; then
    return 1
  fi
  return 0
}

build_delayed_event_fixture() {
  local origin="$1"
  local stamp="$2"
  git init --bare "$origin" >/dev/null
  local seed
  seed="$(mktemp -d "${SANDBOX_ROOT}/seed.XXXXXX")"
  git -C "$seed" init -q
  git -C "$seed" config user.email "ci5d@example.com"
  git -C "$seed" config user.name "ci5d"
  git -C "$seed" checkout -q -b main
  echo "base ${stamp}" >"$seed/README"
  git -C "$seed" add README
  git -C "$seed" commit -q -m "base"
  BASE_SHA="$(git -C "$seed" rev-parse HEAD)"
  git -C "$seed" checkout -q -b "pr-head-${stamp}"
  echo "head ${stamp}" >"$seed/change.txt"
  git -C "$seed" add change.txt
  git -C "$seed" commit -q -m "pr head"
  HEAD_SHA="$(git -C "$seed" rev-parse HEAD)"
  # Squash-merge simulation: main advances without the PR tip commit.
  git -C "$seed" checkout -q main
  echo "squashed ${stamp}" >"$seed/change.txt"
  git -C "$seed" add change.txt
  git -C "$seed" commit -q -m "squash merge on main"
  MAIN_SHA="$(git -C "$seed" rev-parse HEAD)"
  git -C "$seed" push -q "$origin" main
  # Publish exact event commits as fetchable objects without a symbolic PR head ref.
  git -C "$seed" push -q "$origin" "${BASE_SHA}:refs/ci5d/base-${stamp}"
  git -C "$seed" push -q "$origin" "${HEAD_SHA}:refs/ci5d/head-${stamp}"
  # No refs/heads/pr-head*, no pull/head — the delayed-event shape.
  rm -rf "$seed"
}

simulate_checkout_from_workflow() {
  local wf="$1" workspace="$2" origin="$3" head_sha="$4"
  local with_block
  with_block="$(detect_checkout_with_block "$wf")"
  git clone -q "$origin" "$workspace"
  git -C "$workspace" checkout -q main
  CHECKOUT_PRE_HEAD="$(git -C "$workspace" rev-parse HEAD)"
  CHECKOUT_RC=0
  CHECKOUT_LOG="${SANDBOX_ROOT}/checkout-$(basename "$workspace").log"
  : >"$CHECKOUT_LOG"
  # Symbolic PR head is absent (clone only has main).
  if git -C "$workspace" show-ref --verify --quiet "refs/heads/pr-head" 2>/dev/null; then
    fail "fixture leaked a symbolic PR head ref"
  fi
  if checkout_pins_event_head "$with_block"; then
    # Explicit event-head checkout: fetch the exact SHA (ref may be absent).
    # On failure, leave HEAD where it was — never fall back to main as a substitute.
    # Non-negated if/else: set -e is suppressed in the condition, and $? in else is the
    # real git status (unlike `if !` or a bare failing command under set -e).
    if git -C "$workspace" fetch --no-tags origin "$head_sha" >"$CHECKOUT_LOG" 2>&1; then
      CHECKOUT_RC=0
    else
      CHECKOUT_RC=$?
      return 0
    fi
    if git -C "$workspace" checkout --detach "$head_sha" >>"$CHECKOUT_LOG" 2>&1; then
      CHECKOUT_RC=0
    else
      CHECKOUT_RC=$?
      return 0
    fi
  fi
  # else: remain on current main — the measured failure mode without an explicit ref pin.
}

install_relevance_sentinel() {
  local workspace="$1" sentinel="$2"
  mkdir -p "$workspace/scripts/ci"
  rm -f "$sentinel"
  # Stub relevance: records that it ran. Provenance must fail closed before this executes.
  cat >"$workspace/scripts/ci/perf_bench_relevance.py" <<'PY'
from pathlib import Path
import os
import sys

Path(os.environ["RELEVANCE_SENTINEL"]).write_text("ran\n", encoding="utf-8")
sys.stdout.write("store_relevant=false\nsuite_relevant=false\n")
PY
  if [[ ! -f "$workspace/Cargo.toml" ]]; then
    printf '[workspace]\nmembers = []\n' >"$workspace/Cargo.toml"
  fi
}

run_detect_script_in_workspace() {
  local wf="$1" workspace="$2" base_sha="$3" head_sha="$4"
  local script_path out err rc=0
  script_path="${SANDBOX_ROOT}/detect-step.sh"
  detect_run_script "$wf" >"$script_path"
  local sentinel="${SANDBOX_ROOT}/relevance.ran"
  install_relevance_sentinel "$workspace" "$sentinel"
  out="${SANDBOX_ROOT}/detect.out"
  err="${SANDBOX_ROOT}/detect.err"
  (
    cd "$workspace"
    export BASE_SHA="$base_sha" HEAD_SHA="$head_sha"
    export RELEVANCE_SENTINEL="$sentinel"
    export GITHUB_OUTPUT="${SANDBOX_ROOT}/github.output"
    export GITHUB_STEP_SUMMARY="${SANDBOX_ROOT}/github.summary"
    : >"$GITHUB_OUTPUT"
    : >"$GITHUB_STEP_SUMMARY"
    bash "$script_path"
  ) >"$out" 2>"$err" || rc=$?
  DETECT_RC="$rc"
  DETECT_OUT="$out"
  DETECT_ERR="$err"
  DETECT_RELEVANCE_SENTINEL="$sentinel"
}

assert_fail_closed_before_relevance() {
  local kind="$1" needle="$2"
  local log
  # Capture first: under pipefail, `cat | grep -q` can SIGPIPE cat on match and false-reject.
  log="$(cat "$DETECT_OUT" "$DETECT_ERR")"
  [[ "$DETECT_RC" -ne 0 ]] \
    || fail "unreachable ${kind} must exit non-zero; rc=${DETECT_RC}\n${log}"
  [[ ! -e "$DETECT_RELEVANCE_SENTINEL" ]] \
    || fail "unreachable ${kind} must fail before relevance runs (sentinel ${DETECT_RELEVANCE_SENTINEL} exists)"
  grep -Eq "$needle" <<<"$log" \
    || fail "unreachable ${kind} must emit actionable fail-closed diagnostic matching /${needle}/; got:\n${log}"
  grep -Eq 'fail closed|refusing to substitute main' <<<"$log" \
    || fail "unreachable ${kind} diagnostic must be fail-closed; got:\n${log}"
}

# Behavioral negatives:
# - unreachable BASE: detect-step fail-closed before relevance
# - unreachable HEAD: checkout pin fails (detect never runs); no main substitute
assert_unreachable_event_sha_negatives() {
  local wf="$1"
  local stamp origin workspace base_sha head_sha main_sha bogus checkout_head checkout_log
  stamp="$(python3 - <<'PY'
import secrets
print(secrets.token_hex(4))
PY
)"
  origin="${SANDBOX_ROOT}/origin-neg-${stamp}.git"
  build_delayed_event_fixture "$origin" "$stamp"
  base_sha="$(git --git-dir="$origin" rev-parse "refs/ci5d/base-${stamp}")"
  head_sha="$(git --git-dir="$origin" rev-parse "refs/ci5d/head-${stamp}")"
  main_sha="$(git --git-dir="$origin" rev-parse refs/heads/main)"
  bogus="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  git --git-dir="$origin" cat-file -e "${bogus}^{commit}" 2>/dev/null \
    && fail "bogus SHA unexpectedly exists in origin"
  [[ "$bogus" != "$main_sha" ]] || fail "bogus SHA collided with main tip"

  # Negative 1: unreachable BASE_SHA — detect-step runtime (event head is checked out).
  workspace="${SANDBOX_ROOT}/ws-neg-base-${stamp}"
  simulate_checkout_from_workflow "$wf" "$workspace" "$origin" "$head_sha"
  [[ "$CHECKOUT_RC" -eq 0 ]] \
    || fail "reachable event head checkout must succeed for the base-negative fixture; \
log:\n$(cat "$CHECKOUT_LOG")"
  [[ "$(git -C "$workspace" rev-parse HEAD)" == "$head_sha" ]] \
    || fail "negative base fixture requires detached event head checkout"
  run_detect_script_in_workspace "$wf" "$workspace" "$bogus" "$head_sha"
  assert_fail_closed_before_relevance "BASE_SHA" \
    'exact event base commit|exact event.*base'

  # Negative 2: unreachable HEAD_SHA fails at explicit checkout (before Detect).
  # Do not claim the detect step handles a head that could never be checked out.
  workspace="${SANDBOX_ROOT}/ws-neg-head-${stamp}"
  simulate_checkout_from_workflow "$wf" "$workspace" "$origin" "$bogus"
  [[ "$CHECKOUT_RC" -ne 0 ]] \
    || fail "unreachable event head must make explicit checkout exit non-zero"
  checkout_head="$(git -C "$workspace" rev-parse HEAD)"
  # Failed pin must not fall back to treating current main as a successful event-head checkout.
  [[ "$checkout_head" == "$CHECKOUT_PRE_HEAD" ]] \
    || fail "failed event-head checkout must not move HEAD to a substitute ref \
(pre=${CHECKOUT_PRE_HEAD}, now=${checkout_head})"
  # Remaining on the pre-checkout tip is the failed state, not a successful main substitute.
  [[ "$CHECKOUT_RC" -ne 0 ]]
  checkout_log="$(cat "$CHECKOUT_LOG")"
  grep -Fq "$bogus" <<<"$checkout_log" \
    || fail "unreachable event-head checkout log must name the exact head SHA; got:\n${checkout_log}"
  grep -Ei 'fatal|not our ref|could not|does not exist|unknown revision|rejected|upload-pack' \
    <<<"$checkout_log" \
    || fail "unreachable event-head checkout must emit an actionable fetch/ref failure; got:\n${checkout_log}"
}

check_workflow() {
  local wf="$1"
  local with_block script archive_sha

  archive_sha="$(archive_job_sha256 "$wf")"
  [[ "$archive_sha" == "$ARCHIVE_SHA256_PIN" ]] \
    || fail "archive_pr_branch job must remain byte-for-byte unchanged \
(pin=${ARCHIVE_SHA256_PIN}, got=${archive_sha})"

  with_block="$(detect_checkout_with_block "$wf")"
  checkout_pins_event_head "$with_block" \
    || fail "detect-benchmark-relevance checkout must set \`${EVENT_HEAD_REF_LINE}\`"
  if checkout_uses_main_or_default "$with_block"; then
    fail "detect-benchmark-relevance checkout must not use main/default in place of the event head"
  fi
  grep -Eq '^[[:space:]]*fetch-depth:[[:space:]]*0[[:space:]]*$' <<<"$with_block" \
    || fail "detect-benchmark-relevance checkout must keep fetch-depth: 0"

  script="$(detect_run_script "$wf")"
  grep -Fq 'set -euo pipefail' <<<"$script" \
    || fail "detect step must keep set -euo pipefail"
  run_has_head_identity_assert "$script" \
    || fail "detect step must assert checked-out HEAD equals event HEAD_SHA and fail closed"
  run_has_fatal_base_object_check "$script" \
    || fail "detect step must prove exact event BASE_SHA commit object and fail closed if missing"
  run_has_fatal_head_object_check "$script" \
    || fail "detect step must prove exact event HEAD_SHA commit object and fail closed if missing"
  run_fetch_failures_are_fatal "$script" \
    || fail "detect step must not make event SHA fetch/check failures non-fatal"
  grep -Fq 'could not be fetched from origin; fail closed (will not substitute main)' \
    <<<"$script" \
    || fail "detect step must keep the fatal fetch-failure diagnostic for exact event commits"
  grep -Fq 'still missing after fetch; fail closed (will not substitute main)' \
    <<<"$script" \
    || fail "detect step must keep the fatal post-fetch object diagnostic for exact event commits"
  grep -Fq 'git diff --name-only "${BASE_SHA}...${HEAD_SHA}"' <<<"$script" \
    || fail "detect step must still diff the exact event BASE_SHA...HEAD_SHA range"
  # Never substitute main into the diff range.
  if grep -Eq 'git diff[^\n]*\bmain\b' <<<"$script"; then
    fail "detect step must not substitute main into the event diff"
  fi

  # Behavioral: delayed non-closed event — symbolic PR head absent; exact SHAs fetchable.
  local stamp origin workspace
  stamp="$(python3 - <<'PY'
import secrets
print(secrets.token_hex(4))
PY
)"
  origin="${SANDBOX_ROOT}/origin-${stamp}.git"
  build_delayed_event_fixture "$origin" "$stamp"
  # BASE_SHA/HEAD_SHA/MAIN_SHA set by build_delayed_event_fixture via globals below:
  # shellcheck disable=SC2034
  workspace="${SANDBOX_ROOT}/ws-${stamp}"
  # Re-read SHAs from origin notes we pushed.
  BASE_SHA="$(git --git-dir="$origin" rev-parse "refs/ci5d/base-${stamp}")"
  HEAD_SHA="$(git --git-dir="$origin" rev-parse "refs/ci5d/head-${stamp}")"
  MAIN_SHA="$(git --git-dir="$origin" rev-parse refs/heads/main)"
  [[ "$HEAD_SHA" != "$MAIN_SHA" ]] || fail "fixture head must differ from main tip"
  if git --git-dir="$origin" cat-file -e "${HEAD_SHA}^{commit}" \
    && git --git-dir="$origin" merge-base --is-ancestor "$HEAD_SHA" "$MAIN_SHA" 2>/dev/null; then
    fail "fixture head must not be on main (squash-merge shape)"
  fi

  simulate_checkout_from_workflow "$wf" "$workspace" "$origin" "$HEAD_SHA"
  [[ "$CHECKOUT_RC" -eq 0 ]] \
    || fail "positive delayed-event checkout of exact head must succeed; \
log:\n$(cat "$CHECKOUT_LOG")"
  run_detect_script_in_workspace "$wf" "$workspace" "$BASE_SHA" "$HEAD_SHA"
  [[ "$DETECT_RC" -eq 0 ]] \
    || fail "delayed-event provenance should succeed when exact commits are fetchable; \
rc=${DETECT_RC}\nstdout:\n$(cat "$DETECT_OUT")\nstderr:\n$(cat "$DETECT_ERR")"
  [[ -e "$DETECT_RELEVANCE_SENTINEL" ]] \
    || fail "positive delayed-event path must reach relevance after provenance proofs"
  local actual
  actual="$(git -C "$workspace" rev-parse HEAD)"
  [[ "$actual" == "$HEAD_SHA" ]] \
    || fail "after detect, HEAD must be the event head (${HEAD_SHA}), got ${actual}"

  assert_unreachable_event_sha_negatives "$wf"
}

expect_rejected() {
  local name="$1"
  local mutated="${SANDBOX_ROOT}/${name}.yml"
  python3 - "$WORKFLOW" "$mutated" "$name" <<'PY'
import pathlib
import sys

source, destination, name = sys.argv[1:]
text = pathlib.Path(source).read_text(encoding="utf-8")
event_head = "          ref: ${{ github.event.pull_request.head.sha }}\n"
mutations = {
    "remove-explicit-ref": (
        event_head,
        "",
    ),
    "replace-ref-with-main": (
        event_head,
        "          ref: main\n",
    ),
    "remove-base-object-check": (
        '          ensure_event_commit "${BASE_SHA}" "base"\n',
        "",
    ),
    "remove-head-object-check": (
        '          ensure_event_commit "${HEAD_SHA}" "head"\n',
        "",
    ),
    "remove-head-identity": (
        '          actual_head="$(git rev-parse HEAD)"\n'
        '          if [[ "${actual_head}" != "${HEAD_SHA}" ]]; then\n'
        '            echo "::error::checked-out HEAD ${actual_head} is not event head ${HEAD_SHA}; '
        'refusing to substitute main or any other ref for perf relevance"\n'
        '            exit 1\n'
        '          fi\n',
        "",
    ),
    "nonfatal-fetch": (
        '              echo "::error::exact event ${label} commit ${sha} could not be fetched from origin; fail closed (will not substitute main)"\n'
        '              exit 1\n',
        '              echo "::warning::exact event ${label} commit ${sha} could not be fetched; continuing"\n',
    ),
    "change-archive-block": (
        '            echo "::notice::Branch \'$GITHUB_HEAD_REF\' not found in Bencher '
        '(no benchmarks ran). Skipping archive."\n',
        '            echo "::notice::Branch archive skipped (mutated)."\n',
    ),
}
old, new = mutations[name]
count = text.count(old)
if count != 1:
    raise SystemExit(f"mutation {name!r} anchor matched {count} times, expected once")
pathlib.Path(destination).write_text(text.replace(old, new, 1), encoding="utf-8")
PY

  if WORKFLOW="$mutated" bash "$0" --no-mutations >/dev/null 2>&1; then
    echo "FAIL: contract accepted mutation: $name" >&2
    exit 1
  fi
  echo "ok: mutation rejected: $name"
}

# ---------------------------------------------------------------------------
# Entry
# ---------------------------------------------------------------------------
MODE="${1:-}"
check_workflow "$WORKFLOW"
echo "ok: perf_pr event provenance contract (structural + delayed-event behavior)"

if [[ "$MODE" == "--no-mutations" ]]; then
  exit 0
fi

expect_rejected remove-explicit-ref
expect_rejected replace-ref-with-main
expect_rejected remove-base-object-check
expect_rejected remove-head-object-check
expect_rejected remove-head-identity
expect_rejected nonfatal-fetch
expect_rejected change-archive-block

echo "ok: perf_pr event provenance mutations all bite"
