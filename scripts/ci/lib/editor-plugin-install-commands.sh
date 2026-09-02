#!/usr/bin/env bash
# Extract standalone command lines from the Claude Code plugin install section.
#
# One parser, sourced by the editor-transport guard and the release-surface guard.
# This file does not judge cargo vs claude, quotes, or continuations. Cardinality
# of matching H2s, ```bash fences, and the explicit end marker in that section is
# this parser's job: invalid structure fails closed and emits nothing. Semantic
# reject stays with consumers.

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
          if (section && bash_fence) {
            bash_fence_close_count++
            if (marker_before_close) marker_close_count++
          }
          fence = 0
          bash_fence = 0
        } else if (section && bash_fence) {
          nested_fence_before_close = 1
          marker_before_close = 0
        }
      } else {
        fence = 1
        fence_mark = mark
        fence_width = width
        bash_fence = (fence_line == "```bash")
        if (section && bash_fence) {
          bash_fence_count++
          marker_before_close = 0
        }
      }
      next
    }
    !fence && /^## / {
      section = ($0 == "## Install the Claude Code plugin")
      if (section) h2_count++
      next
    }
    section && fence && bash_fence {
      marker_before_close = 0
      if ($0 == "# assay-editor-plugin-install-commands:end") {
        marker_count++
        marker_before_close = 1
        next
      }
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
      if (bash_fence_close_count != 1) {
        print "plugin bash fence is not closed at its own boundary" > "/dev/stderr"
        exit 1
      }
      if (nested_fence_before_close) {
        print "plugin bash fence is not closed at its own boundary" > "/dev/stderr"
        exit 1
      }
      if (marker_count != 1) {
        print "expected exactly one plugin bash fence end marker" > "/dev/stderr"
        exit 1
      }
      if (bash_fence_close_count == 1 && !nested_fence_before_close && marker_close_count != 1) {
        print "plugin bash fence end marker must immediately precede its close" > "/dev/stderr"
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
