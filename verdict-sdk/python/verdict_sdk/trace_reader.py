from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import Any, Dict, List

logger = logging.getLogger(__name__)


def read_events(path: Path) -> List[Dict[str, Any]]:
    """Reads a JSONL trace file, ignoring malformed lines."""
    events = []
    if not path.exists():
        return []

    with open(path, "r", encoding="utf-8") as f:
        for i, line in enumerate(f):
            line = line.strip()
            if not line:
                continue
            try:
                events.append(json.loads(line))
            except json.JSONDecodeError:
                logger.warning(f"Malformed JSON at {path}:{i+1}")
                continue
    return events


def group_by_episode(events: List[Dict[str, Any]]) -> Dict[str, List[Dict[str, Any]]]:
    """
    Groups events by 'episode_id' found in 'start'/'episode_start' events or 'meta'.
    Fallback: If no episode structure, return {'default': events}.

    Assumption: Trace assumes single-threaded episodes or explicitly tagged events.
    """
    # Simple grouping for v0.7.0 MVP:
    # 1. Look for episode_start/end markers
    # 2. Or meta.episode_id

    # For now, if we assume the trace is ONE episode (Replay mode standard):
    return {"default": events}


def extract_last_model_content(events: List[Dict[str, Any]]) -> str:
    """Finds the content of the LAST event with kind='model' or type='generation'."""
    for e in reversed(events):
        # V2 schema: kind="model", content="..."
        if e.get("kind") == "model" and "content" in e:
            return str(e["content"])
        # V1 schema / various: type="generation"
        if e.get("type") == "generation" and "content" in e:
            return str(e["content"])
    return ""


def extract_tool_calls(events: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
    """Returns list of tool call objects/events."""
    calls = []
    for e in events:
        # V2: type="tool_call", tool_name="..."
        if e.get("type") == "tool_call":
            calls.append(e)
            continue

        # V1: maybe inside a step?
        # For v0.7.0 we enforce flattened V2 events

    return calls
