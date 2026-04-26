"""Run one normal and one fallback OpenFeature EvaluationDetails capture."""

from __future__ import annotations

import argparse
from enum import Enum
import importlib.metadata
import json
from pathlib import Path
from typing import Any

from openfeature import api
from openfeature.provider.in_memory_provider import InMemoryFlag, InMemoryProvider


DEFAULT_OUT_DIR = Path(__file__).resolve().parent / "discovery"

FLAGS = {
    "checkout.new_flow": InMemoryFlag("on", {"on": True, "off": False}),
}

CASES = {
    "valid": {
        "flag_key": "checkout.new_flow",
        "default_value": False,
    },
    "fallback": {
        "flag_key": "checkout.missing",
        "default_value": False,
    },
}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a small OpenFeature EvaluationDetails probe and save raw discovery artifacts."
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=DEFAULT_OUT_DIR,
        help="Directory to write raw discovery artifacts into.",
    )
    return parser.parse_args()


def _jsonable(value: Any) -> Any:
    if isinstance(value, Enum):
        return value.value
    if isinstance(value, dict):
        return {str(key): _jsonable(nested) for key, nested in value.items()}
    if isinstance(value, list):
        return [_jsonable(item) for item in value]
    if isinstance(value, tuple):
        return [_jsonable(item) for item in value]
    return value


def _write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(_jsonable(payload), indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def _details_to_dict(details: Any) -> dict[str, Any]:
    if not hasattr(details, "__dict__"):
        raise TypeError(f"unexpected OpenFeature details object: {type(details).__name__}")
    return dict(details.__dict__)


def _build_inputs(case: dict[str, Any], package_version: str) -> dict[str, Any]:
    return {
        "sdk_language": "python",
        "package": "openfeature-sdk",
        "package_version": package_version,
        "provider": "InMemoryProvider",
        "method": "get_boolean_details",
        "flag_key": case["flag_key"],
        "default_value": case["default_value"],
        "defined_flags": {
            "checkout.new_flow": {
                "default_variant": "on",
                "variants": {"on": True, "off": False},
            }
        },
    }


def main() -> int:
    args = _parse_args()
    package_version = importlib.metadata.version("openfeature-sdk")

    api.set_provider(InMemoryProvider(FLAGS))
    client = api.get_client()

    summary: dict[str, Any] = {"package_version": package_version}
    for case_name, case in CASES.items():
        details = client.get_boolean_details(case["flag_key"], case["default_value"])
        returned = _details_to_dict(details)
        inputs = _build_inputs(case, package_version)

        _write_json(args.out_dir / f"{case_name}.evaluation.inputs.json", inputs)
        _write_json(args.out_dir / f"{case_name}.returned.details.json", returned)
        summary[f"{case_name}_reason"] = _jsonable(returned.get("reason"))
        summary[f"{case_name}_value"] = returned.get("value")

    print(json.dumps(_jsonable(summary), indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
