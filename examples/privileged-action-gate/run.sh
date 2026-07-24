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
  # The proxy denies per call (its own non-zero on a denied call is expected and tolerated above),
  # but a missing decision record means the proxy did not run at all — fail loudly rather than print
  # a silent, verdict-less demo with exit 0.
  if [ ! -s "$dec" ]; then
    echo "ERROR: no enforcement_decision record for ${policy} / ${baseline} / MOCK_MODE=${mode}" >&2
    echo "  (build assay-mcp-server, and ensure python3 is on PATH for the mock)" >&2
    exit 1
  fi
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

# ---- open profile: privileged-mcp-action/v0 (import + verify) ---------------------------------
# Two of the scenarios above become evidence bundles under the open profile: the enforcement
# records are imported byte-faithful, then the profile verifier recomputes the claim matrix from
# the carried bytes alone. The matrix is the product; nothing here changes the verdicts above.

# Locate the assay CLI (evidence import/verify), mirroring the proxy lookup. An installed assay
# may predate the profile commands, so the chosen binary is validated before use.
supports_profile() { "$1" evidence verify-privileged-mcp-action --help >/dev/null 2>&1; }
if [ -x "../../target/debug/assay" ] && supports_profile "../../target/debug/assay"; then
  ASSAY_CLI="$(cd ../.. && pwd)/target/debug/assay"
elif command -v assay >/dev/null 2>&1 && supports_profile assay; then
  ASSAY_CLI="assay"
else
  echo "building assay CLI (first run)..." >&2
  (cd ../.. && cargo build -q -p assay-cli)
  ASSAY_CLI="$(cd ../.. && pwd)/target/debug/assay"
fi

# capture <policy> <baseline> <mode> <dec_out> <obs_out>: one scenario, records kept, no output.
capture() {
  local policy="$1" baseline="$2" mode="$3" dec="$4" obs="$5"
  : >"$dec"
  : >"$obs"
  printf '%s\n%s\n' "$INIT" "$CALL" \
    | MOCK_MODE="$mode" "$ASSAY" proxy-enforce \
        --upstream-command "$PY" --upstream-arg -u --upstream-arg "mock_github_mcp.py" \
        --enforce-policy "$policy" \
        --declared-mcp-manifest "$baseline" \
        --enforcement-decision-out "$dec" \
        --denied-call-observation-out "$obs" \
        >/dev/null 2>&1 || true
}

# matrix <label> <bundle>: verify one bundle and print a compact claim-matrix line.
matrix() {
  local label="$1" bundle="$2"
  "$ASSAY_CLI" evidence verify-privileged-mcp-action "$bundle" --format json \
    | "$PY" -c '
import json, sys
label = sys.argv[1]
report = json.load(sys.stdin)
cells = " ".join("{}={}".format(name, cell["status"]) for name, cell in report["claims"].items())
print("  {}: verdict={}  {}".format(label, report["verdict"], cells))
' "$label"
}

echo
echo "Open profile privileged-mcp-action/v0: import the records, recompute the claim matrix."
capture policies/no-allowance.yaml baseline-approved.json approved "$WORK/pmadeny.ndjson" "$WORK/pmadeny-obs.ndjson"
capture policies/allow.yaml        baseline-approved.json approved "$WORK/pmaallow.ndjson" "$WORK/pmaallow-obs.ndjson"

"$ASSAY_CLI" evidence import privileged-mcp-action \
  --decisions "$WORK/pmadeny.ndjson" --denied-observations "$WORK/pmadeny-obs.ndjson" \
  --bundle-out "$WORK/pmadeny.bundle.tar.gz" 2>/dev/null
"$ASSAY_CLI" evidence import privileged-mcp-action \
  --decisions "$WORK/pmaallow.ndjson" \
  --bundle-out "$WORK/pmaallow.bundle.tar.gz" 2>/dev/null

matrix "denied path bundle " "$WORK/pmadeny.bundle.tar.gz"
matrix "allowed path bundle" "$WORK/pmaallow.bundle.tar.gz"
echo "The matrix is recomputed from carried bytes; delivery and side effect stay incomplete by design."
