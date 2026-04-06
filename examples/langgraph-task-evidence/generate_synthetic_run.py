"""Generate a tiny LangGraph tasks-v2 export artifact for Assay interop samples."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from typing import Any, TypedDict

from langgraph.checkpoint.memory import InMemorySaver
from langgraph.graph import END, START, StateGraph


SCHEMA = "langgraph.stream.tasks.export.v1"
LANGGRAPH_VERSION = "1.1.6"
SCENARIOS = ("success", "failure")
SCENARIO_THREAD_IDS = {
    "success": "thread_langgraph_success",
    "failure": "thread_langgraph_failure",
}
SCENARIO_TIMESTAMPS = {
    "success": (
        "2026-04-07T10:14:23Z",
        "2026-04-07T10:14:24Z",
    ),
    "failure": (
        "2026-04-07T10:19:23Z",
        "2026-04-07T10:19:24Z",
    ),
}


class GraphState(TypedDict):
    message: str


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate a tiny LangGraph tasks-v2 export artifact."
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


def _normalize_for_hash(value: Any) -> Any:
    if value is None or isinstance(value, (str, int, bool)):
        return value
    if isinstance(value, float):
        if not math.isfinite(value):
            raise ValueError("non-finite floats are not valid in canonical JSON")
        if value.is_integer():
            return int(value)
        return value
    if isinstance(value, dict):
        return {str(key): _normalize_for_hash(nested) for key, nested in value.items()}
    if isinstance(value, list):
        return [_normalize_for_hash(item) for item in value]
    if isinstance(value, tuple):
        return [_normalize_for_hash(item) for item in value]
    raise TypeError(f"unsupported canonical JSON value: {type(value).__name__}")


def _canonical_json(value: Any) -> str:
    normalized = _normalize_for_hash(value)
    return json.dumps(
        normalized,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
        allow_nan=False,
    )


def _sha256(value: Any) -> str:
    return f"sha256:{hashlib.sha256(_canonical_json(value).encode('utf-8')).hexdigest()}"


def _parse_rfc3339_utc(value: str) -> str:
    normalized = value.replace("Z", "+00:00")
    parsed = datetime.fromisoformat(normalized)
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def _task_ref(task_id: str, refs: dict[str, str]) -> str:
    if task_id not in refs:
        refs[task_id] = f"task_{len(refs) + 1}"
    return refs[task_id]


def _success_step(state: GraphState) -> dict[str, str]:
    return {"message": state["message"] + " world"}


def _failure_step(state: GraphState) -> dict[str, str]:
    raise RuntimeError("forced_failure")


def _compile_graph(scenario: str):
    builder = StateGraph(GraphState)
    if scenario == "success":
        builder.add_node("step1", _success_step)
        builder.add_edge(START, "step1")
        builder.add_edge("step1", END)
    else:
        builder.add_node("fail_step", _failure_step)
        builder.add_edge(START, "fail_step")
        builder.add_edge("fail_step", END)
    return builder.compile(checkpointer=InMemorySaver())


def _build_task_start_record(
    part: dict[str, Any],
    scenario: str,
    task_ref: str,
    timestamp: str,
    thread_id: str,
) -> dict[str, Any]:
    payload = part["data"]
    return {
        "schema": SCHEMA,
        "framework": "langgraph",
        "record_type": "task_start",
        "event_phase": "start",
        "stream_mode": "tasks",
        "stream_version": "v2",
        "langgraph_version": LANGGRAPH_VERSION,
        "seam_status": "first-hypothesis",
        "scenario": scenario,
        "thread_id": thread_id,
        "ns": list(part["ns"]),
        "task_ref": task_ref,
        "task_name": payload["name"],
        "task_input_hash": _sha256(payload["input"]),
        "triggers": list(payload.get("triggers", ())),
        "timestamp": timestamp,
    }


def _build_task_result_record(
    part: dict[str, Any],
    scenario: str,
    task_ref: str,
    timestamp: str,
    thread_id: str,
) -> dict[str, Any]:
    payload = part["data"]
    return {
        "schema": SCHEMA,
        "framework": "langgraph",
        "record_type": "task_result",
        "event_phase": "finish",
        "stream_mode": "tasks",
        "stream_version": "v2",
        "langgraph_version": LANGGRAPH_VERSION,
        "seam_status": "first-hypothesis",
        "scenario": scenario,
        "thread_id": thread_id,
        "ns": list(part["ns"]),
        "task_ref": task_ref,
        "task_name": payload["name"],
        "task_result_hash": _sha256(payload["result"]),
        "error": payload["error"],
        "interrupt_count": len(payload.get("interrupts", [])),
        "timestamp": timestamp,
    }


def _build_stream_error_record(
    scenario: str,
    state_snapshot: Any,
    task_ref: str,
    task_name: str,
    error: str,
    timestamp: str,
    thread_id: str,
) -> dict[str, Any]:
    configurable = state_snapshot.config.get("configurable", {})
    checkpoint_ns = configurable.get("checkpoint_ns", "")
    return {
        "schema": SCHEMA,
        "framework": "langgraph",
        "record_type": "stream_error",
        "event_phase": "error",
        "stream_mode": "tasks",
        "stream_version": "v2",
        "langgraph_version": LANGGRAPH_VERSION,
        "seam_status": "first-hypothesis",
        "scenario": scenario,
        "thread_id": thread_id,
        "ns": [checkpoint_ns] if checkpoint_ns else [],
        "task_ref": task_ref,
        "task_name": task_name,
        "error": error,
        "timestamp": timestamp,
    }


def _generate_records(scenario: str) -> list[dict[str, Any]]:
    graph = _compile_graph(scenario)
    thread_id = SCENARIO_THREAD_IDS[scenario]
    config = {"configurable": {"thread_id": thread_id}}
    task_refs: dict[str, str] = {}
    records: list[dict[str, Any]] = []
    timestamps = tuple(_parse_rfc3339_utc(value) for value in SCENARIO_TIMESTAMPS[scenario])
    timestamp_index = 0

    try:
        for part in graph.stream(
            {"message": "hello"},
            config=config,
            stream_mode="tasks",
            version="v2",
        ):
            if part.get("type") != "tasks":
                continue
            payload = part["data"]
            task_ref = _task_ref(payload["id"], task_refs)
            if "input" in payload:
                records.append(
                    _build_task_start_record(
                        part,
                        scenario,
                        task_ref,
                        timestamps[timestamp_index],
                        thread_id,
                    )
                )
            else:
                records.append(
                    _build_task_result_record(
                        part,
                        scenario,
                        task_ref,
                        timestamps[timestamp_index],
                        thread_id,
                    )
                )
            timestamp_index += 1
    except Exception:
        if scenario != "failure":
            raise
        state_snapshot = graph.get_state(config)
        if not state_snapshot.tasks:
            raise SystemExit("failure scenario did not expose a failed task in state snapshot")
        failed_task = state_snapshot.tasks[0]
        task_ref = _task_ref(failed_task.id, task_refs)
        records.append(
            _build_stream_error_record(
                scenario,
                state_snapshot,
                task_ref,
                failed_task.name,
                failed_task.error or "unknown_error",
                timestamps[timestamp_index],
                thread_id,
            )
        )

    return records


def main() -> int:
    args = _parse_args()
    if args.output.exists() and not args.overwrite:
        raise SystemExit(f"{args.output} already exists; pass --overwrite to replace it")

    records = _generate_records(args.scenario)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", encoding="utf-8") as handle:
        for record in records:
            handle.write(_canonical_json(record))
            handle.write("\n")

    print(f"Wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
