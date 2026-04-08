"""Generate a tiny OpenAI Agents trace export artifact for Assay interop samples."""

from __future__ import annotations

import argparse
import asyncio
import copy
import json
from pathlib import Path
from typing import Any

from agents import Agent, RunConfig, Runner, function_tool, set_trace_processors
from agents.items import ModelResponse
from agents.models.interface import Model
from agents.tracing import TracingProcessor
from agents.usage import Usage
from openai.types.responses import ResponseFunctionToolCall


SCHEMA = "openai.agents.trace.export.v1"
OPENAI_AGENTS_VERSION = "0.13.5"
SCENARIOS = ("success", "failure")
SCENARIO_TRACE_IDS = {
    "success": "trace_openai_agents_success",
    "failure": "trace_openai_agents_failure",
}
SCENARIO_GROUP_IDS = {
    "success": "grp_openai_agents_success",
    "failure": "grp_openai_agents_failure",
}
SCENARIO_WORKFLOW_NAMES = {
    "success": "OpenAI Agents trace sample (success)",
    "failure": "OpenAI Agents trace sample (failure)",
}
SCENARIO_TIMESTAMPS = {
    "success": (
        "2026-04-07T12:14:23Z",
        "2026-04-07T12:14:24Z",
        "2026-04-07T12:14:25Z",
        "2026-04-07T12:14:26Z",
        "2026-04-07T12:14:27Z",
        "2026-04-07T12:14:28Z",
    ),
    "failure": (
        "2026-04-07T12:19:23Z",
        "2026-04-07T12:19:24Z",
        "2026-04-07T12:19:25Z",
        "2026-04-07T12:19:26Z",
        "2026-04-07T12:19:27Z",
        "2026-04-07T12:19:28Z",
    ),
}


class RecordingTraceProcessor(TracingProcessor):
    """Collect trace and span callbacks in callback order."""

    def __init__(self) -> None:
        self.events: list[dict[str, Any]] = []

    def _record(self, kind: str, payload: dict[str, Any] | None) -> None:
        if payload is None:
            return
        self.events.append({"kind": kind, "payload": copy.deepcopy(payload)})

    def on_trace_start(self, trace) -> None:
        self._record("trace_start", trace.export())

    def on_trace_end(self, trace) -> None:
        self._record("trace_end", trace.export())

    def on_span_start(self, span) -> None:
        self._record("span_start", span.export())

    def on_span_end(self, span) -> None:
        self._record("span_end", span.export())

    def shutdown(self) -> None:
        return None

    def force_flush(self) -> None:
        return None


class OneToolModel(Model):
    """A minimal local model that always requests one tool call."""

    async def get_response(
        self,
        system_instructions: str | None,
        input: str | list[Any],
        model_settings,
        tools,
        output_schema,
        handoffs,
        tracing,
        *,
        previous_response_id: str | None,
        conversation_id: str | None,
        prompt,
    ) -> ModelResponse:
        return ModelResponse(
            output=[
                ResponseFunctionToolCall(
                    id="fc_lookup_policy",
                    call_id="call_lookup_policy",
                    arguments='{"query":"latest policy"}',
                    name="lookup_policy",
                    type="function_call",
                )
            ],
            usage=Usage(requests=1, input_tokens=1, output_tokens=1, total_tokens=2),
            response_id="resp_lookup_policy",
        )

    def stream_response(self, *args, **kwargs):
        raise NotImplementedError("streaming is not needed for this sample")


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate a tiny OpenAI Agents trace export artifact."
    )
    parser.add_argument(
        "--scenario",
        required=True,
        choices=SCENARIOS,
        help="Which frozen sample scenario to generate.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="Where to write the NDJSON artifact.",
    )
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="Allow overwriting the output file if it already exists.",
    )
    return parser.parse_args()


def _make_lookup_policy_tool(scenario: str):
    if scenario == "success":

        async def lookup_policy(query: str) -> str:
            return f"policy:{query}"

    else:

        async def lookup_policy(query: str) -> str:
            raise RuntimeError("policy backend unavailable")

    return function_tool(
        lookup_policy,
        name_override="lookup_policy",
        description_override="Look up the latest local policy snapshot.",
    )


def _span_ref(raw_span_id: str, refs: dict[str, str]) -> str:
    if raw_span_id not in refs:
        refs[raw_span_id] = f"span_{len(refs) + 1}"
    return refs[raw_span_id]


def _build_trace_record(
    *,
    payload: dict[str, Any],
    scenario: str,
    event_phase: str,
    emission_sequence: int,
    timestamp: str,
) -> dict[str, Any]:
    return {
        "schema": SCHEMA,
        "framework": "openai-agents-python",
        "surface": "trace_processor",
        "sdk_version": OPENAI_AGENTS_VERSION,
        "seam_status": "first-hypothesis",
        "scenario": scenario,
        "record_type": "trace_event",
        "event_phase": event_phase,
        "object_type": payload["object"],
        "workflow_name": payload["workflow_name"],
        "trace_id": payload["id"],
        "group_id": payload["group_id"],
        "trace_metadata": payload.get("metadata"),
        "trace_include_sensitive_data": False,
        "emission_sequence": emission_sequence,
        "timestamp": timestamp,
    }


