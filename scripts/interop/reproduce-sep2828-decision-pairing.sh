#!/usr/bin/env bash
# Reproduce the upstream SEP-2828 decision/outcome pairing conformance vectors with Assay's
# independent consumer verifier, and print how each case compares.
#
# The vectors are published by vaaraio/vaara under AGPL-3.0-or-later. They are FETCHED at run time
# and never vendored into this MIT repository. Nothing from upstream is executed: the upstream
# checker is not run, only its committed JSON is read, and every verdict below is computed by
# `assay` from the wire bytes.
#
# The upstream revision is PINNED. A reproduction record whose inputs can move is not a record, so
# the default is an immutable commit and a drift shows up as a fetch failure rather than as a
# silently different result. Point ASSAY_INTEROP_REV at another commit to re-run against it.
#
# Usage:  scripts/interop/reproduce-sep2828-decision-pairing.sh [path-to-assay-binary]
# Exit:   0 when every case matches the result recorded in
#         docs/interop/sep2828-decision-pairing-v0.md, 1 when it has drifted, 2 on a setup or
#         fetch failure. A fetch or tool failure is never reported as a comparison result.
set -euo pipefail

ASSAY="${1:-./target/release/assay}"
REV="${ASSAY_INTEROP_REV:-9fefe51a61f16dc13cd64ca8ca4b8792e48fb64b}"
UPSTREAM="https://raw.githubusercontent.com/vaaraio/vaara/${REV}/tests/vectors/decision_pairing_v0/normative"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
FALLBACK_CLASSIFIER="$SCRIPT_DIR/classify_sep2828_fallback.py"
MAX_INPUT_BYTES=1048576
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

die() { echo "$*" >&2; exit 2; }

[ -x "$ASSAY" ] || command -v "$ASSAY" >/dev/null 2>&1 \
  || die "assay binary not found at '$ASSAY'. Build it with: cargo build -p assay-cli --release"
command -v curl >/dev/null 2>&1 || die "curl is required"
command -v python3 >/dev/null 2>&1 || die "python3 is required"
[ -f "$FALLBACK_CLASSIFIER" ] || die "fallback classifier not found at '$FALLBACK_CLASSIFIER'"

echo "SEP-2828 decision_pairing_v0, reproduced by Assay as an independent consumer"
echo "Vectors: vaaraio/vaara (AGPL-3.0-or-later), fetched, not vendored, not executed"
echo "Upstream revision: ${REV}"
echo "Toolchain:         $("$ASSAY" --version 2>/dev/null || echo 'assay ?')"
echo "                   $(curl --version 2>/dev/null | head -1 | cut -d' ' -f1-2)"
echo "                   $(python3 --version 2>&1)"
echo

pass=0
diverge=0
non_reproduced=0

# A missing or empty fetch is a setup failure, never an absent optional input.
fetch() { # case, file
  curl -sfL --max-time 30 --max-filesize "$MAX_INPUT_BYTES" \
    "$UPSTREAM/$1/$2" -o "$WORK/$2" \
    || die "failed to fetch $1/$2 at $REV within $MAX_INPUT_BYTES bytes"
  [ -s "$WORK/$2" ] || die "empty response for $1/$2 at $REV"
  [ "$(wc -c <"$WORK/$2")" -le "$MAX_INPUT_BYTES" ] \
    || die "oversized response for $1/$2 at $REV"
}

# Runs the verifier and echoes the ids of the checks that passed and failed. Exit code 2 means a
# case was correctly refused, so only a code outside {0,2} is a tool failure.
run_pairing() { # case, has_receipt
  local case="$1" has_receipt="$2" rc=0 out
  local args=(evidence verify-mcp-records --decision "$WORK/decision.json" --format json)
  if [ -s "$WORK/attestation.json" ]; then
    args=(evidence verify-mcp-records --attestation "$WORK/attestation.json" \
          --decision "$WORK/decision.json" --format json)
  fi
  [ "$has_receipt" = yes ] && args+=(--outcome "$WORK/receipt.json")
  out="$("$ASSAY" "${args[@]}" 2>/dev/null)" || rc=$?
  case "$rc" in
    0|2) ;;
    *) die "assay exited $rc on $case" ;;
  esac
  printf '%s' "$out"
}

# Compares against the check ids each upstream case pins, not just the overall boolean, so a case
# that fails for the wrong reason is a divergence rather than a match.
compare() { # case, report_json, expect_ok, must_pass (csv), must_fail (csv), note
  local case="$1" verdict
  verdict="$(printf '%s' "$2" | python3 -c '
import json,sys
r=json.load(sys.stdin)
want_ok, must_pass, must_fail = sys.argv[1]=="true", sys.argv[2], sys.argv[3]
checks={c["id"]: c["ok"] for c in r["checks"]}
bad=[]
if r["ok"] is not want_ok: bad.append("ok=%s" % r["ok"])
for cid in [c for c in must_pass.split(",") if c]:
    if checks.get(cid) is not True: bad.append("%s not passing" % cid)
for cid in [c for c in must_fail.split(",") if c]:
    if checks.get(cid) is not False: bad.append("%s not failing" % cid)
print("ok" if not bad else "; ".join(bad))
' "$3" "$4" "$5")"
  if [ "$verdict" = ok ]; then
    printf '  %-46s reproduced   (%s)\n' "$case" "$6"
    pass=$((pass + 1))
  else
    printf '  %-46s DIVERGES     %s  (%s)\n' "$case" "$verdict" "$6"
    diverge=$((diverge + 1))
  fi
}

