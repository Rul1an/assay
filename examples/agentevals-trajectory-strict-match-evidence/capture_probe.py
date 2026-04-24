"""Run one positive and one negative AgentEvals strict-match capture for P26 discovery."""

from __future__ import annotations

import argparse
import importlib.metadata
import json
from pathlib import Path
from typing import Any

from agentevals.trajectory.match import create_trajectory_match_evaluator


DEFAULT_OUT_DIR = Path(__file__).resolve().parent / "discovery"


VALID_OUTPUTS = [
    {
        "role": "assistant",
        "content": "I'll check the weather in Amsterdam.",
        "tool_calls": [
            {
                "id": "call_weather",
                "name": "get_weather",
                "args": {"city": "Amsterdam"},
            }
        ],
    },
    {
        "role": "tool",
        "tool_call_id": "call_weather",
        "content": "12C and clear",
    },
    {
        "role": "assistant",
        "content": "It is 12C and clear in Amsterdam.",
    },
]

REFERENCE_OUTPUTS = [
    {
        "role": "assistant",
        "content": "I'll check the weather in Amsterdam.",
        "tool_calls": [
            {
                "id": "call_weather",
                "name": "get_weather",
                "args": {"city": "Amsterdam"},
            }
        ],
    },
    {
        "role": "tool",
        "tool_call_id": "call_weather",
        "content": "12C and clear",
    },
    {
        "role": "assistant",
        "content": "It is 12C and clear in Amsterdam.",
    },
]

FAILURE_OUTPUTS = [
    {
        "role": "assistant",
        "content": "I'll look it up first.",
        "tool_calls": [
            {
                "id": "call_search",
                "name": "search_web",
                "args": {"query": "Amsterdam weather"},
            }
        ],
    },
    {
        "role": "tool",
        "tool_call_id": "call_search",
        "content": "12C and clear",
    },
    {
        "role": "assistant",
        "content": "It is 12C and clear in Amsterdam.",
    },
]


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a small AgentEvals strict-match probe and save raw discovery artifacts."
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=DEFAULT_OUT_DIR,
        help="Directory to write raw discovery artifacts into.",
    )
    return parser.parse_args()


def _write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def _build_inputs(outputs: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "sdk_language": "python",
        "package": "agentevals",
        "package_version": importlib.metadata.version("agentevals"),
        "evaluator_factory": "create_trajectory_match_evaluator",
        "trajectory_match_mode": "strict",
        "outputs": outputs,
        "reference_outputs": REFERENCE_OUTPUTS,
    }


def main() -> int:
    args = _parse_args()
    evaluator = create_trajectory_match_evaluator(trajectory_match_mode="strict")

    valid_inputs = _build_inputs(VALID_OUTPUTS)
    failure_inputs = _build_inputs(FAILURE_OUTPUTS)

    valid_result = evaluator(
        outputs=valid_inputs["outputs"],
        reference_outputs=valid_inputs["reference_outputs"],
    )
    failure_result = evaluator(
        outputs=failure_inputs["outputs"],
        reference_outputs=failure_inputs["reference_outputs"],
    )

    _write_json(args.out_dir / "valid.evaluator.inputs.json", valid_inputs)
    _write_json(args.out_dir / "valid.returned.result.json", valid_result)
    _write_json(args.out_dir / "failure.evaluator.inputs.json", failure_inputs)
    _write_json(args.out_dir / "failure.returned.result.json", failure_result)

    print(
        json.dumps(
            {
                "package_version": importlib.metadata.version("agentevals"),
                "valid_score": valid_result.get("score"),
                "failure_score": failure_result.get("score"),
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
