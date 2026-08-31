#!/usr/bin/env bash
# Shared by the published-release driver and its credential-free session tests.
# Caller supplies PYTHON_BIN, JQ_BIN, commands_file, results, version and fail().
# shellcheck disable=SC2016,SC2154

record_command() {
  local name="$1" status="$2"
  shift 2
  "$PYTHON_BIN" - "$commands_file" "$name" "$status" "$@" <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
record = {"name": sys.argv[2], "exit_code": int(sys.argv[3]), "argv": sys.argv[4:]}
with path.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")
PY
}

run_capture() {
  local name="$1" expected="$2" stdout_path="$3" stderr_path="$4"
  shift 4
  local status=0
  "$@" >"$stdout_path" 2>"$stderr_path" || status=$?
  record_command "$name" "$status" "$@"
  [[ "$status" -eq "$expected" ]] || {
    cat "$stderr_path" >&2
    fail "$name exited $status, expected $expected"
  }
}

run_published_release_extra_request_cases() {
  local scenario request proxy_status
  for scenario in allow unsupported; do
    mkdir "$results/$scenario" || fail "$scenario result directory must be fresh"
    request="$init_request"$'\n'"$call_request"
    if [[ "$scenario" == unsupported ]]; then
      # A local method rejection needs no upstream handshake; closing stdin must not race it.
      request='{"jsonrpc":"2.0","id":9,"method":"unsupported_for_probe"}'
    fi
    proxy_status=0
    printf '%s\n' "$request" \
      | (cd "$results/$scenario" && \
          "$PYTHON_BIN" -I "$harness_root/scripts/ci/published_release_proxy_phase.py" \
            --timeout-seconds 60 --policy allow --expect "$scenario") || proxy_status=$?
    # The helper records the actual child argv/status, even when its assertion fails.
    cat "$results/$scenario/commands.ndjson" >>"$commands_file" \
      || fail "$scenario command record is missing"
    [[ "$proxy_status" -eq 0 ]] || fail "$scenario request gate exited $proxy_status"
  done
  local allow_bundle="$results/allow/produced.bundle.tar.gz"
  run_capture "allow-produce-bundle" 0 "$results/allow/produce.stdout" "$results/allow/produce.stderr" \
    assay evidence import privileged-mcp-action \
      --decisions "$results/allow/decisions.ndjson" --bundle-out "$allow_bundle" \
      --run-id "published-release-${workflow_run_id}-${workflow_run_attempt}-allow" \
      --import-time 2026-01-01T00:00:00Z
  [[ -s "$allow_bundle" ]] || fail "allow bundle production yielded no bytes"
  run_capture "allow-verify-bundle" 0 "$results/allow/verify.json" "$results/allow/verify.stderr" \
    assay evidence verify-privileged-mcp-action "$allow_bundle" --format json --profile-version v1
  "$JQ_BIN" -se 'length == 1 and (.[0] |
    .schema == "assay.privileged_mcp_action.verify.report.v0" and
    .bundle_integrity == "pass" and .verdict == "valid" and
    .claims.policy_decision_recorded.status == "confirmed" and
    .claims.policy_decision_recorded.source_class == "producer_reported" and
    ([.claims.caller_visible_denial.status, .claims.upstream_delivery.status,
      .claims.external_side_effect.status] | all(. == "incomplete")))
  ' "$results/allow/verify.json" >/dev/null || fail "allow verification exceeded its evidence boundary"
}

run_published_release_session_product() {
  run_capture "doctor" 0 "$results/doctor.json" "$results/doctor.stderr" assay doctor --format json
  "$JQ_BIN" -se --arg version "$version" '
    length == 1 and (.[0] |
    .schema == "assay.doctor_report.v0" and .assay_version == $version and
    (.platform | type == "string") and
    (.status | . == "Ready" or . == "Degraded" or . == "Unsupported") and
    (.backend | (.selected | type == "string") and (.mode | type == "string")) and
    (.config_check | .status == "skipped" and (.reason | type == "string")) and
    (.landlock | ([.available, .fs_enforce, .net_enforce] | all(type == "boolean")) and
      (.abi_probe_status | type == "string") and (.net_connect_ruleset_probe | type == "string")) and
    (.bpf_lsm.available | type == "boolean") and
    (.helper | [.exists, .socket_exists] | all(type == "boolean")) and
    (.sandbox_features | [.env_scrubbing, .scoped_tmp, .fork_safe_preexec, .deny_conflict_detection] |
      all(type == "boolean")))
  ' "$results/doctor.json" >/dev/null || fail "doctor preflight output identity or fields drifted"
  run_capture "init" 0 "$results/init.json" "$results/init.stderr" assay init --preset dev --hello-trace --format json
  "$JQ_BIN" -e '.schema == "assay.init_report.v0"' "$results/init.json" >/dev/null || fail "init output identity drifted"
}
