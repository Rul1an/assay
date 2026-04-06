"""Emit a tiny synthetic CrewAI run through the real event bus."""

from __future__ import annotations

import argparse
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

from crewai.events import (
    CrewKickoffCompletedEvent,
    CrewKickoffFailedEvent,
    CrewKickoffStartedEvent,
    MCPToolExecutionCompletedEvent,
    MCPToolExecutionStartedEvent,
    TaskCompletedEvent,
    TaskFailedEvent,
    TaskStartedEvent,
    ToolUsageErrorEvent,
    ToolUsageFinishedEvent,
    ToolUsageStartedEvent,
    crewai_event_bus,
)
from crewai.events.base_events import BaseEvent, reset_emission_counter
from crewai.tasks.task_output import TaskOutput

from export_listener import CrewAIBoundedExportListener


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate a bounded CrewAI NDJSON export artifact."
    )
    parser.add_argument(
        "--scenario",
        choices=("success", "failure", "mcp-success"),
        default="success",
        help="Synthetic scenario to emit through CrewAI's event bus.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="Path to the NDJSON file that will receive exported records.",
    )
    parser.add_argument(
        "--run-id",
        default=None,
        help="Optional run_id override. Defaults to scenario-specific IDs.",
    )
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="Allow overwriting the output file if it already exists.",
    )
    return parser.parse_args()


def _wait_for_handlers(event: BaseEvent, source: dict[str, str]) -> None:
    future = crewai_event_bus.emit(source, event)
    if future is not None:
        future.result(timeout=5.0)


def _reset_event_bus() -> None:
    """Clear existing global listeners so the sample stays self-contained."""
    with crewai_event_bus._rwlock.w_locked():
        crewai_event_bus._sync_handlers = {}
        crewai_event_bus._async_handlers = {}
        crewai_event_bus._handler_dependencies = {}
        crewai_event_bus._execution_plan_cache = {}


def _stamp(base: datetime, seconds: int) -> datetime:
    return base + timedelta(seconds=seconds)


def _task_output(raw: str) -> TaskOutput:
    return TaskOutput(
        description="Find one source",
        expected_output="One credible source",
        raw=raw,
        agent="researcher",
    )


def _emit_success(source: dict[str, str]) -> None:
    base = datetime(2026, 4, 6, 10, 14, 23, tzinfo=timezone.utc)
    _wait_for_handlers(
        CrewKickoffStartedEvent(
            event_id=f"{source['run_id']}-evt-01",
            timestamp=_stamp(base, 0),
            crew_name="research_crew",
            inputs={"topic": "paperclip safety"},
        ),
        source,
    )
    _wait_for_handlers(
        TaskStartedEvent(
            event_id=f"{source['run_id']}-evt-02",
            timestamp=_stamp(base, 1),
            task_id="task_1",
            task_name="research_task",
            agent_role="researcher",
            context="Find one source on paperclip safety.",
        ),
        source,
    )
    _wait_for_handlers(
        ToolUsageStartedEvent(
            event_id=f"{source['run_id']}-evt-03",
            timestamp=_stamp(base, 2),
            task_id="task_1",
            task_name="research_task",
            agent_role="researcher",
            tool_name="web_search",
            tool_args={"query": "paperclip safety credible source"},
        ),
        source,
    )
    _wait_for_handlers(
        ToolUsageFinishedEvent(
            event_id=f"{source['run_id']}-evt-04",
            timestamp=_stamp(base, 3),
            task_id="task_1",
            task_name="research_task",
            agent_role="researcher",
            tool_name="web_search",
            tool_args={"query": "paperclip safety credible source"},
            started_at=_stamp(base, 2),
            finished_at=_stamp(base, 3),
            output={"hits": 1, "top_domain": "nist.gov"},
            from_cache=False,
        ),
        source,
    )
    _wait_for_handlers(
        TaskCompletedEvent(
            event_id=f"{source['run_id']}-evt-05",
            timestamp=_stamp(base, 4),
            task_id="task_1",
            task_name="research_task",
            agent_role="researcher",
            output=_task_output("Found one credible source from nist.gov."),
        ),
        source,
    )
    _wait_for_handlers(
        CrewKickoffCompletedEvent(
            event_id=f"{source['run_id']}-evt-06",
            timestamp=_stamp(base, 5),
            crew_name="research_crew",
            output={"summary": "research complete"},
            total_tokens=42,
        ),
        source,
    )