case_run() { # case, has_attestation, has_receipt, expect_ok, must_pass, must_fail, note
  rm -f "$WORK"/*.json
  [ "$2" = yes ] && fetch "$1" attestation.json
  fetch "$1" decision.json
  [ "$3" = yes ] && fetch "$1" receipt.json
  compare "$1" "$(run_pairing "$1" "$3")" "$4" "$5" "$6" "$7"
}

case_run valid_pair_allow_executed yes yes true \
  decision_outcome_backlink_match,outcome_decision_digest_match,result_commitment_projection_digest_match \
  "" "Check A and Check B both hold"

case_run decision_only_escalate yes no true \
  decision_attestation_digest_match,outcome_absent "" "decision with no outcome yet"

case_run substituted_attestation_backlink yes yes false \
  decision_attestation_digest_match \
  outcome_attestation_digest_match,decision_outcome_backlink_match \
  "Check A fails on the attestation digest"

case_run substituted_pairing_nonce yes yes false \
  outcome_attestation_digest_match \
  outcome_attestation_nonce_match,decision_outcome_backlink_match \
  "Check A fails on the nonce, digest still matches"

case_run substituted_decision_under_shared_attestation yes yes false \
  decision_outcome_backlink_match outcome_decision_digest_match \
  "Check A holds, Check B fails"

# Supersession is a separate command: it reasons over a set of decisions sharing one back-link,
# not over a decision and outcome pair. The reason code is compared, not only the verdict, so a
# tie broken for the wrong reason would not read as a match.
rm -f "$WORK"/*.json
fetch supersession_equal_decidedat_tie decision_a.json
fetch supersession_equal_decidedat_tie decision_b.json
python3 -c '
import json,sys
json.dump([json.load(open(sys.argv[1])), json.load(open(sys.argv[2]))], open(sys.argv[3],"w"))' \
  "$WORK/decision_a.json" "$WORK/decision_b.json" "$WORK/pair.json"
sup_rc=0
sup_out="$("$ASSAY" evidence verify-mcp-supersession --decisions "$WORK/pair.json" --format json 2>/dev/null)" || sup_rc=$?
case "$sup_rc" in 0|2) ;; *) die "assay exited $sup_rc on supersession" ;; esac
sup_verdict="$(printf '%s' "$sup_out" | python3 -c '
import json,sys
g=json.load(sys.stdin)["groups"][0]
print("%s/%s" % (g["verdict"], g["reason_code"]))')"
if [ "$sup_verdict" = "ambiguous/supersession_ambiguous_missing_sequence" ]; then
  printf '  %-46s reproduced   (%s)\n' supersession_equal_decidedat_tie "equal decidedAt, no ordering field"
  pass=$((pass + 1))
else
  printf '  %-46s DIVERGES     got=%s\n' supersession_equal_decidedat_tie "$sup_verdict"
  diverge=$((diverge + 1))
fi

# The fallback case uses a different named projection in each implementation. Execute it rather
# than printing a fixed conclusion: only the exact recorded projection mismatch is a documented
# non-reproduction. A different failure or a new success is drift that must update this record.
rm -f "$WORK"/*.json
fetch fallback_envelope_binding request_envelope.json
fetch fallback_envelope_binding decision.json
fetch fallback_envelope_binding receipt.json
fallback_rc=0
"$ASSAY" evidence verify-mcp-records \
  --request-envelope "$WORK/request_envelope.json" \
  --decision "$WORK/decision.json" \
  --outcome "$WORK/receipt.json" \
  --fallback-projection named \
  --format json >"$WORK/fallback_report.json" 2>/dev/null || fallback_rc=$?
case "$fallback_rc" in 0|2) ;; *) die "assay exited $fallback_rc on fallback_envelope_binding" ;; esac
upstream_projection_json="$(python3 -c '
import json,sys
d=json.load(sys.stdin)
print(json.dumps(d.get("backLink", {}).get("fallbackProjection"), separators=(",", ":")))
' <"$WORK/decision.json")" || die "could not read upstream fallback projection"
fallback_classification="$(python3 "$FALLBACK_CLASSIFIER" "$upstream_projection_json" \
  <"$WORK/fallback_report.json")" \
  || die "could not classify fallback_envelope_binding"
fallback_disposition="$(printf '%s' "$fallback_classification" | python3 -c \
  'import json,sys; print(json.load(sys.stdin)["classification"])')"
case "$fallback_disposition" in
  documented_non_reproduction)
    printf '  %-46s not reproduced (explicit named-projection mismatch)\n' fallback_envelope_binding
    non_reproduced=$((non_reproduced + 1))
    ;;
  reproduced)
    printf '  %-46s DIVERGES     now reproduced; update the recorded result\n' fallback_envelope_binding
    diverge=$((diverge + 1))
    ;;
  *)
    printf '  %-46s DIVERGES     unexpected fallback failure\n' fallback_envelope_binding
    diverge=$((diverge + 1))
    ;;
esac

echo
echo "reproduced: $pass   diverged: $diverge   documented non-reproduction: $non_reproduced"
if [ "$diverge" -ne 0 ] || [ "$pass" -ne 6 ] || [ "$non_reproduced" -ne 1 ]; then
  echo "Result differs from the recorded 6 of 7. Update docs/interop/sep2828-decision-pairing-v0.md." >&2
  exit 1
fi
