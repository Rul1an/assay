"""Generate a tiny serialized artifact derived from a local pydantic_evals EvaluationReport."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from datetime import datetime, timezone
import json
from pathlib import Path

from pydantic_evals import Case, Dataset
from pydantic_evals.evaluators import EqualsExpected, Evaluator, EvaluatorContext
from pydantic_evals.reporting import EvaluationReportAdapter


EXTERNAL_SCHEMA = "pydantic-evals.evaluation-report.export.v1"


@dataclass
class ExactScorePoints(Evaluator):
    """Tiny deterministic evaluator so the frozen sample has explicit scores."""

    def evaluate(self, ctx: EvaluatorContext[str, str]) -> int:
        return 100 if ctx.output == ctx.expected_output else 25


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate a tiny serialized artifact derived from a pydantic_evals EvaluationReport."
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
        help="Where to write the exported report artifact.",
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


def _build_dataset(scenario: str) -> tuple[Dataset[str, str], str]:
    if scenario == "valid":
        cases = [
            Case(name="case-hello", inputs="hello", expected_output="HELLO"),
            Case(name="case-quiet", inputs="quiet", expected_output="QUIET"),
        ]
        task = uppercase_eval_valid
    else:
        cases = [
            Case(name="case-hello", inputs="hello", expected_output="HELLO"),
            Case(name="case-bye", inputs="bye", expected_output="GOODBYE"),
        ]
        task = uppercase_eval_failure

    dataset = Dataset(
        name="uppercase-demo-dataset",
        cases=cases,
        evaluators=[EqualsExpected(), ExactScorePoints()],
    )
    return dataset, task.__name__


def _status_for_case(case_dump: dict) -> str:
    assertions = case_dump.get("assertions", {})
    evaluator_failures = case_dump.get("evaluator_failures", [])
    if evaluator_failures:
        return "failed"
    if assertions and all(bool(assertion["value"]) for assertion in assertions.values()):
        return "passed"
    return "failed"


def _build_exported_artifact(scenario: str, timestamp: str) -> dict:
    dataset, experiment_name = _build_dataset(scenario)
    if scenario == "valid":
        report = dataset.evaluate_sync(uppercase_eval_valid)
        report_id = "pydantic-ai-eval-valid"
    else:
        report = dataset.evaluate_sync(uppercase_eval_failure)
        report_id = "pydantic-ai-eval-failure"

    report_dump = EvaluationReportAdapter.dump_python(report)
    case_results = []
    pass_count = 0
    fail_count = 0

    for case_dump in report_dump["cases"]:
        status = _status_for_case(case_dump)
        if status == "passed":
            pass_count += 1
        else:
            fail_count += 1

        scores = {
            name: int(score["value"])
            for name, score in case_dump.get("scores", {}).items()
        }
        case_result = {
            "case_id": case_dump["name"],
            "status": status,
            "scores": scores,
        }
        assertions = {
            name: bool(assertion["value"])
            for name, assertion in case_dump.get("assertions", {}).items()
        }
        if assertions:
            case_result["assertions"] = assertions
        case_results.append(case_result)

    outcome = "passed" if fail_count == 0 else "failed"
    return {
        "schema": EXTERNAL_SCHEMA,
        "framework": "pydantic-ai",
        "surface": "evaluation_report",
        "dataset_name": dataset.name,
        "experiment_name": experiment_name,
        "report_id": report_id,
        "timestamp": timestamp,
        "outcome": outcome,
        "summary": {
            "case_count": len(case_results),
            "pass_count": pass_count,
            "fail_count": fail_count,
        },
        "case_results": case_results,
    }


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
