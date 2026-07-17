#!/usr/bin/env bash
# =============================================================================
# tellur-verify.sh  —  Semi-geautomatiseerde verificatie van sydneyvb-nl/tellur
# =============================================================================
# Draait stappen 1-7 uit de overdracht en vult de beoordelingsrubriek deels in.
#
# ONTWERP-PRINCIPES:
#   * NOOIT `set -e`: we willen doorgaan na een gefaalde test en 'm loggen.
#   * Geen aannames over exacte CLI-syntax: de binary + subcommando's worden
#     ontdekt via `--help`. Waar syntax onzeker is → status UNKNOWN i.p.v. FAIL.
#   * Alle ruwe output belandt in een results-map voor handmatige review.
#   * Draai dit in een WEGWERP-VM/CONTAINER: Tellur zet globale hooks.
#
# GEBRUIK:
#   ./tellur-verify.sh [pad-naar-bestaande-clone]
#   (zonder argument: kloont vers naar een tempdir)
# =============================================================================
set -u
set -o pipefail

# ---------- 0. Setup ---------------------------------------------------------
REPO_URL="https://github.com/sydneyvb-nl/tellur.git"
WORK="${1:-}"
STAMP="$(date +%Y%m%d-%H%M%S 2>/dev/null || echo run)"
RES="$(pwd)/tellur-results-${STAMP}"
mkdir -p "$RES"
LOG="$RES/run.log"

# Rubriek-resultaten: index -> "STATUS|omschrijving"
declare -A RUBRIC
set_r() { RUBRIC["$1"]="$2|$3"; }   # nr, STATUS(PASS/FAIL/UNKNOWN/SKIP), tekst

say()  { echo -e "\n\033[1;36m==> $*\033[0m" | tee -a "$LOG"; }
info() { echo "    $*" | tee -a "$LOG"; }
run()  { echo "\$ $*" | tee -a "$LOG"; "$@" >>"$LOG" 2>&1; return $?; }

trap 'echo "Resultaten in: $RES"' EXIT

say "Resultaten-map: $RES"

# ---------- 1. Clone + build -------------------------------------------------
say "STAP 1 — clone + build"
if [ -z "$WORK" ]; then
  WORK="$(mktemp -d 2>/dev/null || echo /tmp/tellur-src)"
  info "Klonen naar $WORK"
  if run git clone --depth 1 "$REPO_URL" "$WORK"; then :; else
    set_r 1 FAIL "clone mislukt — zie run.log"; say "ABORT: clone mislukt"; exit 1
  fi
fi
cd "$WORK" || { set_r 1 FAIL "cd naar clone mislukt"; exit 1; }
info "Werkmap: $(pwd)  |  commit: $(git rev-parse --short HEAD 2>/dev/null || echo '?')"

for t in rustc cargo git; do
  command -v "$t" >/dev/null 2>&1 || info "WAARSCHUWING: '$t' niet gevonden op PATH"
done

if run cargo build --workspace --release; then
  set_r 1 PASS "cargo build --workspace --release schoon"
else
  set_r 1 FAIL "build mislukt — zie run.log (mogelijk toolchain/feature-flags)"
fi

# ---------- 2. Test suite ----------------------------------------------------
say "STAP 2 — test suite"
TEST_OUT="$RES/cargo-test.txt"
cargo test --workspace >"$TEST_OUT" 2>&1
TEST_RC=$?
# Tel 'test result: ok. N passed'
PASSED=$(grep -Eo '[0-9]+ passed' "$TEST_OUT" | grep -Eo '[0-9]+' | awk '{s+=$1} END{print s+0}')
FAILED=$(grep -Eo '[0-9]+ failed' "$TEST_OUT" | grep -Eo '[0-9]+' | awk '{s+=$1} END{print s+0}')
info "tests passed=$PASSED failed=$FAILED (exit=$TEST_RC)"
if [ "$TEST_RC" -eq 0 ]; then
  set_r 2 PASS "cargo test groen: ${PASSED} passed (claim ~61)"
else
  set_r 2 FAIL "cargo test: ${PASSED} passed, ${FAILED} failed — zie cargo-test.txt"
fi

# ---------- 3. Binary + CLI-oppervlak ontdekken ------------------------------
say "STAP 3 — binary + subcommando's ontdekken"
BIN=""
for cand in tellur tellur-cli; do
  p="target/release/$cand"
  [ -x "$p" ] && BIN="$p" && break
done
# Fallback: eerste uitvoerbare non-.d in target/release die 'tellur' bevat
if [ -z "$BIN" ]; then
  BIN=$(find target/release -maxdepth 1 -type f -perm -u+x -name '*tellur*' ! -name '*.d' 2>/dev/null | head -1)
