#!/usr/bin/env bash
set -euo pipefail

# Contract test for the MCP Registry release transaction (issue #1870):
#   - resolve-mcp-registry-tag.sh: release / prerelease / workflow_dispatch /
#     workflow_call-from-tag-push tag resolution, fail-closed on everything else
#   - check-mcp-registry-version.sh: absent vs published vs content mismatch
#   - record-mcp-registry-result.sh: idempotent terminal-result line on the release
#   - workflow wiring: release.yml runs the publisher as a dependent job and
#     mcp-registry-publish.yml stays dispatchable for idempotent recovery

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RESOLVE="${ROOT}/scripts/ci/resolve-mcp-registry-tag.sh"
CHECK="${ROOT}/scripts/ci/check-mcp-registry-version.sh"
RECORD="${ROOT}/scripts/ci/record-mcp-registry-result.sh"
PUBLISH_WF="${ROOT}/.github/workflows/mcp-registry-publish.yml"
RELEASE_WF="${ROOT}/.github/workflows/release.yml"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# ---------------------------------------------------------------
# resolve-mcp-registry-tag.sh
# ---------------------------------------------------------------

resolve_case() {
  local name="$1" expect_success="$2" expect_tag="$3"
  shift 3
  local out
  if out="$(env -i PATH="$PATH" "$@" bash "$RESOLVE" 2>/dev/null)"; then
    [[ "$expect_success" == "true" ]] || fail "resolve: $name unexpectedly succeeded"
    [[ "$out" == "$expect_tag" ]] || fail "resolve: $name returned '$out', expected '$expect_tag'"
  else
    [[ "$expect_success" == "false" ]] || fail "resolve: $name unexpectedly failed"
  fi
}

resolve_case "stable release event" true "v3.35.0" \
  EVENT_NAME=release RELEASE_TAG=v3.35.0 RELEASE_PRERELEASE=false
resolve_case "prerelease release event is refused" false "" \
  EVENT_NAME=release RELEASE_TAG=v3.36.0-rc1 RELEASE_PRERELEASE=true
resolve_case "release event with unstated prerelease flag is refused" false "" \
  EVENT_NAME=release RELEASE_TAG=v3.35.0
resolve_case "manual dispatch" true "v3.35.0" \
  EVENT_NAME=workflow_dispatch INPUT_VERSION=v3.35.0
resolve_case "workflow_call from a tag push" true "v3.35.0" \
  EVENT_NAME=push INPUT_VERSION=v3.35.0
resolve_case "workflow_call from a dispatched release run" true "v3.35.0" \
  EVENT_NAME=workflow_dispatch INPUT_VERSION=v3.35.0
resolve_case "tag push without an explicit version input is refused" false "" \
  EVENT_NAME=push
resolve_case "rc tag is refused on every path" false "" \
  EVENT_NAME=push INPUT_VERSION=v3.36.0-rc1
resolve_case "beta tag is refused" false "" \
  EVENT_NAME=workflow_dispatch INPUT_VERSION=v3.36.0-beta.1
resolve_case "multi-line tag is refused" false "" \
  EVENT_NAME=workflow_dispatch INPUT_VERSION="$(printf 'v3.35.0\nevil')"
resolve_case "unsupported event is refused" false "" \
  EVENT_NAME=schedule
resolve_case "empty event is refused" false ""

echo "ok: resolve-mcp-registry-tag cases"

# ---------------------------------------------------------------
# check-mcp-registry-version.sh
# ---------------------------------------------------------------

make_server_json() {
  local path="$1" version="$2" sha="$3" name="${4:-io.github.Rul1an/assay-mcp-server}"
  jq -cn --arg name "$name" --arg version "$version" --arg sha "$sha" \
    '{name: $name, version: $version, packages: [{fileSha256: $sha}]}' > "$path"
}

check_case() {
  local name="$1" http_code="$2" body_json="$3" local_json="$4" \
    expect_success="$5" expect_status="${6:-}"
  local temp_dir
  temp_dir="$(mktemp -d)"
  mkdir -p "${temp_dir}/bin"
  cat > "${temp_dir}/bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
out=""
prev=""
for arg in "$@"; do
  if [[ "$prev" == "-o" ]]; then
    out="$arg"
  fi
  prev="$arg"
done
[[ -n "$out" ]] || { echo "fake curl expects -o" >&2; exit 90; }
cat "$FAKE_CURL_BODY" > "$out"
printf '%s' "$FAKE_CURL_CODE"
EOF
  chmod +x "${temp_dir}/bin/curl"
  printf '%s\n' "$body_json" > "${temp_dir}/body.json"

  local out rc=0
  out="$(PATH="${temp_dir}/bin:${PATH}" \
    FAKE_CURL_BODY="${temp_dir}/body.json" \
    FAKE_CURL_CODE="$http_code" \
    TAG="v3.35.0" \
    LOCAL_SERVER_JSON="$local_json" \
    bash "$CHECK" 2>"${temp_dir}/stderr")" || rc=$?
  if [[ "$expect_success" == "true" ]]; then
    [[ "$rc" -eq 0 ]] || { cat "${temp_dir}/stderr" >&2; fail "check: $name unexpectedly failed"; }
    [[ "$out" == "$expect_status" ]] || fail "check: $name returned '$out', expected '$expect_status'"
  else
    [[ "$rc" -ne 0 ]] || fail "check: $name unexpectedly succeeded"
  fi
  rm -rf "$temp_dir"
}

