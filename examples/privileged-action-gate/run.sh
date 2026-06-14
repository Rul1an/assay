#!/usr/bin/env bash
# privileged-action-gate — one command, offline.
#
# An agent reaches a GitHub MCP server through the enforcing proxy and tries github.add_deploy_key
# (a privileged write). The proxy decides per call BEFORE forwarding and writes a replayable
# assay.enforcement_decision.v0 record. Five scenarios show the three deny axes, the allowed path,
# and the separate (non-gating) conformance signal. Everything runs against a local mock: no real
# credentials, no real GitHub call.
set -euo pipefail
cd "$(dirname "$0")"

# Locate the enforcing proxy. Prefer a build in this repo; otherwise an installed binary; else build.
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
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

INIT='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"demo","version":"1"}}}'
CALL='{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"github.add_deploy_key","arguments":{"owner":"acme","repo":"prod-app"}}}'

# run <policy> <baseline> <mock_mode>
run() {
  local policy="$1" baseline="$2" mode="$3"
  local dec="$WORK/decision.ndjson" conf="$WORK/conformance.ndjson"
  : >"$dec"
  : >"$conf"
  # No client tools/list: the proxy's bounded pre-call establish does the one re-list (internal and
  # synchronous), so the observation is deterministic and free of client/establish list races.
  printf '%s\n%s\n' "$INIT" "$CALL" \
    | MOCK_MODE="$mode" "$ASSAY" proxy-enforce \
        --upstream-command "$PY" --upstream-arg -u --upstream-arg "mock_github_mcp.py" \
        --enforce-policy "$policy" \
        --declared-mcp-manifest "$baseline" \
        --enforcement-decision-out "$dec" \
        --tool-conformance-out "$conf" \
        >/dev/null 2>&1 || true
  "$PY" - "$dec" "$conf" <<'PYEOF'
import json, sys
dec, conf = sys.argv[1], sys.argv[2]
rows = [json.loads(x) for x in open(dec) if x.strip()]
mism = ""
try:
    for c in (json.loads(x) for x in open(conf) if x.strip()):
        if c.get("conformance") == "mismatched":
            mism = f"  + conformance: mismatched ({c.get('mismatch_kind')})  [separate, non-gating]"
except FileNotFoundError:
    pass
for r in rows:
    d = r.get("decision")
    name = r.get("tool", {}).get("name", "?")
    reason = r.get("reason")
    mark = "✅ ALLOW" if d == "allow" else "❌ DENY "
    print(f"{mark}  {name}  reason={reason}{mism if d == 'allow' else ''}")
PYEOF
}

echo "Privileged action under review: github.add_deploy_key on acme/prod-app"
echo
run policies/no-allowance.yaml           baseline-approved.json           approved
run policies/insufficient-credential.yaml baseline-approved.json          approved
run policies/allow.yaml                  baseline-approved.json           drifted
run policies/allow.yaml                  baseline-approved.json           approved
run policies/allow.yaml                  baseline-approved-readonly.json  drifted
echo
echo "Each call wrote an assay.enforcement_decision.v0 record (replayable)."
echo "Non-claims: a deny is fail-closed caution, not a verdict on intent; an allow is the decision to"
echo "forward, never proof the action happened; the mock performs no real GitHub call; the conformance"
echo "signal is recorded beside the verdict and never changes or gates it."
