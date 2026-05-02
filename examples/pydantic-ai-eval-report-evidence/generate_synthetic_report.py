"""Generate one reduced case-result artifact from a local pydantic_evals report."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from datetime import datetime, timezone
import json
from pathlib import Path

from pydantic_evals import Case, Dataset
from pydantic_evals.evaluators import EqualsExpected, Evaluator, EvaluatorContext
from pydantic_evals.reporting import EvaluationReportAdapter


EXTERNAL_SCHEMA = "pydantic-evals.report-case-result.export.v1"


@dataclass
class ExactScorePoints(Evaluator):
    """Tiny deterministic evaluator so the frozen sample has explicit scores."""

    def evaluate(self, ctx: EvaluatorContext[str, str]) -> float:
        return 1.0 if ctx.output == ctx.expected_output else 0.25


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Generate one reduced case-result artifact derived from "
            "pydantic_evals EvaluationReport.cases[]."
        )
    )
    parser.add_argument(
        "--scenario",
        required=True,
        choices=("valid", "failure"),
        help="Which frozen report scenario to generate.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="Where to write the reduced case-result artifact.",
    )
    parser.add_argument(
        "--timestamp",
        default=None,
        help="RFC3339 UTC timestamp to embed in the exported artifact.",
    )
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="Allow overwriting the output file if it already exists.",
    )
    return parser.parse_args()


def _parse_rfc3339_utc(value: str | None) -> str:
    if value is None:
        return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    normalized = value.replace("Z", "+00:00")
    try:
        parsed = datetime.fromisoformat(normalized)
    except ValueError as exc:
        raise ValueError(f"invalid RFC3339 timestamp: {value}") from exc
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def uppercase_eval_valid(inputs: str) -> str:
    return inputs.upper()


def uppercase_eval_failure(inputs: str) -> str:
    return inputs.upper()


def _build_dataset(scenario: str) -> Dataset[str, str]:
    if scenario == "valid":
        cases = [
            Case(name="case-hello", inputs="hello", expected_output="HELLO"),
            Case(name="case-quiet", inputs="quiet", expected_output="QUIET"),
        ]
    else:
        cases = [
            Case(name="case-hello", inputs="hello", expected_output="HELLO"),
            Case(name="case-bye", inputs="bye", expected_output="GOODBYE"),
        ]

    return Dataset(
        name="uppercase-demo-dataset",
        cases=cases,
        evaluators=[EqualsExpected(), ExactScorePoints()],
    )


def _non_empty_string(value: object) -> str | None:
    if isinstance(value, str) and value.strip():
        return value
    return None


def _assertion_passed(name: str, assertion: dict) -> bool:
    value = assertion.get("value")
    if not isinstance(value, bool):
        raise ValueError(f"assertion {name} value must be a boolean")
    return value


def _bounded_result_from_assertion(name: str, assertion: dict) -> dict:
    result = {
        "kind": "assertion",
        "evaluator_name": _non_empty_string(assertion.get("name")) or name,
        "passed": _assertion_passed(name, assertion),
    }
    reason = _non_empty_string(assertion.get("reason"))
    if reason is not None:
        result["reason"] = reason
    return result


def _bounded_result_from_score(name: str, score: dict) -> dict:
    result = {
        "kind": "score",
        "evaluator_name": _non_empty_string(score.get("name")) or name,
        "score": score["value"],
    }
    reason = _non_empty_string(score.get("reason"))
    if reason is not None:
        result["reason"] = reason
    return result


def _select_case_dump(report_dump: dict, scenario: str) -> dict:
    cases = report_dump["cases"]
    if scenario == "valid":
        return cases[0]

    for case_dump in cases:
        assertions = case_dump.get("assertions", {})
        if any(
            not _assertion_passed(name, assertion)
            for name, assertion in assertions.items()
        ):
            return case_dump
    return cases[-1]


def _build_exported_artifact(scenario: str, timestamp: str) -> dict:
    dataset = _build_dataset(scenario)
    if scenario == "valid":
        report = dataset.evaluate_sync(uppercase_eval_valid)
    else:
        report = dataset.evaluate_sync(uppercase_eval_failure)

    report_dump = EvaluationReportAdapter.dump_python(report)
    case_dump = _select_case_dump(report_dump, scenario)
    results = [
        _bounded_result_from_assertion(name, assertion)
        for name, assertion in sorted(case_dump.get("assertions", {}).items())
    ]
    results.extend(
        _bounded_result_from_score(name, score)
        for name, score in sorted(case_dump.get("scores", {}).items())
    )
    if not results:
        raise ValueError("selected case did not expose bounded assertion or score results")

    artifact = {
        "schema": EXTERNAL_SCHEMA,
        "framework": "pydantic_evals",
        "surface": "evaluation_report.cases.case_result",
        "case_name": case_dump["name"],
        "results": results,
        "timestamp": timestamp,
    }
    source_case_name = _non_empty_string(case_dump.get("source_case_name"))
    if source_case_name is not None:
        artifact["source_case_name"] = source_case_name
    return artifact


def main() -> int:
    args = _parse_args()
    if args.output.exists() and not args.overwrite:
        raise SystemExit(f"{args.output} already exists; pass --overwrite to replace it")

    try:
        timestamp = _parse_rfc3339_utc(args.timestamp)
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc

    artifact = _build_exported_artifact(args.scenario, timestamp)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(artifact, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(f"Wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