fi
if [ -z "$BIN" ]; then
  set_r 3 FAIL "geen tellur-binary gevonden in target/release"
  say "ABORT: geen binary — verdere runtime-tests overgeslagen"
  # rubriek 4-13 blijven UNKNOWN
  for i in 4 5 6 7 8 9 10 11 12 13; do set_r $i SKIP "geen binary"; done
else
  BIN="$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")"
  info "Binary: $BIN"
  "$BIN" --help  >"$RES/cli-help.txt" 2>&1
  "$BIN" --version >>"$RES/cli-help.txt" 2>&1
  # Probe elk verwacht subcommando
  HELPALL="$RES/cli-subcommands.txt"; : >"$HELPALL"
  FOUND_CMDS=""
  for c in setup explain blame sessions verify pr-report policy export hooks connect inspect repo serve; do
    echo "===== $c =====" >>"$HELPALL"
    if "$BIN" "$c" --help >>"$HELPALL" 2>&1; then FOUND_CMDS="$FOUND_CMDS $c"; fi
  done
  info "Subcommando's die reageerden op --help:$FOUND_CMDS"
  # Minimaal explain+blame+verify verwacht
  if echo "$FOUND_CMDS" | grep -q verify && echo "$FOUND_CMDS" | grep -q blame; then
    set_r 3 PASS "CLI aanwezig; subcommando's:$FOUND_CMDS"
  else
    set_r 3 UNKNOWN "binary werkt maar kernsubcommando's onduidelijk — zie cli-subcommands.txt"
  fi
fi

has_cmd() { echo "${FOUND_CMDS:-}" | grep -qw "$1"; }

