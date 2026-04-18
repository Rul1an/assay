"""Artifact emission helpers for the paused approval pattern."""

from __future__ import annotations

from typing import Any, Mapping

from ._common import DEFAULT_SCHEMA, DEFAULT_SURFACE
from .validate import validate_pause_artifact


def emit_pause_artifact(
    captured: Mapping[str, Any],
    *,
    framework: str,
    resume_state_ref: str,
    schema: str = DEFAULT_SCHEMA,
    surface: str = DEFAULT_SURFACE,
    validate: bool = True,
) -> dict[str, Any]:
    """Emit the canonical pause-only artifact from captured pause-state data."""

    artifact = {
        "schema": schema,
        "framework": framework,
        "surface": surface,
        "timestamp": captured["timestamp"],
        "pause_reason": captured["pause_reason"],
        "interruptions": captured["interruptions"],
        "resume_state_ref": resume_state_ref,
    }

    for field_name in (
        "active_agent_ref",
        "last_agent_ref",
        "metadata_ref",
        "policy_snapshot_hash",
        "policy_decisions",
    ):
        if field_name in captured:
            artifact[field_name] = captured[field_name]

    if validate:
        return validate_pause_artifact(artifact)
    return artifact
