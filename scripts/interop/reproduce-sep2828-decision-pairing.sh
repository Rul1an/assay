#!/usr/bin/env bash
# Reproduce the upstream SEP-2828 decision/outcome pairing conformance vectors with Assay's
# independent consumer verifier, and print how each case compares.
#
# The vectors are published by vaaraio/vaara under AGPL-3.0-or-later. They are FETCHED at run time
# and never vendored into this MIT repository. Nothing from upstream is executed: the upstream
# checker is not run, only its committed JSON is read, and every verdict below is computed by
# `assay` from the wire bytes.
#
# Usage:  scripts/interop/reproduce-sep2828-decision-pairing.sh [path-to-assay-binary]
# Exit:   0 when the comparison matches the recorded result in
#         docs/interop/sep2828-decision-pairing-v0.md, 1 when it has drifted.
set -euo pipefail

ASSAY="${1:-./target/release/assay}"
UPSTREAM="https://raw.githubusercontent.com/vaaraio/vaara/main/tests/vectors/decision_pairing_v0/normative"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

if ! command -v "$ASSAY" >/dev/null 2>&1 && [ ! -x "$ASSAY" ]; then
  echo "assay binary not found at '$ASSAY'. Build it with: cargo build -p assay-cli --release" >&2
  exit 2
fi

fetch() { # case, file
  curl -sfL "$UPSTREAM/$1/$2" -o "$WORK/$2" 2>/dev/null
}

pass=0
diverge=0

pairing() { # case, expected_ok, note
  local case="$1" want="$2" note="$3"
  mkdir -p "$WORK" && rm -f "$WORK"/*.json
  fetch "$case" attestation.json || true
  fetch "$case" decision.json
  local args=(evidence verify-mcp-records --attestation "$WORK/attestation.json" \
              --decision "$WORK/decision.json" --format json)
  if fetch "$case" receipt.json; then
    args+=(--outcome "$WORK/receipt.json")
  fi
  # A correctly-failing case exits 2 by design, so the exit code is not an error here.
  local out got
  out="$("$ASSAY" "${args[@]}" 2>/dev/null || true)"
  got="$(printf '%s' "$out" | python3 -c 'import json,sys; print(json.load(sys.stdin)["ok"])')"
  report "$case" "$want" "$got" "$note"
}

report() { # case, want, got, note
  if [ "$2" = "$3" ]; then
    printf '  %-46s reproduced   (%s)\n' "$1" "$4"
    pass=$((pass + 1))
  else
    printf '  %-46s DIVERGES     want=%s got=%s  (%s)\n' "$1" "$2" "$3" "$4"
    diverge=$((diverge + 1))
  fi
}

echo "SEP-2828 decision_pairing_v0, reproduced by Assay as an independent consumer"
echo "Vectors: vaaraio/vaara (AGPL-3.0-or-later), fetched, not vendored, not executed"
echo

pairing valid_pair_allow_executed                    True  "Check A and Check B both hold"
pairing decision_only_escalate                       True  "decision with no outcome yet"
pairing substituted_attestation_backlink             False "Check A fails on the attestation digest"
pairing substituted_pairing_nonce                    False "Check A fails on the nonce"
pairing substituted_decision_under_shared_attestation False "Check A holds, Check B fails"

# Supersession is a separate command: it reasons over a set of decisions sharing one back-link,
# not over a decision/outcome pair.
rm -f "$WORK"/*.json
fetch supersession_equal_decidedat_tie decision_a.json
fetch supersession_equal_decidedat_tie decision_b.json
python3 -c '
import json,sys
a=json.load(open(sys.argv[1])); b=json.load(open(sys.argv[2]))
json.dump([a,b],open(sys.argv[3],"w"))' "$WORK/decision_a.json" "$WORK/decision_b.json" "$WORK/pair.json"
supersession_out="$("$ASSAY" evidence verify-mcp-supersession --decisions "$WORK/pair.json" --format json 2>/dev/null || true)"
verdict="$(printf '%s' "$supersession_out" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["groups"][0]["verdict"])')"
report supersession_equal_decidedat_tie ambiguous "$verdict" "equal decidedAt, no ordering field"

# The fallback case is a known, documented divergence: Assay implements its own named projection
# (assay.fallback_projection.v0) and the upstream projection's pre-image cannot be reconstructed
# from the published specification text. See docs/interop/sep2828-decision-pairing-v0.md.
printf '  %-46s not reproduced (see the record: projection pre-image)\n' fallback_envelope_binding

echo
echo "reproduced: $pass   diverged: $diverge   documented non-reproduction: 1"
if [ "$diverge" -ne 0 ]; then
  echo "Result differs from the recorded 6 of 7. Update docs/interop/sep2828-decision-pairing-v0.md." >&2
  exit 1
fi
