#!/usr/bin/env bash
# Contract for scripts/ci/cancel-superseded-ci-runs.sh and its job in ci.yml.
#
# The behaviour cases run the script against a stub gh that serves a fixed run list and records
# every cancel call, so each case is judged by which run ids were cancelled and by exit status.
# The call-site check reads the parsed workflow and refuses neutralisations of a copy of it.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${ROOT}/scripts/ci/cancel-superseded-ci-runs.sh"
WORKFLOW="${ROOT}/.github/workflows/ci.yml"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

failures=0
pass() { printf 'PASS: %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; failures=$((failures + 1)); }

[[ -f "$SCRIPT" ]] || { printf 'FAIL: %s is missing\n' "${SCRIPT#"${ROOT}"/}" >&2; exit 1; }

SHA_NEW=1111111111111111111111111111111111111111
SHA_OLD=2222222222222222222222222222222222222222
REPO=Rul1an/assay
FORK=someone/assay

# One run list covers every case. Run 500 is the run doing the cancelling.
cat > "$TMP/runs.json" <<EOF
{"workflow_runs": [
  {"id": 410, "head_sha": "$SHA_OLD", "status": "in_progress", "head_repository": {"full_name": "$REPO"}, "pull_requests": [{"number": 7}]},
  {"id": 420, "head_sha": "$SHA_OLD", "status": "queued", "head_repository": {"full_name": "$REPO"}, "pull_requests": [{"number": 7}]},
  {"id": 430, "head_sha": "$SHA_OLD", "status": "completed", "head_repository": {"full_name": "$REPO"}, "pull_requests": [{"number": 7}]},
  {"id": 440, "head_sha": "$SHA_NEW", "status": "in_progress", "head_repository": {"full_name": "$REPO"}, "pull_requests": [{"number": 7}]},
  {"id": 450, "head_sha": "$SHA_OLD", "status": "in_progress", "head_repository": {"full_name": "$REPO"}, "pull_requests": [{"number": 8}]},
  {"id": 460, "head_sha": "$SHA_OLD", "status": "in_progress", "head_repository": {"full_name": "$FORK"}, "pull_requests": [{"number": 7}]},
  {"id": 470, "head_sha": "$SHA_OLD", "status": "waiting", "head_repository": {"full_name": "$REPO"}, "pull_requests": []},
  {"id": 600, "head_sha": "$SHA_OLD", "status": "in_progress", "head_repository": {"full_name": "$REPO"}, "pull_requests": [{"number": 7}]}
]}
EOF

mkdir -p "$TMP/bin"
cat > "$TMP/bin/gh" <<'EOF'
#!/usr/bin/env bash
# Stub: GET lists runs from $STUB_RUNS (or fails when STUB_LIST_FAIL=1); POST .../cancel records
# the id in $STUB_LOG (or fails for ids in STUB_CANCEL_FAIL). Every argv is appended to $STUB_ARGV.
printf '%s\n' "$*" >> "$STUB_ARGV"
if [[ "$1 $2 $3" == "api -X GET" ]]; then
  [[ "${STUB_LIST_FAIL:-0}" == 1 ]] && exit 1
  cat "$STUB_RUNS"; exit 0
fi
if [[ "$1 $2 $3" == "api -X POST" && "$4" =~ /actions/runs/([0-9]+)/cancel$ ]]; then
  id="${BASH_REMATCH[1]}"
  [[ " ${STUB_CANCEL_FAIL:-} " == *" $id "* ]] && exit 1
  echo "$id" >> "$STUB_LOG"; exit 0
fi
echo "unexpected gh call: $*" >&2; exit 3
EOF
chmod +x "$TMP/bin/gh"

run_script() {
  # run_script <log> [extra env...]: prints exit status to $TMP/rc
  local log="$1"; shift
  : > "$log"; : > "$TMP/argv"
  set +e
  env PATH="$TMP/bin:$PATH" STUB_RUNS="$TMP/runs.json" STUB_LOG="$log" STUB_ARGV="$TMP/argv" \
    REPO="$REPO" PR_NUMBER=7 HEAD_SHA="$SHA_NEW" HEAD_REF="feature/a&b=c" RUN_ID=500 "$@" \
    bash "$SCRIPT" > "$TMP/out" 2>&1
  echo $? > "$TMP/rc"
  set -e
}

# 1. Only the older, unfinished runs of this pull request on another head from this repository.
run_script "$TMP/c1"
if [[ "$(cat "$TMP/rc")" == 0 && "$(sort -n "$TMP/c1" | tr '\n' ' ')" == "410 420 " ]]; then
  pass "cancels exactly the older unfinished runs of this pull request (410, 420)"
else
  fail "expected exit 0 and cancels 410 420, got exit $(cat "$TMP/rc") and '$(tr '\n' ' ' < "$TMP/c1")'"
  sed 's/^/  | /' "$TMP/out" >&2
fi
for id in 430 440 450 460 470 600; do
  grep -qx "$id" "$TMP/c1" && fail "cancelled run $id, which must be left alone"
done
grep -qx 600 "$TMP/c1" || pass "a newer run (600) is never cancelled"

# 2. The branch reaches gh as a form field, not inside the URL.
if grep -q -- '-f branch=feature/a&b=c' "$TMP/argv" && ! grep -q 'runs?branch\|&b=c/' "$TMP/argv"; then
  pass "the branch name is passed as a form field"
else
  fail "the branch name must be passed with -f branch=, got: $(head -1 "$TMP/argv")"
fi

# 3. A refused cancel (read-only token on a fork) is reported, not fatal.
run_script "$TMP/c3" STUB_CANCEL_FAIL="410"
if [[ "$(cat "$TMP/rc")" == 0 ]] && grep -q 'could not cancel run 410' "$TMP/out" && grep -qx 420 "$TMP/c3"; then
  pass "a refused cancel is reported and the rest still go"
else
  fail "a refused cancel must be a notice with exit 0; got exit $(cat "$TMP/rc")"
  sed 's/^/  | /' "$TMP/out" >&2
fi

# 4. A run list that cannot be read cancels nothing and does not fail the run.
run_script "$TMP/c4" STUB_LIST_FAIL=1
if [[ "$(cat "$TMP/rc")" == 0 && ! -s "$TMP/c4" ]] && grep -q 'could not list' "$TMP/out"; then
  pass "an unreadable run list cancels nothing"
else
  fail "an unreadable run list must cancel nothing with exit 0; got exit $(cat "$TMP/rc")"
fi

# 5. Bad inputs are a usage error and cancel nothing.
for bad in "PR_NUMBER=0" "PR_NUMBER=7;rm" "RUN_ID=x" "HEAD_SHA=abc" "HEAD_REF=" "REPO=no-slash" "WORKFLOW=../x.yml"; do
  run_script "$TMP/c5" "$bad"
  if [[ "$(cat "$TMP/rc")" == 2 && ! -s "$TMP/c5" ]]; then
    pass "usage error for $bad"
  else
    fail "expected exit 2 and no cancels for $bad, got exit $(cat "$TMP/rc")"
  fi
done

# --- Call site --------------------------------------------------------------------------------
if ruby -ryaml - "$WORKFLOW" <<'RUBY'
path = ARGV.fetch(0)
doc = YAML.safe_load_file(path, aliases: false)
JOB = "cancel-superseded"
RUN = "bash scripts/ci/cancel-superseded-ci-runs.sh"
ENV_EXPECTED = {
  "GH_TOKEN" => "${{ github.token }}",
  "REPO" => "${{ github.repository }}",
  "PR_NUMBER" => "${{ github.event.pull_request.number }}",
  "HEAD_SHA" => "${{ github.event.pull_request.head.sha }}",
  "HEAD_REF" => "${{ github.event.pull_request.head.ref }}",
  "RUN_ID" => "${{ github.run_id }}",
}

def problems(doc)
  out = []
  jobs = doc.is_a?(Hash) ? doc["jobs"] : nil
  return ["ci.yml has no jobs"] unless jobs.is_a?(Hash)
  out << "ci.yml must not have a workflow-level concurrency group (#3653)" if doc.key?("concurrency")
  job = jobs[JOB]
  return out + ["ci.yml has no #{JOB} job"] unless job.is_a?(Hash)
  out << "#{JOB} must run only for pull_request" unless job["if"] == "github.event_name == 'pull_request'"
  out << "#{JOB} permissions must be exactly actions: write" unless job["permissions"] == { "actions" => "write" }
  out << "#{JOB} must not need other jobs" if job.key?("needs")
  out << "#{JOB} must not set concurrency" if job.key?("concurrency")
  out << "#{JOB} must not set continue-on-error" if job.key?("continue-on-error")
  steps = job["steps"]
  return out + ["#{JOB} has no steps"] unless steps.is_a?(Array)
  runs = steps.select { |s| s.is_a?(Hash) && s.key?("run") }
  out << "#{JOB} must have exactly one run step" unless runs.length == 1
  if runs.length == 1
    s = runs.first
    out << "#{JOB} must run exactly #{RUN}" unless s["run"] == RUN
    out << "#{JOB} env must be exactly the expected inputs" unless s["env"] == ENV_EXPECTED
    %w[if continue-on-error].each { |k| out << "#{JOB} run step must not set #{k}" if s.key?(k) }
  end
  checkout = steps.find { |s| s.is_a?(Hash) && s["uses"].to_s.start_with?("actions/checkout@") }
  out << "#{JOB} checkout must not persist credentials" unless checkout && checkout["with"].is_a?(Hash) && checkout["with"]["persist-credentials"] == false
  aggregate = jobs.values.find { |j| j.is_a?(Hash) && j["name"] == "CI" }
  out << "the CI aggregate must not need #{JOB}; it is housekeeping, not a gate" if aggregate && Array(aggregate["needs"]).include?(JOB)
  out
end

real = problems(doc)
unless real.empty?
  warn real.map { |p| "FAIL: #{p}" }.join("\n")
  exit 1
end
def mutated(doc)
  copy = Marshal.load(Marshal.dump(doc)); yield copy; copy
end
j = lambda { |d| d["jobs"]["cancel-superseded"] }
step = lambda { |d| j.(d)["steps"].find { |s| s.key?("run") } }
mutations = {
  "job removed" => lambda { |d| d["jobs"].delete("cancel-superseded") },
  "runs on push too" => lambda { |d| j.(d).delete("if") },
  "write-all" => lambda { |d| j.(d)["permissions"] = "write-all" },
  "extra contents: write" => lambda { |d| j.(d)["permissions"]["contents"] = "write" },
  "run with || true" => lambda { |d| step.(d)["run"] += " || true" },
  "step if: false" => lambda { |d| step.(d)["if"] = false },
  "HEAD_SHA from github.sha" => lambda { |d| step.(d)["env"]["HEAD_SHA"] = "${{ github.sha }}" },
  "credentials persisted" => lambda { |d| j.(d)["steps"].find { |s| s["uses"].to_s.start_with?("actions/checkout@") }["with"]["persist-credentials"] = true },
  "workflow group back" => lambda { |d| d["concurrency"] = { "group" => "x", "cancel-in-progress" => true } },
  "aggregate gates on it" => lambda { |d| d["jobs"]["ci"]["needs"] = Array(d["jobs"]["ci"]["needs"]) + ["cancel-superseded"] },
}
survived = mutations.select { |_, m| problems(mutated(doc, &m)).empty? }.keys
unless survived.empty?
  warn "FAIL: call-site pin did not refuse: #{survived.join(', ')}"
  exit 1
end
puts "call-site pin refuses #{mutations.length} neutralisations"
RUBY
then
  pass "ci.yml runs the cancel job for pull requests only, with actions: write and no shared group"
else
  fail "ci.yml does not run the cancel job as required"
fi

if [[ "$failures" -ne 0 ]]; then
  printf '%d cancel-superseded case(s) failed\n' "$failures" >&2
  exit 1
fi
echo "cancel-superseded contract: all cases passed"