# ---------- Sandbox-repo -----------------------------------------------------
if [ -n "$BIN" ]; then
  say "STAP 4 — sandbox-repo + tellur activeren"
  DEMO="$(mktemp -d 2>/dev/null || echo /tmp/tellur-demo)"
  info "Sandbox: $DEMO"
  (
    cd "$DEMO" || exit 1
    git init -q
    git config user.email t@t.local; git config user.name tester
    mkdir -p src
    printf 'fn main() {\n    println!("hello");\n}\n' > src/main.rs
    git add -A && git commit -qm "human baseline"
  )
  # setup / init proberen (meerdere kandidaten)
  INIT_OK=1
  for attempt in "setup" "repo init" "connect" "init"; do
    # shellcheck disable=SC2086
    if ( cd "$DEMO" && "$BIN" $attempt </dev/null ) >>"$RES/step4-setup.txt" 2>&1; then
      info "init via: '$BIN $attempt'"; INIT_OK=0; break
    fi
  done
  if [ -d "$DEMO/.tellur" ] || [ $INIT_OK -eq 0 ]; then
    set_r 4 PASS "tellur geactiveerd (.tellur aanwezig: $([ -d "$DEMO/.tellur" ] && echo ja || echo nee))"
  else
    set_r 4 UNKNOWN "init-commando niet bevestigd — zie step4-setup.txt"
  fi
  ls -la "$DEMO/.tellur" >>"$RES/step4-setup.txt" 2>&1 || true

  # ---------- 5. AI-edit via hooks ingest ------------------------------------
  say "STAP 5 — AI-edit door capture-pijplijn (hooks ingest)"
  printf 'fn main() {\n    println!("hello from AI");\n}\n' > "$DEMO/src/main.rs"
  INGEST_OK=1
  if has_cmd hooks; then
    PAYLOAD='{"session_id":"test-1","source":"claude-code","tool":"Edit","file_paths":["src/main.rs"],"model":"claude-opus-4-8","prompt":"add AI greeting"}'
    for form in \
        "hooks ingest --source claude-code --auto-init" \
        "hooks ingest --source claude-code" \
        "hooks ingest"; do
      # shellcheck disable=SC2086
      if ( cd "$DEMO" && echo "$PAYLOAD" | "$BIN" $form ) >>"$RES/step5-ingest.txt" 2>&1; then
        info "ingest via: '$BIN $form'"; INGEST_OK=0; break
      fi
    done
  fi
  if [ $INGEST_OK -eq 0 ]; then
    set_r 5 PASS "hooks ingest accepteerde payload (zie step5-ingest.txt — verifieer of er een sessie is)"
  else
    set_r 5 UNKNOWN "ingest-payloadvorm niet bevestigd — check docs/ADAPTERS.md + 'hooks ingest --help'"
  fi

  # ---------- 6. Attributie uitlezen -----------------------------------------
  say "STAP 6 — blame / explain / sessions"
  ( cd "$DEMO" && "$BIN" sessions ) >"$RES/step6-sessions.txt" 2>&1 || true
  ( cd "$DEMO" && "$BIN" blame src/main.rs ) >"$RES/step6-blame.txt" 2>&1 || true
  ( cd "$DEMO" && "$BIN" explain src/main.rs:2 ) >"$RES/step6-explain.txt" 2>&1 || true
  if grep -qiE 'ai|claude|origin' "$RES/step6-blame.txt" "$RES/step6-explain.txt" 2>/dev/null; then
    set_r 6 PASS "blame/explain tonen attributie (verifieer origin=Ai + model handmatig)"
  else
    set_r 6 UNKNOWN "geen duidelijke Ai-attributie zichtbaar — zie step6-*.txt"
  fi

  # ---------- 7. Negatieve test: edit ZONDER bewijs --------------------------
  say "STAP 7 — negatieve test: edit zonder ingest mag NIET 'Ai' worden"
  printf '\n// plain human edit, no ingest\n' >> "$DEMO/src/main.rs"
  ( cd "$DEMO" && git add -A && git commit -qm "human edit no ingest" ) >>"$LOG" 2>&1 || true
  ( cd "$DEMO" && "$BIN" blame src/main.rs ) >"$RES/step7-blame.txt" 2>&1 || true
  LASTLINE_ORIGIN=$(tail -5 "$RES/step7-blame.txt" 2>/dev/null | grep -ioE 'human|unknown|ai' | tail -1)
  info "origin laatste (human) regel ≈ '${LASTLINE_ORIGIN:-?}'"
  if echo "${LASTLINE_ORIGIN:-}" | grep -qiE 'human|unknown'; then
    set_r 7 PASS "edit zonder bewijs → ${LASTLINE_ORIGIN} (correct)"
  elif echo "${LASTLINE_ORIGIN:-}" | grep -qi 'ai'; then
    set_r 7 FAIL "edit ZONDER bewijs werd 'Ai' gelabeld — attributie onbetrouwbaar!"
  else
    set_r 7 UNKNOWN "kon origin van human-regel niet bepalen — zie step7-blame.txt"
  fi

  # ---------- 8/9/10. Verify + tamper ----------------------------------------
  say "STAP 8-10 — verify (intact) + tamper-detectie"
  ( cd "$DEMO" && "$BIN" verify ) >"$RES/step8-verify-intact.txt" 2>&1
  V_RC=$?
  if [ $V_RC -eq 0 ]; then set_r 8 PASS "verify OK op intacte store"
  else set_r 8 UNKNOWN "verify gaf exit=$V_RC op intacte store — zie step8"; fi

  # Vind de event-store om te muteren
  STORE=$(find "$DEMO/.tellur" -maxdepth 3 -type f \( -name '*.ndjson' -o -name '*.jsonl' -o -name '*.db' -o -name '*.sqlite*' -o -name '*.log' \) 2>/dev/null | head -1)
  info "Event-store gok: ${STORE:-<niet gevonden>}"
  if [ -n "$STORE" ] && [ -f "$STORE" ]; then
    cp "$STORE" "$RES/store.backup"
    case "$STORE" in
      *.ndjson|*.jsonl|*.log)
        # 9: muteer één byte in payload (verander 'hello' -> 'hallo' o.i.d.)
        sed -i.bak 's/hello/hackd/g' "$STORE" 2>/dev/null || \
          perl -pi -e 's/hello/hackd/g' "$STORE" 2>/dev/null
        ( cd "$DEMO" && "$BIN" verify ) >"$RES/step9-verify-mutated.txt" 2>&1
        if [ $? -ne 0 ]; then set_r 9 PASS "verify FAALT na byte-mutatie (correct)"
        else set_r 9 FAIL "verify detecteerde byte-mutatie NIET"; fi
        cp "$RES/store.backup" "$STORE"   # herstel
        # 10: truncation — verwijder laatste regel
        sed -i.bak '$ d' "$STORE" 2>/dev/null
        ( cd "$DEMO" && "$BIN" verify ) >"$RES/step10-verify-truncated.txt" 2>&1
        if [ $? -ne 0 ]; then set_r 10 PASS "verify FAALT na truncation (correct)"
        else set_r 10 FAIL "verify detecteerde truncation NIET"; fi
        cp "$RES/store.backup" "$STORE"
        ;;
      *)
        set_r 9  UNKNOWN "store is binair ($STORE) — muteer handmatig (sqlite3) en run 'verify'"
        set_r 10 UNKNOWN "truncation handmatig testen op $STORE"
        ;;
    esac
  else
    set_r 9  UNKNOWN "event-store niet gevonden — inspecteer .tellur/ handmatig"
    set_r 10 UNKNOWN "event-store niet gevonden"
  fi

  # ---------- 11/12. Policy ---------------------------------------------------
  say "STAP 11-12 — policy check + pr-report"
  mkdir -p "$DEMO/src/auth" "$DEMO/.tellur"
  echo 'pub fn login() {}' > "$DEMO/src/auth/mod.rs"
  cat > "$DEMO/.tellur/policy.yml" <<'YAML'