WORK="$(mktemp -d)"
make_server_json "${WORK}/server.json" "3.35.0" "aaaa1111"
make_server_json "${WORK}/other-sha.json" "3.35.0" "bbbb2222"
make_server_json "${WORK}/wrong-version.json" "3.34.0" "aaaa1111"

published_body="$(jq -cn '{server: {name: "io.github.Rul1an/assay-mcp-server", version: "3.35.0", packages: [{fileSha256: "aaaa1111"}]}}')"

divergent_packages_body="$(jq -cn '{server: {name: "io.github.Rul1an/assay-mcp-server", version: "3.35.0", packages: [{fileSha256: "aaaa1111", identifier: "https://elsewhere.example/other.mcpb"}]}}')"

check_case "absent version reports absent" 404 '{"error":"not found"}' \
  "${WORK}/server.json" true "absent"
check_case "published identical version reports published" 200 "$published_body" \
  "${WORK}/server.json" true "published"
check_case "published version with different content fails closed" 200 "$published_body" \
  "${WORK}/other-sha.json" false
check_case "same digest but divergent package set fails closed" 200 "$divergent_packages_body" \
  "${WORK}/server.json" false
check_case "registry payload version mismatch fails closed" 200 "$published_body" \
  "${WORK}/wrong-version.json" false
check_case "unexpected registry status fails closed" 500 '{"error":"boom"}' \
  "${WORK}/server.json" false
check_case "tag and local server.json version must agree" 404 '{}' \
  "${WORK}/wrong-version.json" false

grep -q -- '--max-time' "$CHECK" \
  || fail "check: registry curl must carry a bounded --max-time"

echo "ok: check-mcp-registry-version cases"

# ---------------------------------------------------------------
# record-mcp-registry-result.sh
# ---------------------------------------------------------------

record_case() {
  local name="$1" result="$2" initial_body="$3" expect_success="$4"
  local temp_dir
  temp_dir="$(mktemp -d)"
  mkdir -p "${temp_dir}/bin"
  cat > "${temp_dir}/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "$1 $2" in
  "release view")
    cat "$FAKE_RELEASE_BODY"
    ;;
  "release edit")
    notes_file=""
    prev=""
    for arg in "$@"; do
      if [[ "$prev" == "--notes-file" ]]; then
        notes_file="$arg"
      fi
      prev="$arg"
    done
    [[ -n "$notes_file" ]] || { echo "fake gh expects --notes-file" >&2; exit 90; }
    cat "$notes_file" > "$FAKE_RELEASE_BODY"
    ;;
  *)
    echo "unexpected gh invocation: $*" >&2
    exit 91
    ;;
esac
EOF
  chmod +x "${temp_dir}/bin/gh"
  printf '%s\n' "$initial_body" > "${temp_dir}/release-body.md"

  local rc=0
  PATH="${temp_dir}/bin:${PATH}" \
    FAKE_RELEASE_BODY="${temp_dir}/release-body.md" \
    VERSION="v3.35.0" \
    RESULT="$result" \
    RUN_URL="https://github.com/Rul1an/assay/actions/runs/123" \
    bash "$RECORD" >/dev/null 2>"${temp_dir}/stderr" || rc=$?
  if [[ "$expect_success" == "true" ]]; then
    [[ "$rc" -eq 0 ]] || { cat "${temp_dir}/stderr" >&2; fail "record: $name unexpectedly failed"; }
    grep -c "mcp-registry-status" "${temp_dir}/release-body.md" | grep -qx "1" \
      || fail "record: $name must leave exactly one status marker"
    grep -q "actions/runs/123" "${temp_dir}/release-body.md" \
      || fail "record: $name must link the registry run"
    grep -q "$result" "${temp_dir}/release-body.md" \
      || fail "record: $name must record the terminal result"
  else
    [[ "$rc" -ne 0 ]] || fail "record: $name unexpectedly succeeded"
  fi
  rm -rf "$temp_dir"
}

record_case "first success append" success "release notes body" true
record_case "failure is recorded too" failure "release notes body" true
record_case "rerun replaces the previous marker" success \
  "release notes body
