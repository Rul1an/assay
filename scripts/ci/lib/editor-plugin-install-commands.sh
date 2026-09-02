#!/usr/bin/env bash
# Extract standalone command lines from the Claude Code plugin install section.
#
# One parser, sourced by the editor-transport guard and the release-surface guard.
# This file does not judge cargo vs claude, quotes, or continuations. Missing or
# wrong H2 / fence emit nothing; consumers own cardinality and semantic reject.

editor_plugin_install_commands() {
  local recipe="${1:-}"
  local max_recipe_bytes=65536

  if [ -z "$recipe" ]; then
    printf 'usage: editor_plugin_install_commands <recipe-path>\n' >&2
    return 2
  fi
  if [ ! -f "$recipe" ]; then
    printf 'recipe not found: %s\n' "$recipe" >&2
    return 1
  fi
  if [ "$(wc -c < "$recipe")" -gt "$max_recipe_bytes" ]; then
    printf 'recipe exceeds %s-byte limit: %s\n' "$max_recipe_bytes" "$recipe" >&2
    return 1
  fi

  awk '
    {
      fence_line = $0
      sub(/^ ? ? ?/, "", fence_line)
    }
    match(fence_line, /^(```+|~~~+)/) {
      width = RLENGTH
      mark = substr(fence_line, 1, 1)
      rest = substr(fence_line, width + 1)
      if (fence) {
        if (mark == fence_mark && width >= fence_width && rest ~ /^[ \t]*$/) {
          fence = 0
          bash_fence = 0
        }
      } else {
        fence = 1
        fence_mark = mark
        fence_width = width
        bash_fence = (fence_line == "```bash")
      }
      next
    }
    !fence && /^## / {
      section = ($0 == "## Install the Claude Code plugin")
      next
    }
    section && fence && bash_fence {
      sub(/^[ \t]+/, "")
      sub(/[ \t]+$/, "")
      if ($0 != "" && $0 !~ /^#/) print
    }
  ' "$recipe"
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  set -euo pipefail
  editor_plugin_install_commands "$@"
fi
