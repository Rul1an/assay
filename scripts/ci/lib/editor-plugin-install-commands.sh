#!/usr/bin/env bash
# Extract standalone command lines from the Claude Code plugin install section.
#
# One parser, sourced by the editor-transport guard and the release-surface guard.
# This file does not judge cargo vs claude, quotes, or continuations. Cardinality
# of matching H2s and ```bash fences in that section is this parser's job: not
# exactly one of each fails closed and emits nothing. Semantic reject stays with
# consumers.

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
          if (section && bash_fence) bash_fence_close_count++
          fence = 0
          bash_fence = 0
        } else if (section && bash_fence) {
          nested_fence_before_close = 1
        }
      } else {
        fence = 1
        fence_mark = mark
        fence_width = width
        bash_fence = (fence_line == "```bash")
        if (section && bash_fence) bash_fence_count++
      }
      next
    }
    !fence && /^## / {
      section = ($0 == "## Install the Claude Code plugin")
      if (section) h2_count++
      next
    }
    section && fence && bash_fence {
      sub(/^[ \t]+/, "")
      sub(/[ \t]+$/, "")
      if ($0 != "" && $0 !~ /^#/) {
        if (ncmds) cmds = cmds "\n" $0
        else cmds = $0
        ncmds++
      }
    }
    END {
      if (h2_count != 1) {
        print "expected exactly one plugin install heading" > "/dev/stderr"
        exit 1
      }
      if (bash_fence_count != 1) {
        print "expected exactly one bash fence in the plugin section" > "/dev/stderr"
        exit 1
      }
      if (bash_fence_close_count != 1 || nested_fence_before_close) {
        print "plugin bash fence is not closed at its own boundary" > "/dev/stderr"
        exit 1
      }
      if (ncmds) print cmds
    }
  ' "$recipe"
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  set -euo pipefail
  editor_plugin_install_commands "$@"
fi
