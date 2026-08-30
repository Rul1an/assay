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