def _emit_failure(source: dict[str, str]) -> None:
    base = datetime(2026, 4, 6, 10, 19, 23, tzinfo=timezone.utc)
    _wait_for_handlers(
        CrewKickoffStartedEvent(
            event_id=f"{source['run_id']}-evt-01",
            timestamp=_stamp(base, 0),
            crew_name="research_crew",
            inputs={"topic": "paperclip safety"},
        ),
        source,
    )
    _wait_for_handlers(
        TaskStartedEvent(
            event_id=f"{source['run_id']}-evt-02",
            timestamp=_stamp(base, 1),
            task_id="task_1",
            task_name="research_task",
            agent_role="researcher",
            context="Find one source on paperclip safety.",
        ),
        source,
    )
    _wait_for_handlers(
        ToolUsageStartedEvent(
            event_id=f"{source['run_id']}-evt-03",
            timestamp=_stamp(base, 2),
            task_id="task_1",
            task_name="research_task",
            agent_role="researcher",
            tool_name="web_search",
            tool_args={"query": "paperclip safety credible source"},
        ),
        source,
    )
    _wait_for_handlers(
        ToolUsageErrorEvent(
            event_id=f"{source['run_id']}-evt-04",
            timestamp=_stamp(base, 3),
            task_id="task_1",
            task_name="research_task",
            agent_role="researcher",
            tool_name="web_search",
            tool_args={"query": "paperclip safety credible source"},
            error="tool_timeout",
        ),
        source,
    )
    _wait_for_handlers(
        TaskFailedEvent(
            event_id=f"{source['run_id']}-evt-05",
            timestamp=_stamp(base, 4),
            task_id="task_1",
            task_name="research_task",
            agent_role="researcher",
            error="tool_timeout",
        ),
        source,
    )
    _wait_for_handlers(
        CrewKickoffFailedEvent(
            event_id=f"{source['run_id']}-evt-06",
            timestamp=_stamp(base, 5),
            crew_name="research_crew",
            error="task_1_failed",
        ),
        source,
    )


def _emit_mcp_success(source: dict[str, str]) -> None:
    base = datetime(2026, 4, 6, 10, 24, 23, tzinfo=timezone.utc)
    _wait_for_handlers(
        CrewKickoffStartedEvent(
            event_id=f"{source['run_id']}-evt-01",
            timestamp=_stamp(base, 0),
            crew_name="mcp_research_crew",
            inputs={"topic": "filesystem policy"},
        ),
        source,
    )
    _wait_for_handlers(
        MCPToolExecutionStartedEvent(
            event_id=f"{source['run_id']}-evt-02",
            timestamp=_stamp(base, 1),
            server_name="filesystem",
            transport_type="stdio",
            tool_name="read_file",
            tool_args={"path": "/tmp/example.txt"},
        ),
        source,
    )
    _wait_for_handlers(
        MCPToolExecutionCompletedEvent(
            event_id=f"{source['run_id']}-evt-03",
            timestamp=_stamp(base, 2),
            server_name="filesystem",
            transport_type="stdio",
            tool_name="read_file",
            tool_args={"path": "/tmp/example.txt"},
            result={"bytes": 128},
            started_at=_stamp(base, 1),
            completed_at=_stamp(base, 2),
        ),
        source,
    )
    _wait_for_handlers(
        CrewKickoffCompletedEvent(
            event_id=f"{source['run_id']}-evt-04",
            timestamp=_stamp(base, 3),
            crew_name="mcp_research_crew",
            output={"summary": "mcp run complete"},
            total_tokens=12,
        ),
        source,
    )


def main() -> int:
    args = _parse_args()
    if args.output.exists() and not args.overwrite:
        raise SystemExit(f"{args.output} already exists; pass --overwrite to replace it")

    scenario_run_id = {
        "success": "run_crewai_success",
        "failure": "run_crewai_failure",
        "mcp-success": "run_crewai_mcp_success",
    }[args.scenario]
    source = {
        "run_id": args.run_id or scenario_run_id,
        "source_kind": "crewai.synthetic_example",
    }

    reset_emission_counter()
    _reset_event_bus()
    listener = CrewAIBoundedExportListener(args.output)
    try:
        if args.scenario == "success":
            _emit_success(source)
        elif args.scenario == "failure":
            _emit_failure(source)
        else:
            _emit_mcp_success(source)
    finally:
        listener.close()

    print(f"Wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
