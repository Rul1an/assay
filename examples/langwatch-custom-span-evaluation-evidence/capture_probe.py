"""Emit and retrieve two LangWatch custom span evaluations for P25 discovery."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import time
from typing import Any

import httpx
import langwatch


DEFAULT_ENDPOINT = "http://127.0.0.1:5560"
DEFAULT_OUT_DIR = Path(__file__).resolve().parent / "discovery"
DEFAULT_VALID_SCORE = 0.92
DEFAULT_FAILURE_SCORE = 0.08


@dataclass(frozen=True)
class CaseConfig:
    case_name: str
    evaluation_name: str
    passed: bool
    score: float
    label: str
    details: str | None


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Emit one positive and one negative LangWatch custom span evaluation and save raw discovery payloads."
    )
    parser.add_argument(
        "--endpoint",
        default=os.environ.get("LANGWATCH_ENDPOINT", DEFAULT_ENDPOINT),
        help="LangWatch base URL. Defaults to LANGWATCH_ENDPOINT or http://127.0.0.1:5560.",
    )
    parser.add_argument(
        "--api-key",
        default=os.environ.get("LANGWATCH_API_KEY"),
        help="LangWatch API key. Defaults to LANGWATCH_API_KEY.",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=DEFAULT_OUT_DIR,
        help="Directory to write raw discovery artifacts into.",
    )
    return parser.parse_args()


def _require_api_key(api_key: str | None) -> str:
    if api_key and api_key.strip():
        return api_key.strip()
    raise SystemExit("LANGWATCH_API_KEY is required for live P25 capture")


def _write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def _validate_auth(endpoint: str, api_key: str) -> None:
    response = httpx.post(
        f"{endpoint.rstrip('/')}/api/auth/validate",
        headers={"X-Auth-Token": api_key},
        timeout=15.0,
    )
    response.raise_for_status()


def _format_span_id(span: Any) -> str:
    span_context = span.get_span_context()
    if not span_context or not span_context.is_valid:
        raise RuntimeError("current LangWatch span did not expose a valid span context")
    return format(span_context.span_id, "x")


def _extract_matching_evaluation_span(
    trace_response: dict[str, Any],
    *,
    target_span_id: str,
    evaluation_name: str,
) -> dict[str, Any] | None:
    spans = trace_response.get("spans")
    if not isinstance(spans, list):
        return None

    for span in spans:
        if not isinstance(span, dict):
            continue
        if span.get("type") != "evaluation":
            continue
        if span.get("name") != evaluation_name:
            continue
        if span.get("parent_id") != target_span_id:
            continue
        output = span.get("output")
        if not isinstance(output, dict):
            continue
        if output.get("type") != "evaluation_result":
            continue
        return span
    return None


def _wait_for_trace_surface(
    *,
    trace_id: str,
    target_span_id: str,
    evaluation_name: str,
    deadline_seconds: float = 45.0,
) -> tuple[dict[str, Any], dict[str, Any]]:
    deadline = time.monotonic() + deadline_seconds
    last_error: str | None = None

    while time.monotonic() < deadline:
        try:
            trace_response = langwatch.traces.get(trace_id)
        except Exception as exc:  # noqa: BLE001 - discovery note wants the raw fetch to keep retrying
            last_error = str(exc)
            time.sleep(1.0)
            continue

        evaluation_span = _extract_matching_evaluation_span(
            trace_response,
            target_span_id=target_span_id,
            evaluation_name=evaluation_name,
        )
        if evaluation_span:
            return trace_response, evaluation_span
        time.sleep(1.0)

    suffix = f" Last error: {last_error}" if last_error else ""
    raise RuntimeError(
        "timed out waiting for LangWatch trace details to surface the evaluation span."
        + suffix
    )


def _capture_case(config: CaseConfig) -> dict[str, Any]:
    emitted_input = {
        "sdk_surface": "langwatch.get_current_span().add_evaluation",
        "name": config.evaluation_name,
        "passed": config.passed,
        "score": config.score,
        "label": config.label,
    }
    if config.details is not None:
        emitted_input["details"] = config.details

    with langwatch.trace(name=f"p25.langwatch.{config.case_name}") as trace:
        trace.update(
            metadata={
                "labels": [
                    "assay",
                    "p25",
                    "langwatch",
                    "custom_span_evaluation",
                    config.case_name,
                ]
            }
        )
        with langwatch.span(name=f"p25.target.{config.case_name}", type="tool") as target_span:
            target_span_id = _format_span_id(target_span)
            langwatch.get_current_span().add_evaluation(
                name=config.evaluation_name,
                passed=config.passed,
                score=config.score,
                label=config.label,
                details=config.details,
            )

        trace_id = trace.trace_id

    if not trace_id:
        raise RuntimeError("LangWatch trace did not expose a trace_id after capture")

    surfaced_trace_response, surfaced_evaluation_span = _wait_for_trace_surface(
        trace_id=trace_id,
        target_span_id=target_span_id,
        evaluation_name=config.evaluation_name,
    )

    return {
        "trace_id": trace_id,
        "target_span_id": target_span_id,
        "emitted_input": emitted_input,
        "surfaced_trace_response": surfaced_trace_response,
        "surfaced_evaluation_span": surfaced_evaluation_span,
    }


def main() -> int:
    args = _parse_args()
    api_key = _require_api_key(args.api_key)
    endpoint = args.endpoint.rstrip("/")

    _validate_auth(endpoint, api_key)
    langwatch.setup(api_key=api_key, endpoint_url=endpoint, debug=False)

    valid = _capture_case(
        CaseConfig(
            case_name="valid",
            evaluation_name="correctness",
            passed=True,
            score=DEFAULT_VALID_SCORE,
            label="correct",
            details="Short bounded explanation from the LangWatch SDK probe.",
        )
    )
    failure = _capture_case(
        CaseConfig(
            case_name="failure",
            evaluation_name="correctness",
            passed=False,
            score=DEFAULT_FAILURE_SCORE,
            label="incorrect",
            details=None,
        )
    )

    _write_json(args.out_dir / "valid.emitted.input.json", valid["emitted_input"])
    _write_json(
        args.out_dir / "valid.surfaced.trace.response.json",
        valid["surfaced_trace_response"],
    )
    _write_json(
        args.out_dir / "valid.surfaced.evaluation.span.json",
        valid["surfaced_evaluation_span"],
    )
    _write_json(args.out_dir / "failure.emitted.input.json", failure["emitted_input"])
    _write_json(
        args.out_dir / "failure.surfaced.trace.response.json",
        failure["surfaced_trace_response"],
    )
    _write_json(
        args.out_dir / "failure.surfaced.evaluation.span.json",
        failure["surfaced_evaluation_span"],
    )

    print(
        json.dumps(
            {
                "endpoint": endpoint,
                "valid": {
                    "trace_id": valid["trace_id"],
                    "target_span_id": valid["target_span_id"],
                },
                "failure": {
                    "trace_id": failure["trace_id"],
                    "target_span_id": failure["target_span_id"],
                },
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
