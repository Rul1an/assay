"""Run one positive and one negative AutoEvals ExactMatch capture for P27 discovery."""

from __future__ import annotations

import argparse
import importlib.metadata
import json
from pathlib import Path
from typing import Any

from autoevals import ExactMatch


DEFAULT_OUT_DIR = Path(__file__).resolve().parent / "discovery"


CASES = {
    "valid": {
        "output": "Amsterdam",
        "expected": "Amsterdam",
    },
    "failure": {
        "output": "Amsterdam",
        "expected": "Rotterdam",
    },
}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a small AutoEvals ExactMatch probe and save raw discovery artifacts."
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


def _score_to_dict(score: Any) -> dict[str, Any]:
    if not hasattr(score, "__dict__"):
        raise TypeError(f"unexpected AutoEvals score object: {type(score).__name__}")
    return dict(score.__dict__)


def _build_inputs(case: dict[str, str]) -> dict[str, Any]:
    return {
        "sdk_language": "python",
        "package": "autoevals",
        "package_version": importlib.metadata.version("autoevals"),
        "scorer": "ExactMatch",
        "output": case["output"],
        "expected": case["expected"],
    }


def main() -> int:
    args = _parse_args()
    scorer = ExactMatch()
    package_version = importlib.metadata.version("autoevals")
    summary: dict[str, Any] = {"package_version": package_version}

    for case_name, case in CASES.items():
        inputs = _build_inputs(case)
        score = scorer.eval(output=case["output"], expected=case["expected"])
        returned = _score_to_dict(score)
        _write_json(args.out_dir / f"{case_name}.scorer.inputs.json", inputs)
        _write_json(args.out_dir / f"{case_name}.returned.score.json", returned)
        summary[f"{case_name}_score"] = returned.get("score")

    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
