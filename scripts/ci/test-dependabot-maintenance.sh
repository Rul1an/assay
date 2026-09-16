#!/usr/bin/env bash
# Behavioral contract for scripts/ci/dependabot-maintenance.sh + dependabot-maintenance.yml (#3035).
#
# Branch updates and auto-merge enabled with GITHUB_TOKEN trigger no push workflows on main, and
# leave every pull_request run on the refreshed head in action_required. The lane therefore acts
# through a GitHub App installation token. The script must refuse to act under any other identity,
# and a refused update or auto-merge must fail the run instead of passing silently.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${SCRIPT:-${ROOT}/scripts/ci/dependabot-maintenance.sh}"
WORKFLOW="${WORKFLOW:-${ROOT}/.github/workflows/dependabot-maintenance.yml}"
APP_TOKEN_ACTION="actions/create-github-app-token@bcd2ba49218906704ab6c1aa796996da409d3eb1"
FAILURES=0
TEST_TEMP_DIR=""

cleanup() { [[ -n "$TEST_TEMP_DIR" ]] && rm -rf "$TEST_TEMP_DIR"; }
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  FAILURES=$((FAILURES + 1))
}

ok() { echo "ok   $*"; }

[[ -f "$SCRIPT" ]] || { echo "FAIL: missing $SCRIPT" >&2; exit 1; }
[[ -f "$WORKFLOW" ]] || { echo "FAIL: missing $WORKFLOW" >&2; exit 1; }

TEST_TEMP_DIR="$(mktemp -d)"
BIN="${TEST_TEMP_DIR}/bin"
mkdir -p "$BIN"

cat >"${BIN}/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${FAKE_GH_LOG}"
case "$1 ${2:-}" in
  "auth status") exit 0 ;;
  "pr list")
    for arg in "$@"; do
      if [[ "$arg" == "app/dependabot" ]]; then
        cat "${FAKE_PRS_JSON}"
        exit 0
      fi
    done
    printf '%s\n' '[]'
    ;;
  "pr view") printf '%s\n' 'BEHIND' ;;
  "pr merge")
    [[ "${FAKE_MERGE_FAIL:-}" == "1" ]] && { echo "auto-merge refused" >&2; exit 1; }
    exit 0
    ;;
  "api --method")
    [[ "${FAKE_UPDATE_FAIL:-}" == "1" ]] && { echo "update-branch refused" >&2; exit 1; }
    exit 0
    ;;
  *) echo "fake gh: unexpected call: $*" >&2; exit 97 ;;
esac
EOF
chmod +x "${BIN}/gh"

PRS_JSON="${TEST_TEMP_DIR}/prs.json"
cat >"$PRS_JSON" <<'EOF'
[{"number":41,"title":"chore(deps): bump example","mergeStateStatus":"BEHIND","isDraft":false,"autoMergeRequest":null,"url":"https://example.invalid/41","author":{"login":"app/dependabot"},"headRefOid":"0000000000000000000000000000000000000041"}]
EOF

# run_case NAME [VAR=VALUE ...]: runs the script with a clean identity environment plus the given
# assignments. Sets CASE_RC, CASE_OUT (stdout+stderr) and CASE_LOG (the fake gh call log).
run_case() {
  local name="$1"
  shift
  CASE_LOG="${TEST_TEMP_DIR}/${name}.gh.log"
  CASE_OUT="${TEST_TEMP_DIR}/${name}.out"
  : >"$CASE_LOG"
  set +e
  env -u APP_SLUG -u EXPECTED_APP_SLUG -u GITHUB_STEP_SUMMARY \
    PATH="${BIN}:${PATH}" FAKE_GH_LOG="$CASE_LOG" FAKE_PRS_JSON="$PRS_JSON" GH_TOKEN="fake-token" \
    "$@" bash "$SCRIPT" "Rul1an/assay" >"$CASE_OUT" 2>&1
  CASE_RC=$?
  set -e
}

gh_calls() { grep -c . "$CASE_LOG" || true; }

# 1. No App identity at all: refuse before any GitHub call.
run_case no-identity
if [[ "$CASE_RC" -ne 0 && "$(gh_calls)" -eq 0 ]] && grep -q "APP_SLUG" "$CASE_OUT"; then
  ok "refuses without an App identity, before any gh call"
else
  fail "missing App identity must refuse before any gh call (rc=$CASE_RC calls=$(gh_calls))"
fi

# 2. Expected slug unset: the check has nothing to compare against, so it must refuse.
run_case no-expected APP_SLUG=assay-dependabot-lane
if [[ "$CASE_RC" -ne 0 && "$(gh_calls)" -eq 0 ]] && grep -q "EXPECTED_APP_SLUG" "$CASE_OUT"; then
  ok "refuses when the expected slug is not configured"
