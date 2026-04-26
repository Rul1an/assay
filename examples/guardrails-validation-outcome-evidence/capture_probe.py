"""Run one passing and one failing Guardrails ValidationResult capture."""

from __future__ import annotations

import argparse
import importlib.metadata
import json
from pathlib import Path
from typing import Any

from guardrails.validators import FailResult, PassResult, Validator, register_validator


DEFAULT_OUT_DIR = Path(__file__).resolve().parent / "discovery"
VALIDATOR_NAME = "contains-approved-term"
REQUIRED_TERM = "approved"

CASES = {
    "valid": {
        "value": "approved response",
        "metadata": {"required_term": REQUIRED_TERM},
    },
    "failure": {
        "value": "needs review",
        "metadata": {"required_term": REQUIRED_TERM},
    },
}


@register_validator(name=VALIDATOR_NAME, data_type="string")
class ContainsApprovedTerm(Validator):
    """Small local validator for public ValidationResult shape discovery."""

    def _validate(self, value: Any, metadata: dict[str, Any]) -> PassResult | FailResult:
        required_term = metadata.get("required_term", REQUIRED_TERM)
        if isinstance(value, str) and required_term in value.lower():
            return PassResult()
        return FailResult(
            error_message=f"Value must include required term: {required_term}",
            fix_value=f"{value} {required_term}",
        )


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a small Guardrails ValidationResult probe and save raw discovery artifacts."
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


def _result_to_dict(result: PassResult | FailResult) -> dict[str, Any]:
    return result.model_dump(mode="json", exclude_none=False)


def _build_inputs(case: dict[str, Any], package_version: str) -> dict[str, Any]:
    return {
        "sdk_language": "python",
        "package": "guardrails-ai",
        "package_version": package_version,
        "public_path": "Validator.validate",
        "validator_name": VALIDATOR_NAME,
        "value": case["value"],
        "metadata": case["metadata"],
    }


def main() -> int:
    args = _parse_args()
    package_version = importlib.metadata.version("guardrails-ai")
    validator = ContainsApprovedTerm()

    summary: dict[str, Any] = {
        "package_version": package_version,
        "public_path": "Validator.validate",
    }
    for case_name, case in CASES.items():
        result = validator.validate(case["value"], case["metadata"])
        returned = _result_to_dict(result)
        inputs = _build_inputs(case, package_version)

        _write_json(args.out_dir / f"{case_name}.validation.inputs.json", inputs)
        _write_json(args.out_dir / f"{case_name}.returned.result.json", returned)
        summary[f"{case_name}_outcome"] = returned.get("outcome")

    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
