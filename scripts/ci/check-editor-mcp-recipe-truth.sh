#!/usr/bin/env bash
# The executable editor MCP recipe must retain its prerequisites and shipped transport boundary.
#
# WHY THIS EXISTS (#2360)
#
# `docs/guides/editor-mcp-recipe.md` is an *executable* recipe: a reader runs its commands
# against a real editor. That makes every sentence in it a claim about shipped behaviour, and
# the shipped MCP boundary is the legacy stdio handshake. `assay-mcp-server` keeps
# `MODERN_PROTOCOL_VERSION` deprecated and out of negotiation on purpose, and Assay ships no
# remote HTTP transport, no OAuth/OIDC, and no MCP UI.
#
# The recipe nevertheless carried release-candidate wording ("provisional", "will be finalised
# once the spec is final") plus OAuth 2.1 / sandboxed-iframe instructions, which reads as an
# Assay transport capability that exists. This guard keeps the four drifts out.
#
# WHY THIS IS OFFLINE AND UNCONDITIONAL
#
# The acceptance row asks that the recipe cannot claim the modern revision "until the full
# implementation issue is closed with driven evidence". Keying that off live issue state would
# put a network call on a required gate and make the check fail for reasons unrelated to the
# tree. So the forbidden-claim rules below are unconditional and hermetic. Lifting them is a
# deliberate edit to this file, and the only thing that earns it is #2358 closed with driven
# evidence — not a version bump, and not this guard going quiet.
#
# WHAT THIS DELIBERATELY DOES NOT TOUCH
#
# Historical records (CHANGELOG entries, ADR-036, dated measurement baselines) are supposed to
# keep saying what was true when written; this guard reads one file. The local `assay mcp wrap`
# and `proxy-enforce` claims are shipped behaviour and are asserted PRESENT below, so a future
# edit cannot satisfy this guard by deleting them.
#
# Usage: scripts/ci/check-editor-mcp-recipe-truth.sh
#        ASSAY_EDITOR_RECIPE=path/to/recipe.md scripts/ci/check-editor-mcp-recipe-truth.sh  (tests only)

set -euo pipefail

cd "$(dirname "$0")/../.."

# shellcheck source=scripts/ci/lib/editor-plugin-install-commands.sh
source scripts/ci/lib/editor-plugin-install-commands.sh

RECIPE="${ASSAY_EDITOR_RECIPE:-docs/guides/editor-mcp-recipe.md}"
MAX_RECIPE_BYTES=65536
MODERN_REVISION='2026-07-28'
IMPL_ISSUE='2358'

failures=0
fail() {
  failures=$((failures + 1))
  printf 'FAIL: %s\n' "$*"
}
ok() { printf 'ok   %s\n' "$*"; }

if [ ! -f "$RECIPE" ]; then
  printf 'FAIL: executable editor recipe missing: %s\n' "$RECIPE"
  exit 1
fi

# Bound all subsequent captures, including grep diagnostics and extracted commands.
if [ "$(wc -c < "$RECIPE")" -gt "$MAX_RECIPE_BYTES" ]; then
  printf 'FAIL: recipe exceeds %s-byte limit: %s\n' "$MAX_RECIPE_BYTES" "$RECIPE"
  exit 1
fi

# A forbidden pattern is reported with the offending line so the failure names the sentence,
# not just the rule.
forbid() {
  local label="$1" pattern="$2"
  local hits
  hits="$(grep -nEi -- "$pattern" "$RECIPE" || true)"
  if [ -n "$hits" ]; then
    fail "$RECIPE: $label"
    printf '%s\n' "$hits" | sed 's/^/      /'
  else
    ok "$label: absent"
  fi
}

require() {
  local label="$1" pattern="$2"
  if grep -qEi -- "$pattern" "$RECIPE"; then
    ok "$label: present"
  else
    fail "$RECIPE: $label"
  fi
}

# --- Drift 1: stale release-candidate copy -----------------------------------------------
forbid 'stale release-candidate wording (provisional / release candidate)' \
  '(provisional|release[ -]candidate)'

# --- Drift 2: future-tense finalisation --------------------------------------------------
# "will be finalised", "finalising on", "once the spec is final" — a recipe describes what a
# reader can run now, so a promise about a future specification does not belong in it.
forbid 'future-tense specification promise' \
  '(will be finali[sz]ed|finali[sz]ing on|once the spec(ification)? is final|is not yet final)'

# --- Drift 3: remote OAuth / OIDC / UI recipe instructions -------------------------------
# Assay ships none of these. Naming them as something to align a wrapped server to turns an
# unshipped design into an instruction.
forbid 'remote OAuth/OIDC instruction in the executable path' \
  '(OAuth|OIDC|OpenID|PKCE)'
forbid 'MCP UI / sandboxed-iframe instruction in the executable path' \
  '(sandboxed[ -]iframe|server UIs?|MCP UIs?)'

# --- Drift 4: the modern revision must not read as delivered -----------------------------
# The revision may be named, but only as work that is tracked elsewhere and not shipped. If the
# recipe mentions it at all, it must also point at the implementation issue, so a reader cannot
# take the mention for a capability.
if grep -qF -- "$MODERN_REVISION" "$RECIPE"; then
  if grep -qE -- "#${IMPL_ISSUE}|issues/${IMPL_ISSUE}" "$RECIPE"; then
    ok "modern revision $MODERN_REVISION mentioned with #${IMPL_ISSUE} tracked"
  else
    fail "$RECIPE: names MCP $MODERN_REVISION without linking the implementation issue #${IMPL_ISSUE}"
  fi
  forbid "MCP $MODERN_REVISION described as shipped or supported" \
    "(supports?|implements?|ships?|available|enabled)[^.]{0,40}${MODERN_REVISION}|${MODERN_REVISION}[^.]{0,40}(is (now )?(supported|implemented|available|shipped)|support)"
else
  ok "modern revision $MODERN_REVISION not named"
fi

# --- Shipped claims that must survive any future edit of this file -----------------------
# Asserted PRESENT so the forbidden-pattern rules above cannot be satisfied by gutting the
# recipe's real content.
require 'local stdio wrap claim retained' '(assay mcp wrap|`assay mcp wrap`)'
require 'proxy-enforce claim retained' 'proxy-enforce'

# Keep these as standalone commands in this recipe, not a general shell/Markdown grammar.
# Comments, prose, and commands in a different H2 section cannot satisfy plugin prerequisites.
# Version pinning of cargo install is owned by check-release-surface.sh, not this guard.
plugin_commands="$(editor_plugin_install_commands "$RECIPE")"
for command in 'assay version' 'assay-mcp-server --version'; do
  if printf '%s\n' "$plugin_commands" | grep -Fxq -- "$command"; then
    ok "plugin prerequisite command present: $command"
  else
    fail "$RECIPE: plugin prerequisite command missing: $command"
  fi
done

printf '\n'
if [ "$failures" -gt 0 ]; then
  printf 'editor MCP recipe truth: %s prerequisite or shipped-transport drift(s)\n' "$failures"
  printf '\n'
  printf 'The executable recipe describes what a reader can run against a real editor today.\n'
  printf 'Assay ships the legacy stdio handshake; it does not ship remote HTTP, OAuth/OIDC or\n'
  printf 'MCP UI. Move unshipped design material out of the executable path and link #%s\n' "$IMPL_ISSUE"
  printf 'rather than implying delivery. Do not rewrite historical records to satisfy this guard.\n'
  exit 1
fi

printf 'editor MCP recipe truth: %s describes the shipped stdio boundary\n' "$RECIPE"