else
  fail "unset EXPECTED_APP_SLUG must refuse before any gh call (rc=$CASE_RC calls=$(gh_calls))"
fi

# 3. A token from a different App (or the github-actions App) is not this lane's identity.
run_case wrong-app APP_SLUG=github-actions EXPECTED_APP_SLUG=assay-dependabot-lane
if [[ "$CASE_RC" -ne 0 && "$(gh_calls)" -eq 0 ]] && grep -q "github-actions" "$CASE_OUT"; then
  ok "refuses a token minted for a different App"
else
  fail "mismatched App slug must refuse before any gh call (rc=$CASE_RC calls=$(gh_calls))"
fi

# 4. Correct identity: a BEHIND PR is updated and auto-merge is enabled, and the run passes.
run_case happy APP_SLUG=assay-dependabot-lane EXPECTED_APP_SLUG=assay-dependabot-lane
if [[ "$CASE_RC" -eq 0 ]] \
  && grep -q "api --method PUT repos/Rul1an/assay/pulls/41/update-branch" "$CASE_LOG" \
  && grep -q "pr merge 41 --repo Rul1an/assay --merge --auto" "$CASE_LOG"; then
  ok "updates a BEHIND PR and enables auto-merge under the App identity"
else
  fail "happy path must update and enable auto-merge and exit 0 (rc=$CASE_RC)"
  sed 's/^/      /' "$CASE_OUT" >&2
fi

# 5. A refused branch update fails the run, and auto-merge is still attempted for the PR.
run_case update-refused APP_SLUG=assay-dependabot-lane EXPECTED_APP_SLUG=assay-dependabot-lane FAKE_UPDATE_FAIL=1
if [[ "$CASE_RC" -ne 0 ]] && grep -q "branch=update-failed" "$CASE_OUT" \
  && grep -q "pr merge 41" "$CASE_LOG"; then
  ok "a refused branch update turns the run red"
else
  fail "a refused update-branch must exit non-zero (rc=$CASE_RC)"
fi

# 6. A refused auto-merge fails the run.
run_case merge-refused APP_SLUG=assay-dependabot-lane EXPECTED_APP_SLUG=assay-dependabot-lane FAKE_MERGE_FAIL=1
if [[ "$CASE_RC" -ne 0 ]] && grep -q "auto_merge=enable-failed" "$CASE_OUT"; then
  ok "a refused auto-merge turns the run red"
else
  fail "a refused auto-merge must exit non-zero (rc=$CASE_RC)"
fi

# 7. Workflow contract: the App token is the only write credential the script receives.
wf="$(cat "$WORKFLOW")"
check_wf() {
  local label="$1" pattern="$2"
  if grep -Eq -- "$pattern" <<<"$wf"; then ok "workflow: $label"; else fail "workflow: $label"; fi
}
check_wf_absent() {
  local label="$1" pattern="$2"
  if grep -Eq -- "$pattern" <<<"$wf"; then fail "workflow: $label"; else ok "workflow: $label"; fi
}
check_wf "App token action is SHA-pinned" "uses: ${APP_TOKEN_ACTION} # v3\.2\.0"
check_wf "token step has id app-token" "id: app-token"
check_wf "client id comes from the environment variable" 'client-id: \$\{\{ vars\.DEPENDABOT_APP_CLIENT_ID \}\}'
check_wf "private key comes from the environment secret" 'private-key: \$\{\{ secrets\.DEPENDABOT_APP_PRIVATE_KEY \}\}'
check_wf "token requests contents: write" "permission-contents: write"
check_wf "token requests pull-requests: write" "permission-pull-requests: write"
check_wf_absent "token requests no workflows permission" "permission-workflows"
check_wf "job uses the dependabot-maintenance environment" "name: dependabot-maintenance"
check_wf "environment creates no deployment record" "deployment: false"
check_wf "GH_TOKEN is the App token" 'GH_TOKEN: \$\{\{ steps\.app-token\.outputs\.token \}\}'
check_wf "APP_SLUG is the token step output" 'APP_SLUG: \$\{\{ steps\.app-token\.outputs\.app-slug \}\}'
check_wf "EXPECTED_APP_SLUG is the environment variable" 'EXPECTED_APP_SLUG: \$\{\{ vars\.DEPENDABOT_APP_SLUG \}\}'
check_wf_absent "github.token is never handed to the script" 'github\.token'
check_wf_absent "job GITHUB_TOKEN no longer holds write scopes" '^[[:space:]]+(contents|pull-requests): write'

if [[ "$FAILURES" -ne 0 ]]; then
  echo "dependabot-maintenance contract: ${FAILURES} failure(s)" >&2
  exit 1
fi
echo "dependabot-maintenance contract: all checks passed"