<!-- mcp-registry-status --> MCP Registry publication: failure (https://github.com/Rul1an/assay/actions/runs/99)" true
record_case "unknown result is refused" bogus "release notes body" false
# A prerelease's skipped publication must leave an explicit marker: absence
# on a release record must stay distinguishable from a pre-feature release
# (AGENTS.md: absence never reads clean).
record_case "skipped writes an explicit marker" skipped "release notes body" true

echo "ok: record-mcp-registry-result cases"

# ---------------------------------------------------------------
# Workflow wiring
# ---------------------------------------------------------------

wf_pin() {
  local file="$1" pattern="$2" why="$3"
  grep -Eq "$pattern" "$file" || fail "wiring: $why (pattern '$pattern' missing in ${file#"$ROOT"/})"
}

# The publish workflow must be callable from release.yml and keep manual recovery.
wf_pin "$PUBLISH_WF" '^[[:space:]]*workflow_call:' \
  "mcp-registry-publish.yml must expose workflow_call"
wf_pin "$PUBLISH_WF" '^[[:space:]]*workflow_dispatch:' \
  "mcp-registry-publish.yml must keep workflow_dispatch recovery"
wf_pin "$PUBLISH_WF" '^[[:space:]]*release:' \
  "mcp-registry-publish.yml must keep the release trigger for manually published releases"
# Workflow-level concurrency is ignored for called workflows; the guard must sit on the job.
awk '/^jobs:/{injobs=1} injobs && /concurrency:/{found=1} END{exit !found}' "$PUBLISH_WF" \
  || fail "wiring: publish concurrency group must be declared at job level"
wf_pin "$PUBLISH_WF" 'check-mcp-registry-version\.sh' \
  "publish must consult the registry before and after publishing"
wf_pin "$PUBLISH_WF" '^[[:space:]]*- name: Check whether this version is already published$' \
  "publish must pre-check the registry for idempotent retries"
wf_pin "$PUBLISH_WF" '^[[:space:]]*- name: Confirm the registry serves this version$' \
  "publish must confirm the terminal result by reading the registry back"
wf_pin "$PUBLISH_WF" 'MCP_PUBLISHER_LINUX_AMD64_SHA256' \
  "publisher checksum enforcement must remain"
wf_pin "$PUBLISH_WF" '^[[:space:]]*timeout-minutes:' \
  "publish job must carry a bounded timeout"
wf_pin "$PUBLISH_WF" 'retrying in' \
  "terminal confirmation must retry read-after-write lag before failing"
# The publish job's if may only reference the release event: under
# workflow_call the caller's event is push/workflow_dispatch, and any
# condition that can skip the called job there turns an unpublished release
# into a green caller job (all-skipped reusable workflows read success).
grep -Eq "if: \\$\\{\\{ github\\.event_name != 'release' \\|\\| github\\.event\\.release\\.prerelease == false \\}\\}" "$PUBLISH_WF" \
  || fail "wiring: publish job if-condition must stay release-event-only (all-skipped call reads green)"
wf_pin "$PUBLISH_WF" 'login github-oidc' \
  "OIDC identity must remain the publication credential"
wf_pin "$PUBLISH_WF" 'mcp-publisher validate server\.json' \
  "release asset validation must remain"

# release.yml must run the publisher as part of the release transaction.
wf_pin "$RELEASE_WF" '^[[:space:]]*publish-mcp-registry:' \
  "release.yml must have a publish-mcp-registry job"
wf_pin "$RELEASE_WF" 'uses: \./\.github/workflows/mcp-registry-publish\.yml' \
  "publish-mcp-registry must call the pinned local publisher workflow"
wf_pin "$RELEASE_WF" 'version: \$\{\{ needs\.release-contract\.outputs\.version \}\}' \
  "publisher must receive the contract-validated release version"
wf_pin "$RELEASE_WF" '^[[:space:]]*record-mcp-registry-result:' \
  "release.yml must record the registry terminal result on the release"
wf_pin "$RELEASE_WF" 'record-mcp-registry-result\.sh' \
  "record job must use the tested recording script"

python3 - "$RELEASE_WF" <<'PY'
import re
import sys

text = open(sys.argv[1]).read()

def job_block(name):
    match = re.search(rf"^  {name}:\n(.*?)(?=^  [a-zA-Z][\w-]*:|\Z)", text, re.S | re.M)
    if not match:
        sys.exit(f"wiring: job {name} not found in release.yml")
    return match.group(1)

publish = job_block("publish-mcp-registry")
record = job_block("record-mcp-registry-result")

for needed in ("release-contract", "release"):
    if needed not in publish.split("uses:")[0]:
        sys.exit(f"wiring: publish-mcp-registry must depend on {needed}")
for guard in ("-rc", "-beta"):
    if guard not in publish:
        sys.exit(f"wiring: publish-mcp-registry must exclude {guard} prereleases")
if "id-token: write" not in publish:
    sys.exit("wiring: publish-mcp-registry must grant id-token: write for OIDC")
if "contents: write" in publish:
    sys.exit("wiring: publish-mcp-registry must not hold contents: write")

if "always()" not in record:
    sys.exit("wiring: record job must run on publish failure too")
if "needs.release.result == 'success'" not in record:
    sys.exit(
        "wiring: record job must run exactly when a release record exists — "
        "including prerelease skips, excluding failed releases"
    )
if "publish-mcp-registry" not in record:
    sys.exit("wiring: record job must depend on the publish job")
if "contents: write" not in record:
    sys.exit("wiring: record job needs contents: write to edit the release")
PY

echo "ok: workflow wiring"
echo "PASS: mcp-registry release transaction contract"