def _build_span_record(
    *,
    payload: dict[str, Any],
    scenario: str,
    event_phase: str,
    emission_sequence: int,
    timestamp: str,
    span_refs: dict[str, str],
) -> dict[str, Any]:
    span_data = payload["span_data"]
    record: dict[str, Any] = {
        "schema": SCHEMA,
        "framework": "openai-agents-python",
        "surface": "trace_processor",
        "sdk_version": OPENAI_AGENTS_VERSION,
        "seam_status": "first-hypothesis",
        "scenario": scenario,
        "record_type": "span_event",
        "event_phase": event_phase,
        "object_type": payload["object"],
        "workflow_name": SCENARIO_WORKFLOW_NAMES[scenario],
        "trace_id": payload["trace_id"],
        "group_id": SCENARIO_GROUP_IDS[scenario],
        "trace_include_sensitive_data": False,
        "emission_sequence": emission_sequence,
        "timestamp": timestamp,
        "span_id": _span_ref(payload["id"], span_refs),
        "parent_span_id": (
            _span_ref(payload["parent_id"], span_refs) if payload.get("parent_id") else None
        ),
        "span_type": span_data["type"],
        "span_name": span_data.get("name"),
        "observed_status": (
            "started" if event_phase == "start" else ("failed" if payload.get("error") else "ok")
        ),
    }

    if span_data["type"] == "agent":
        record["agent_name"] = span_data.get("name")
        record["available_tools"] = span_data.get("tools")
        record["available_handoffs"] = span_data.get("handoffs")
        record["output_type"] = span_data.get("output_type")
    elif span_data["type"] == "function":
        record["tool_name"] = span_data.get("name")

    if payload.get("error") is not None:
        record["error"] = payload["error"]

    return record


def _normalize_records(events: list[dict[str, Any]], scenario: str) -> list[dict[str, Any]]:
    timestamps = SCENARIO_TIMESTAMPS[scenario]
    if len(events) != len(timestamps):
        raise ValueError(
            f"expected {len(timestamps)} trace callbacks for {scenario}, got {len(events)}"
        )

    span_refs: dict[str, str] = {}
    records: list[dict[str, Any]] = []
    for index, event in enumerate(events):
        kind = event["kind"]
        payload = event["payload"]
        timestamp = timestamps[index]
        if kind == "trace_start":
            record = _build_trace_record(
                payload=payload,
                scenario=scenario,
                event_phase="start",
                emission_sequence=index + 1,
                timestamp=timestamp,
            )
        elif kind == "trace_end":
            record = _build_trace_record(
                payload=payload,
                scenario=scenario,
                event_phase="finish",
                emission_sequence=index + 1,
                timestamp=timestamp,
            )
        elif kind == "span_start":
            record = _build_span_record(
                payload=payload,
                scenario=scenario,
                event_phase="start",
                emission_sequence=index + 1,
                timestamp=timestamp,
                span_refs=span_refs,
            )
        elif kind == "span_end":
            record = _build_span_record(
                payload=payload,
                scenario=scenario,
                event_phase="finish",
                emission_sequence=index + 1,
                timestamp=timestamp,
                span_refs=span_refs,
            )
        else:
            raise ValueError(f"unsupported event kind: {kind}")
        records.append(record)

    return records


async def _generate_records(scenario: str) -> list[dict[str, Any]]:
    processor = RecordingTraceProcessor()
    set_trace_processors([processor])

    agent = Agent(
        name="audit-agent",
        tools=[_make_lookup_policy_tool(scenario)],
        model=OneToolModel(),
        tool_use_behavior="stop_on_first_tool",
    )
    await Runner.run(
        agent,
        "Check the latest policy before continuing.",
        run_config=RunConfig(
            workflow_name=SCENARIO_WORKFLOW_NAMES[scenario],
            trace_id=SCENARIO_TRACE_IDS[scenario],
            group_id=SCENARIO_GROUP_IDS[scenario],
            trace_metadata={"sample": scenario, "source": "local-trace-processor"},
            trace_include_sensitive_data=False,
        ),
    )
    return _normalize_records(processor.events, scenario)


def _write_ndjson(path: Path, records: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        for record in records:
            handle.write(json.dumps(record, ensure_ascii=False, separators=(",", ":"), sort_keys=True))
            handle.write("\n")


def main() -> int:
    args = _parse_args()
    if args.output.exists() and not args.overwrite:
        raise SystemExit(f"{args.output} already exists; pass --overwrite to replace it")

    records = asyncio.run(_generate_records(args.scenario))
    _write_ndjson(args.output, records)
    print(f"Wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
