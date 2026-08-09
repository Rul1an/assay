#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
ASSAY_CLAUDE_WORKFLOW_SCRIPT="$SCRIPT_DIR/$(basename -- "${BASH_SOURCE[0]}")"
export ASSAY_CLAUDE_WORKFLOW_SCRIPT

exec python3 "$SCRIPT_DIR/claude_plugin_install_workflow.py" "$@"
