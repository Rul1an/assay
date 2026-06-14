#!/usr/bin/env bash
# Produce a SARIF 2.1.0 report from the privileged-action-gate scenarios: run them through the
# enforcing proxy, collect the assay.enforcement_decision.v0 records, and project the denies to
# SARIF for the GitHub Security tab. Offline; no real credentials, no real GitHub call.
#
#   ./sarif.sh [output.sarif]   (default: enforcement.sarif)
set -euo pipefail
cd "$(dirname "$0")"

if [ -x "../../target/debug/assay-mcp-server" ]; then
  ASSAY="$(cd ../.. && pwd)/target/debug/assay-mcp-server"
elif command -v assay-mcp-server >/dev/null 2>&1; then
  ASSAY="assay-mcp-server"
else
  echo "building assay-mcp-server (first run)..." >&2
  (cd ../.. && cargo build -q -p assay-mcp-server)
  ASSAY="$(cd ../.. && pwd)/target/debug/assay-mcp-server"
fi

PY="${PYTHON:-python3}"
OUT="${1:-enforcement.sarif}"
DEC="$(mktemp)"
trap 'rm -f "$DEC"' EXIT

INIT='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"demo","version":"1"}}}'
CALL='{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"github.add_deploy_key","arguments":{"owner":"acme","repo":"prod-app"}}}'

# emit <policy> <baseline> <mock_mode> — appends one enforcement_decision.v0 record to $DEC.
emit() {
  printf '%s\n%s\n' "$INIT" "$CALL" \
    | MOCK_MODE="$3" "$ASSAY" proxy-enforce \
        --upstream-command "$PY" --upstream-arg -u --upstream-arg "mock_github_mcp.py" \
        --enforce-policy "$1" --declared-mcp-manifest "$2" \
        --enforcement-decision-out "$DEC" \
        >/dev/null 2>&1 || true
}

emit policies/no-allowance.yaml           baseline-approved.json           approved
emit policies/insufficient-credential.yaml baseline-approved.json          approved
emit policies/allow.yaml                  baseline-approved.json           drifted
emit policies/allow.yaml                  baseline-approved.json           approved
emit policies/allow.yaml                  baseline-approved-readonly.json  drifted

if [ ! -s "$DEC" ]; then
  echo "ERROR: no enforcement_decision records produced (build assay-mcp-server, ensure python3)" >&2
  exit 1
fi

"$ASSAY" enforcement-sarif --input "$DEC" --output "$OUT"
n="$("$PY" -c "import json,sys; print(len(json.load(open('$OUT'))['runs'][0]['results']))")"
echo "wrote $OUT ($n SARIF result(s) — one per denied privileged action)"
