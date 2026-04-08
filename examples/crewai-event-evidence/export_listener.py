"""Bounded CrewAI event export for the Assay sample."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
from typing import Any

from crewai.events import (
    BaseEventListener,
    CrewKickoffCompletedEvent,
    CrewKickoffFailedEvent,
    CrewKickoffStartedEvent,
    MCPToolExecutionCompletedEvent,
    MCPToolExecutionFailedEvent,
    MCPToolExecutionStartedEvent,
    TaskCompletedEvent,
    TaskFailedEvent,
    TaskStartedEvent,
    ToolUsageErrorEvent,
    ToolUsageFinishedEvent,
    ToolUsageStartedEvent,
)
from crewai.events.base_events import BaseEvent
from crewai.events.event_bus import CrewAIEventsBus


EXPORT_SCHEMA = "crewai.event.export.v1"


def _canonical_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True)


def _sha256(value: Any) -> str:
    payload = _canonical_json(_to_serializable(value)).encode("utf-8")
    return f"sha256:{hashlib.sha256(payload).hexdigest()}"


def _to_serializable(value: Any) -> Any:
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    if isinstance(value, dict):
        return {str(k): _to_serializable(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_to_serializable(v) for v in value]
    model_dump = getattr(value, "model_dump", None)
    if callable(model_dump):
        return model_dump(mode="json", exclude_none=True)
    to_json = getattr(value, "to_json", None)
    if callable(to_json):
        return to_json()
    return str(value)


def _iso8601(value: datetime | None) -> str | None:
    if value is None:
        return None
    if value.tzinfo is None:
        value = value.replace(tzinfo=timezone.utc)
    return value.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def _duration_ms(started_at: datetime | None, finished_at: datetime | None) -> int | None:
    if started_at is None or finished_at is None:
        return None
    return int((finished_at - started_at).total_seconds() * 1000)


def _get_source_value(source: Any, key: str) -> str | None:
    if isinstance(source, dict):
        value = source.get(key)
    else:
        value = getattr(source, key, None)
    if value is None:
        return None
    return str(value)


@dataclass
class _ExportFieldSet:
    record: dict[str, Any]

    def put(self, key: str, value: Any) -> None:
        if value is None:
            return
        if value == "":
            return
        self.record[key] = value


class CrewAIBoundedExportListener(BaseEventListener):
    """Write a bounded subset of CrewAI events to NDJSON."""

    def __init__(self, output_path: str | Path) -> None:
        self.output_path = Path(output_path)
        self.output_path.parent.mkdir(parents=True, exist_ok=True)
        self._stream = self.output_path.open("w", encoding="utf-8")
        super().__init__()

    def close(self) -> None:
        self._stream.close()

    def setup_listeners(self, event_bus: CrewAIEventsBus) -> None:
        for event_cls in (
            CrewKickoffStartedEvent,
            CrewKickoffCompletedEvent,
            CrewKickoffFailedEvent,
            TaskStartedEvent,
            TaskCompletedEvent,
            TaskFailedEvent,
            ToolUsageStartedEvent,
            ToolUsageFinishedEvent,
            ToolUsageErrorEvent,
            MCPToolExecutionStartedEvent,
            MCPToolExecutionCompletedEvent,
            MCPToolExecutionFailedEvent,
        ):
            event_bus.on(event_cls)(self._make_handler())

    def _make_handler(self) -> Callable[[Any, BaseEvent], None]:
        def handler(source: Any, event: BaseEvent) -> None:
            self._write_record(source, event)

        return handler

    def _write_record(self, source: Any, event: BaseEvent) -> None:
        record = self._record_from_event(source, event)
        self._stream.write(_canonical_json(record))
        self._stream.write("\n")
        self._stream.flush()

    def _record_from_event(self, source: Any, event: BaseEvent) -> dict[str, Any]:
        record: dict[str, Any] = {
            "schema": EXPORT_SCHEMA,
            "event_class": event.__class__.__name__,
            "event_type": event.type,
            "timestamp": _iso8601(event.timestamp),
            "run_id": _get_source_value(source, "run_id") or "unknown_run",
            "event_id": event.event_id,
            "parent_event_id": event.parent_event_id,
            "previous_event_id": event.previous_event_id,
            "triggered_by_event_id": event.triggered_by_event_id,
            "started_event_id": event.started_event_id,
            "emission_sequence": event.emission_sequence,
        }

        fields = _ExportFieldSet(record)
        fields.put("source_type", event.source_type)
        fields.put("source_fingerprint", event.source_fingerprint)
        fields.put("task_id", getattr(event, "task_id", None))
        fields.put("task_name", getattr(event, "task_name", None))
        fields.put("agent_id", getattr(event, "agent_id", None))
        fields.put("agent_role", getattr(event, "agent_role", None))
        fields.put("crew_name", getattr(event, "crew_name", None))

        if isinstance(event, CrewKickoffStartedEvent):
            fields.put("status", "started")
            if event.inputs is not None:
                fields.put("inputs_hash", _sha256(event.inputs))
        elif isinstance(event, CrewKickoffCompletedEvent):
            fields.put("status", "ok")
            fields.put("total_tokens", event.total_tokens)
            fields.put("output_hash", _sha256(event.output))
        elif isinstance(event, CrewKickoffFailedEvent):
            fields.put("status", "failed")
            fields.put("error", event.error)
        elif isinstance(event, TaskStartedEvent):
            fields.put("status", "started")
            fields.put("context_hash", _sha256(event.context))
        elif isinstance(event, TaskCompletedEvent):
            fields.put("status", "ok")
            fields.put("output_hash", _sha256(event.output))
        elif isinstance(event, TaskFailedEvent):
            fields.put("status", "failed")
            fields.put("error", event.error)
        elif isinstance(event, ToolUsageStartedEvent):
            fields.put("status", "started")
            fields.put("tool_name", event.tool_name)
            fields.put("tool_args_hash", _sha256(event.tool_args))
        elif isinstance(event, ToolUsageFinishedEvent):
            fields.put("status", "ok")
            fields.put("tool_name", event.tool_name)
            fields.put("tool_args_hash", _sha256(event.tool_args))
            fields.put("duration_ms", _duration_ms(event.started_at, event.finished_at))
            fields.put("from_cache", event.from_cache)
            fields.put("output_hash", _sha256(event.output))
        elif isinstance(event, ToolUsageErrorEvent):
            fields.put("status", "failed")
            fields.put("tool_name", event.tool_name)
            fields.put("tool_args_hash", _sha256(event.tool_args))
            fields.put("error", str(event.error))
        elif isinstance(event, MCPToolExecutionStartedEvent):
            fields.put("status", "started")
            fields.put("server_name", event.server_name)
            fields.put("transport_type", event.transport_type)
            fields.put("tool_name", event.tool_name)
            fields.put("tool_args_hash", _sha256(event.tool_args))
        elif isinstance(event, MCPToolExecutionCompletedEvent):
            fields.put("status", "ok")
            fields.put("server_name", event.server_name)
            fields.put("transport_type", event.transport_type)
            fields.put("tool_name", event.tool_name)
            fields.put("tool_args_hash", _sha256(event.tool_args))
            fields.put(
                "duration_ms",
                _duration_ms(event.started_at, event.completed_at),
            )
            fields.put("result_hash", _sha256(event.result))
        elif isinstance(event, MCPToolExecutionFailedEvent):
            fields.put("status", "failed")
            fields.put("server_name", event.server_name)
            fields.put("transport_type", event.transport_type)
            fields.put("tool_name", event.tool_name)
            fields.put("tool_args_hash", _sha256(event.tool_args))
            fields.put("error", event.error)
            fields.put("error_type", event.error_type)

        return record
