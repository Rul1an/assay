#!/usr/bin/env python3
"""Minimal reviewer smoke path for the paused approval pattern."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from patterns.paused_approval import (
    capture_paused_approval,
    derive_resume_state_ref,
    emit_pause_artifact,
)


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Emit one paused approval artifact from raw fixture inputs.")
    parser.add_argument("--paused-result", type=Path, required=True, help="Raw paused result JSON input.")
    parser.add_argument("--serialized-state", type=Path, required=True, help="Serialized paused state input.")
    parser.add_argument("--framework", default="openai_agents_js", help="Originating runtime family token.")
    parser.add_argument(
        "--schema",
        default="openai-agents-js.tool-approval-interruption.export.v1",
        help="Artifact schema identifier to emit.",
    )
    parser.add_argument(
        "--surface",
        default="tool_approval_interruption_resumable_state",
        help="Bounded runtime seam identifier to emit.",
    )
    parser.add_argument("--output", type=Path, default=None, help="Optional output path. Defaults to stdout.")
    return parser.parse_args()


def main() -> int:
    args = _parse_args()
    raw = json.loads(args.paused_result.read_text(encoding="utf-8"))
    serialized_state = args.serialized_state.read_text(encoding="utf-8")

    captured = capture_paused_approval(raw)
    artifact = emit_pause_artifact(
        captured,
        framework=args.framework,
        schema=args.schema,
        surface=args.surface,
        resume_state_ref=derive_resume_state_ref(serialized_state),
    )

    rendered = json.dumps(artifact, indent=2, sort_keys=True)
    if args.output is None:
        print(rendered)
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(f"{rendered}\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
