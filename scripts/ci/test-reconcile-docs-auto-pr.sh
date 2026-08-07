#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${ROOT}/scripts/ci/reconcile-docs-auto-pr.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_line() {
  grep -Fxq "$2" "$1" || fail "missing '$2' in $1"
}

valid_view() {
  local number="$1"
  local head="$2"
  local state="$3"
  local base="${4:-base-main}"
  jq -cn \
    --argjson number "$number" \
    --arg head "$head" \
    --arg state "$state" \
    --arg base "$base" \
    '{
      number: $number,
      author: {login: "app/github-actions"},
      baseRefName: "main",
      baseRefOid: $base,
      headRefName: "docs/auto-update",
      headRefOid: $head,
      headRepositoryOwner: {login: "Rul1an"},
      isCrossRepository: false,
      mergeStateStatus: $state
    }'
}

run_case() {
  local name="$1"
  local list_json="$2"
  local view_jsonl="${3:-}"
  local files_json="${4:-}"
  local expect_success="${5:-true}"
  local live_base="${6:-base-main}"
  local temp_dir
  temp_dir="$(mktemp -d)"
  [[ -n "$files_json" ]] || files_json='[[{"filename":"docs/generated.md"}]]'

  mkdir -p "${temp_dir}/bin"
  cat > "${temp_dir}/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$FAKE_GH_LOG"
case "$1 $2" in
  "pr list")
    printf '%s\n' "$FAKE_LIST_JSON"
    ;;
  "pr close")
    # Recorded in FAKE_GH_LOG by the printf above, so a case can assert it happened.
    ;;
  "pr view")
    count="$(cat "$FAKE_VIEW_COUNT")"
    count=$((count + 1))
    printf '%s\n' "$count" > "$FAKE_VIEW_COUNT"
    response="$(sed -n "${count}p" "$FAKE_VIEW_FILE")"
    [[ -n "$response" ]] || response="$(tail -n 1 "$FAKE_VIEW_FILE")"
    if [[ " $* " == *" --jq .headRefOid "* ]]; then
      jq -r '.headRefOid' <<<"$response"
    else
      printf '%s\n' "$response"
    fi
    ;;
  "api --paginate")
    printf '%s\n' "$FAKE_FILES_JSON"
    ;;
  "api --method")
    printf '%s\n' '{"message":"Updating pull request branch."}'
    ;;
  *)
    if [[ "$1" == "api" && "$2" == repos/*/commits/* ]]; then
      count="$(cat "$FAKE_BASE_COUNT")"
      count=$((count + 1))
      printf '%s\n' "$count" > "$FAKE_BASE_COUNT"
      response="$(sed -n "${count}p" "$FAKE_BASE_FILE")"
      [[ -n "$response" ]] || response="$(tail -n 1 "$FAKE_BASE_FILE")"
      printf '%s\n' "$response"
    else
      echo "unexpected gh invocation: $*" >&2
      exit 91
    fi
    ;;
esac
EOF
  chmod +x "${temp_dir}/bin/gh"

  export FAKE_GH_LOG="${temp_dir}/gh.log"
  export FAKE_LIST_JSON="$list_json"
  export FAKE_VIEW_FILE="${temp_dir}/view.jsonl"
  export FAKE_VIEW_COUNT="${temp_dir}/view.count"
  export FAKE_FILES_JSON="$files_json"
  export FAKE_BASE_FILE="${temp_dir}/base-sha.txt"
  export FAKE_BASE_COUNT="${temp_dir}/base-sha.count"
  export GITHUB_OUTPUT="${temp_dir}/outputs"
  printf '%s\n' "$view_jsonl" > "$FAKE_VIEW_FILE"
  printf '0\n' > "$FAKE_VIEW_COUNT"
  printf '%s\n' "$live_base" > "$FAKE_BASE_FILE"
  printf '0\n' > "$FAKE_BASE_COUNT"

  if PATH="${temp_dir}/bin:${PATH}" \
    EXPECTED_BASE_SHA="base-main" \
    REPO="Rul1an/assay" \
    MAX_ATTEMPTS=3 \
    RETRY_SECONDS=0 \
    bash "$SCRIPT" >"${temp_dir}/stdout" 2>"${temp_dir}/stderr"; then
    [[ "$expect_success" == "true" ]] || fail "$name unexpectedly succeeded"
  else
    [[ "$expect_success" == "false" ]] || {
      cat "${temp_dir}/stderr" >&2
      fail "$name unexpectedly failed"
    }
  fi

  CASE_DIR="$temp_dir"
}

cleanup_case() {
  rm -rf "$CASE_DIR"
}

run_case "no PR" '[]'
assert_line "$GITHUB_OUTPUT" "pr_number="
assert_line "$GITHUB_OUTPUT" "head_sha="
assert_line "$GITHUB_OUTPUT" "branch_updated=false"
! grep -q '^api --method' "$FAKE_GH_LOG" || fail "no-PR case updated a branch"
cleanup_case

run_case "current PR" '[{"number":42}]' "$(valid_view 42 head-current CLEAN)"
assert_line "$GITHUB_OUTPUT" "pr_number=42"
assert_line "$GITHUB_OUTPUT" "head_sha=head-current"
assert_line "$GITHUB_OUTPUT" "branch_updated=false"
! grep -q '^api --method' "$FAKE_GH_LOG" || fail "current PR updated a branch"
cleanup_case

run_case "behind PR" \
  '[{"number":43}]' \
  "$(valid_view 43 head-old BEHIND base-old)
$(valid_view 43 head-new BLOCKED)"
assert_line "$GITHUB_OUTPUT" "head_sha=head-new"
assert_line "$GITHUB_OUTPUT" "branch_updated=true"
grep -Fq "expected_head_sha=head-old" "$FAKE_GH_LOG" ||
  fail "update was not bound to the observed head"
cleanup_case

run_case "raced update" \
  '[{"number":44}]' \
  "$(valid_view 44 head-old BEHIND base-old)
$(valid_view 44 head-raced BEHIND base-raced)
$(valid_view 44 head-current BLOCKED)"
assert_line "$GITHUB_OUTPUT" "head_sha=head-current"
[[ "$(grep -c '^api --method' "$FAKE_GH_LOG")" -eq 2 ]] ||
  fail "raced BEHIND head was not reconciled again"
grep -Fq "expected_head_sha=head-raced" "$FAKE_GH_LOG" ||
  fail "second update was not bound to the raced head"
cleanup_case

run_case "same update remains in flight" \
  '[{"number":54}]' \
  "$(valid_view 54 head-old BEHIND base-old)
$(valid_view 54 head-old BEHIND base-old)
$(valid_view 54 head-new BLOCKED)"
assert_line "$GITHUB_OUTPUT" "head_sha=head-new"
[[ "$(grep -c '^api --method' "$FAKE_GH_LOG")" -eq 1 ]] ||
  fail "same in-flight head received duplicate update requests"
cleanup_case

run_case "unknown then behind" \
  '[{"number":45}]' \
  "$(valid_view 45 head-old UNKNOWN base-old)
$(valid_view 45 head-old BEHIND base-old)
$(valid_view 45 head-new CLEAN)"
assert_line "$GITHUB_OUTPUT" "head_sha=head-new"
assert_line "$GITHUB_OUTPUT" "branch_updated=true"
cleanup_case

run_case "unknown stays unknown" \
  '[{"number":46}]' \
  "$(valid_view 46 head-old UNKNOWN)" \
  '[[{"filename":"docs/generated.md"}]]' \
  false
grep -Fq "did not reach a stable merge state" "$CASE_DIR/stderr" ||
  fail "persistent UNKNOWN diagnostic missing"
cleanup_case

run_case "dirty PR" \
  '[{"number":47}]' \
  "$(valid_view 47 head-dirty DIRTY)" \
  '[[{"filename":"docs/generated.md"}]]' \
  false
grep -Fq "merge state DIRTY" "$CASE_DIR/stderr" ||
  fail "DIRTY diagnostic missing"
cleanup_case

run_case "update did not advance" \
  '[{"number":52}]' \
  "$(valid_view 52 head-stuck BEHIND)
$(valid_view 52 head-stuck CLEAN)" \
  '[[{"filename":"docs/generated.md"}]]' \
  false
grep -Fq "did not advance from head-stuck" "$CASE_DIR/stderr" ||
  fail "stuck update diagnostic missing"
cleanup_case

run_case "duplicate PRs" \
  '[{"number":48},{"number":49}]' \
  '' \
  '[[{"filename":"docs/generated.md"}]]' \
  false
grep -Fq "expected exactly one open docs PR" "$CASE_DIR/stderr" ||
  fail "duplicate PR diagnostic missing"
cleanup_case

run_case "non-doc change" \
  '[{"number":50}]' \
  "$(valid_view 50 head-source CLEAN)" \
  '[[{"filename":"docs/generated.md"},{"filename":"src/main.rs"}]]' \
  false
grep -Fq "contains non-doc paths" "$CASE_DIR/stderr" ||
  fail "non-doc diagnostic missing"
cleanup_case

cross_repo="$(
  valid_view 51 head-fork CLEAN |
    jq -c '.isCrossRepository = true | .headRepositoryOwner.login = "attacker"'
)"
run_case "cross-repository PR" \
  '[{"number":51}]' \
  "$cross_repo" \
  '[[{"filename":"docs/generated.md"}]]' \
  false
grep -Fq "unexpected author, repository, or branch identity" "$CASE_DIR/stderr" ||
  fail "cross-repository diagnostic missing"
cleanup_case

run_case "main advanced during generation" \
  '[{"number":55}]' \
  "$(valid_view 55 head-current CLEAN)" \
  '[[{"filename":"docs/generated.md"}]]' \
  false \
  base-new
grep -Fq "main advanced from base-main to base-new" "$CASE_DIR/stderr" ||
  fail "stale generation diagnostic missing"
cleanup_case

run_case "main advances during validation" \
  '[{"number":56}]' \
  "$(valid_view 56 head-current CLEAN)" \
  '[[{"filename":"docs/generated.md"}]]' \
  false \
  "base-main
base-new"
grep -Fq "main advanced from base-main to base-new during validation" "$CASE_DIR/stderr" ||
  fail "final base-race diagnostic missing"
cleanup_case

run_case "head moves during file validation" \
  '[{"number":53}]' \
  "$(valid_view 53 head-validated CLEAN)
$(valid_view 53 head-moved CLEAN)" \
  '[[{"filename":"docs/generated.md"}]]' \
  false
grep -Fq "moved from head-validated to head-moved during validation" "$CASE_DIR/stderr" ||
  fail "post-file-validation head race diagnostic missing"
cleanup_case

check_workflow_contract() {
  # shellcheck disable=SC2016 # Ruby source and GitHub expressions are literal.
  ruby -ryaml -e '
    def load_yaml(path)
      YAML.safe_load_file(path, aliases: false)
    end

    docs, ci, lane, host = ARGV.map { |path| load_yaml(path) }
    docs_on = docs["on"] || docs[true]
    ci_on = ci["on"] || ci[true]
    lane_on = lane["on"] || lane[true]
    host_on = host["on"] || host[true]

    abort("docs workflow must run on every main push") unless
      docs_on.dig("push", "branches") == ["main"] && !docs_on.dig("push").key?("paths-ignore")
    abort("docs workflow must cancel stale generation") unless
      docs.dig("concurrency", "group") == "docs-auto-update" &&
      docs.dig("concurrency", "cancel-in-progress") == true
    abort("docs workflow needs actions: write") unless
      docs.dig("jobs", "generate-docs", "permissions", "actions") == "write"
    abort("docs workflow must not synthesize checks") if
      docs.dig("jobs", "generate-docs", "permissions").key?("checks")

    docs_steps = docs.dig("jobs", "generate-docs", "steps")
    reconcile = docs_steps.find { |step| step["name"] == "Reconcile existing docs PR with main" }
    abort("reconciliation is not bound to the generation base") unless
      reconcile && reconcile.dig("env", "EXPECTED_BASE_SHA").to_s.include?("github.sha")
    dispatch = docs_steps.find { |step| step["name"] == "Dispatch required workflows for reconciled docs head" }
    abort("missing required-workflow dispatch step") unless dispatch
    dispatch_if = dispatch["if"].to_s
    %w[has_changes branch_updated force_update].each do |term|
      abort("dispatch condition missing #{term}") unless dispatch_if.include?(term)
    end
    dispatch_run = dispatch["run"].to_s
    %w[ci.yml assay-runner-lane-check.yml host-capability-check.yml].each do |workflow|
      abort("missing dispatch for #{workflow}") unless
        dispatch_run.include?("gh workflow run #{workflow}")
    end
    abort("dispatches are not branch-bound") unless
      dispatch_run.scan(%r{--ref "\$BRANCH"}).length == 3
    abort("dispatches are not expected-head-bound") unless
      dispatch_run.scan(/expected_head_sha=/).length == 3 &&
      dispatch.dig("env", "EXPECTED_HEAD_SHA").to_s.include?("reconcile_pr.outputs.head_sha")
    abort("docs workflow still creates required check-runs") if
      dispatch_run.include?("check-runs")

    ci_inputs = ci_on.dig("workflow_dispatch", "inputs")
    abort("CI dispatch inputs missing") unless
      ci_inputs.key?("pr_number") && ci_inputs.key?("expected_head_sha")
    abort("CI scope lacks pull-request read permission") unless
      ci.dig("jobs", "scope", "permissions", "pull-requests") == "read"
    ci_run = ci.dig("jobs", "scope", "steps").find { |step| step["id"] == "detect" }["run"].to_s
    %w[GITHUB_SHA EXPECTED_HEAD_SHA pulls/ files?per_page=100].each do |term|
      abort("CI dispatch guard missing #{term}") unless ci_run.include?(term)
    end

    lane_inputs = lane_on.dig("workflow_dispatch", "inputs")
    abort("lane expected-head input missing") unless lane_inputs.key?("expected_head_sha")
    lane_run = lane.dig("jobs", "lane-check", "steps")
      .find { |step| step["name"] == "Check PR delegated lane proof" }["run"].to_s
    %w[GITHUB_SHA EXPECTED_HEAD_SHA pulls/].each do |term|
      abort("lane dispatch guard missing #{term}") unless lane_run.include?(term)
    end

    host_inputs = host_on.dig("workflow_dispatch", "inputs")
    abort("host dispatch inputs missing") unless
      host_inputs.key?("pr_number") && host_inputs.key?("expected_head_sha")
    host_run = host.dig("jobs", "check", "steps")
      .find { |step| step["name"] == "Validate host-capability proof" }["run"].to_s
    %w[GITHUB_SHA EXPECTED_HEAD_SHA pulls/].each do |term|
      abort("host dispatch guard missing #{term}") unless host_run.include?(term)
    end
  ' "$@"
}

DOCS_WORKFLOW="${ROOT}/.github/workflows/docs-auto-update.yml"
CI_WORKFLOW="${ROOT}/.github/workflows/ci.yml"
LANE_WORKFLOW="${ROOT}/.github/workflows/assay-runner-lane-check.yml"
HOST_WORKFLOW="${ROOT}/.github/workflows/host-capability-check.yml"

# --- an empty docs PR is finished, not broken ------------------------------------------------
#
# The reconciler used to reject this as "an invalid or empty files response" and exit 2, so
# `Auto-Update Docs` failed on every merge to main for eight consecutive runs while the system was
# working correctly: #2081 moved the drift check onto the PR that causes the drift, which is exactly
# what makes the bot's PR empty.
run_case "empty docs PR closes and succeeds" \
  '[{"number":42}]' \
  "$(valid_view 42 head-current CLEAN)" \
  '[[]]' \
  true

if grep -q "pr close 42" "${CASE_DIR}/gh.log"; then
  echo "ok    an empty docs PR is closed"
else
  echo "FAIL  an empty docs PR was not closed"
  FAILURES=$((FAILURES + 1))
fi

# --- a malformed response still refuses --------------------------------------------------------
#
# The distinction the old check collapsed: this one means the reconciler cannot see what it is
# reconciling, and refusing is correct.
run_case "malformed files response refuses" \
  '[{"number":42}]' \
  "$(valid_view 42 head-current CLEAN)" \
  '{"message":"Not Found"}' \
  false

check_workflow_contract \
  "$DOCS_WORKFLOW" \
  "$CI_WORKFLOW" \
  "$LANE_WORKFLOW" \
  "$HOST_WORKFLOW"

mutation_dir="$(mktemp -d)"
trap 'rm -rf "$mutation_dir"' EXIT
cp "$DOCS_WORKFLOW" "$mutation_dir/docs.yml"
cp "$CI_WORKFLOW" "$mutation_dir/ci.yml"
cp "$LANE_WORKFLOW" "$mutation_dir/lane.yml"
cp "$HOST_WORKFLOW" "$mutation_dir/host.yml"

ruby -pi -e 'sub("cancel-in-progress: true", "cancel-in-progress: false")' \
  "$mutation_dir/docs.yml"
if check_workflow_contract \
  "$mutation_dir/docs.yml" \
  "$mutation_dir/ci.yml" \
  "$mutation_dir/lane.yml" \
  "$mutation_dir/host.yml" >/dev/null 2>&1; then
  fail "workflow contract accepted stale-run concurrency"
fi

cp "$DOCS_WORKFLOW" "$mutation_dir/docs.yml"
ruby -pi -e \
  'sub("gh workflow run host-capability-check.yml", "echo skipped-host-workflow")' \
  "$mutation_dir/docs.yml"
if check_workflow_contract \
  "$mutation_dir/docs.yml" \
  "$mutation_dir/ci.yml" \
  "$mutation_dir/lane.yml" \
  "$mutation_dir/host.yml" >/dev/null 2>&1; then
  fail "workflow contract accepted a missing canonical dispatch"
fi

cp "$DOCS_WORKFLOW" "$mutation_dir/docs.yml"
ruby -pi -e 'gsub("expected_head_sha=", "unbound_head_sha=")' \
  "$mutation_dir/docs.yml"
if check_workflow_contract \
  "$mutation_dir/docs.yml" \
  "$mutation_dir/ci.yml" \
  "$mutation_dir/lane.yml" \
  "$mutation_dir/host.yml" >/dev/null 2>&1; then
  fail "workflow contract accepted an unbound dispatch"
fi

echo "docs auto-update reconciliation tests passed"
