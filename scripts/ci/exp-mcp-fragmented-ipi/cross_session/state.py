#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict


SCHEMA_VERSION = "exp_mcp_fragmented_ipi_cross_session_state_v1"


@dataclass
class CrossSessionState:
    decay_runs: int
    sensitive_read_seen: bool
    remaining_runs: int

    def to_json(self) -> Dict[str, Any]:
        payload = {
            "schema_version": SCHEMA_VERSION,
            "decay_runs": self.decay_runs,
            "sensitive_read_seen": self.sensitive_read_seen,
            "remaining_runs": self.remaining_runs,
        }
        payload["state_snapshot_id"] = snapshot_id(payload)
        return payload


def snapshot_id(payload: Dict[str, Any]) -> str:
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def load_state(path: Path, decay_runs: int) -> CrossSessionState:
    if not path.exists():
        return CrossSessionState(decay_runs=decay_runs, sensitive_read_seen=False, remaining_runs=0)

    obj = json.loads(path.read_text(encoding="utf-8"))
    if obj.get("schema_version") != SCHEMA_VERSION:
        raise ValueError(f"unexpected state schema_version: {obj.get('schema_version')}")

    return CrossSessionState(
        decay_runs=int(obj["decay_runs"]),
        sensitive_read_seen=bool(obj["sensitive_read_seen"]),
        remaining_runs=int(obj["remaining_runs"]),
    )


def save_state(path: Path, state: CrossSessionState) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(state.to_json(), indent=2, sort_keys=True), encoding="utf-8")


def begin_session(state: CrossSessionState) -> Dict[str, Any]:
    active = state.remaining_runs > 0
    if active:
        state.remaining_runs = max(0, state.remaining_runs - 1)
    report = state.to_json()
    report["cross_session_window_active"] = active
    return report


def on_sensitive_read(state: CrossSessionState) -> None:
    state.sensitive_read_seen = True
    state.remaining_runs = int(state.decay_runs)