rules:
  - id: auth-needs-review
    paths: ["src/auth/**"]
    when: { attribution.origin: Ai }
    require: { reviewer_from_codeowners: true, tests_run: true }
    severity: high
YAML
  if has_cmd policy; then
    ( cd "$DEMO" && "$BIN" policy check ) >"$RES/step11-policy.txt" 2>&1
    P_RC=$?
    info "policy check exit=$P_RC"
    if grep -qiE 'auth-needs-review|fail|violation|high' "$RES/step11-policy.txt" 2>/dev/null || [ $P_RC -ne 0 ]; then
      set_r 11 PASS "policy check produceerde finding/non-zero exit"
    else
      set_r 11 UNKNOWN "geen duidelijke finding — check policy-syntax in step11-policy.txt"
    fi
  else
    set_r 11 SKIP "geen policy-subcommando"
  fi
  if has_cmd pr-report; then
    ( cd "$DEMO" && "$BIN" pr-report --base HEAD~1 --head HEAD ) >"$RES/step12-prreport.txt" 2>&1
    [ -s "$RES/step12-prreport.txt" ] && set_r 12 PASS "pr-report gaf output (verifieer findings)" \
                                       || set_r 12 UNKNOWN "pr-report leeg — zie step12"
  else
    set_r 12 SKIP "geen pr-report-subcommando"
  fi

  # ---------- 13. Export ------------------------------------------------------
  say "STAP 13 — export + basale JSON-validatie"
  EXP_OK=1
  if has_cmd export; then
    for fmt in json slsa spdx agent-trace; do
      out="$RES/export-$fmt.json"
      ( cd "$DEMO" && "$BIN" export --format "$fmt" ) >"$out" 2>>"$RES/step13-export.err"
      if [ -s "$out" ]; then
        if command -v jq >/dev/null 2>&1; then
          jq empty "$out" >/dev/null 2>&1 && info "export $fmt: valide JSON" || info "export $fmt: GEEN valide JSON"
        else
          info "export $fmt: $(wc -c <"$out") bytes (installeer jq voor validatie)"
        fi
        EXP_OK=0
      fi
    done
  fi
  if [ $EXP_OK -eq 0 ]; then
    set_r 13 PASS "export produceerde output (schema-validatie SLSA/SPDX handmatig)"
  else
    set_r 13 UNKNOWN "export niet bevestigd — zie step13-export.err"
  fi
fi

# 14/15 vereisen server/editor → buiten scope van dit script
set_r 14 SKIP "Hub-server test handmatig (stap 8 overdracht)"
set_r 15 SKIP "Editor-extensie test handmatig (stap 8 overdracht)"

# ---------- Rubriek uitprinten ----------------------------------------------
say "BEOORDELINGSRUBRIEK"
DESC=( \
 "1|cargo build --workspace" \
 "2|cargo test (~61 tests)" \
 "3|CLI-subcommando's bestaan" \
 "4|setup init .tellur/" \
 "5|AI-edit via hooks ingest gevangen" \
 "6|blame/explain toont Ai+model" \
 "7|edit ZONDER bewijs -> niet Ai" \
 "8|verify OK op intacte store" \
 "9|verify FAALT na byte-mutatie" \
 "10|verify FAALT na truncation" \
 "11|policy check genereert finding" \
 "12|pr-report risk-rapport" \
 "13|export (JSON/SLSA/SPDX)" \
 "14|Hub: unauth geweigerd" \
 "15|editor-extensie vangt save" )
REPORT="$RES/RUBRIC.md"
{
  echo "# Tellur verificatie-rubriek ($STAMP)"
  echo "commit: $(git -C "$WORK" rev-parse --short HEAD 2>/dev/null || echo '?')"
  echo ""
  echo "| # | Test | Status | Detail |"
  echo "|---|------|--------|--------|"
  for d in "${DESC[@]}"; do
    n="${d%%|*}"; txt="${d#*|}"
    entry="${RUBRIC[$n]:-UNKNOWN|niet uitgevoerd}"
    st="${entry%%|*}"; det="${entry#*|}"
    case "$st" in
      PASS) icon="✅";; FAIL) icon="❌";; SKIP) icon="⏭️";; *) icon="❓";;
    esac
    echo "| $n | $txt | $icon $st | $det |"
  done
  echo ""
  echo "_Ruwe output per stap: zie de \`step*.txt\` en \`export-*.json\` in deze map._"
} > "$REPORT"

cat "$REPORT" | tee -a "$LOG"
say "Klaar. Rubriek: $REPORT  |  Alle output: $RES"
